//! End-to-end `DoGet` flows: a real Flight server on a real socket, a real
//! session token, and a real PostgreSQL behind it.
//!
//! Until #1001 these six tests carried numbered "1. Create `FlightTicket` 2. Send
//! `DoGet` request 3. Receive schema message 4. Receive data batches" doc comments
//! and called **no Flight RPC at all** — each reduced to
//! `assert!(service.schema_registry().contains("ta_users"))`, which
//! `register_defaults()` makes true for any service, with or without a database.
//! Every one of them created a fixture database it never queried. The `do_get`
//! path could have been broken in every way they named and all six passed.
//!
//! Each test now performs the flow its name describes and asserts on the rows that
//! come back.
//!
//! `#[ignore]` — needs `DATABASE_URL` (a real PostgreSQL, with permission to
//! `CREATE DATABASE`). Named explicitly by the Dagger `integration` leg (`observers` suite, which
//! binds a Postgres),
//! so these either run or the leg fails; they can no longer self-skip into a false
//! green. Run with:
//! `cargo test -p fraiseql-arrow --all-features --test flight_e2e_test --
//! --ignored --test-threads=1`.
#![allow(clippy::unwrap_used, clippy::print_stdout, clippy::print_stderr)] // Reason: test code, panics are acceptable

use std::{collections::HashMap, sync::Arc};

use arrow::record_batch::RecordBatch;
use arrow_flight::{Ticket, flight_service_client::FlightServiceClient};
use fraiseql_arrow::{FlightTicket, flight_server::FraiseQLFlightService};
use sqlx::postgres::PgPoolOptions;
use tonic::transport::{Endpoint, Server};

const TEST_FLIGHT_SECRET: &str = "flight-e2e-session-secret";

/// Test database setup and teardown (reused from `flight_integration.rs`).
struct TestDb {
    pool:          sqlx::PgPool,
    database_name: String,
}

impl TestDb {
    /// Create a test database and set up tables.
    ///
    /// Returns `Ok(None)` when `DATABASE_URL` is not set. These tests are
    /// `#[ignore]`d and named explicitly by the leg, so this is the local
    /// convenience path, not a way for CI to skip them.
    async fn setup() -> Result<Option<Self>, Box<dyn std::error::Error>> {
        let Some(db_url) = fraiseql_test_support::try_database_url() else {
            eprintln!("Skipping: DATABASE_URL not set");
            return Ok(None);
        };

        let pool = PgPoolOptions::new().max_connections(1).connect(&db_url).await?;

        let test_db_name =
            format!("fraiseql_arrow_e2e_{}", uuid::Uuid::new_v4().to_string().replace('-', "_"));

        sqlx::query(&format!("CREATE DATABASE \"{}\"", test_db_name))
            .execute(&pool)
            .await?;

        let test_db_url = match db_url.rsplit_once('/') {
            Some((base, _)) => format!("{}/{}", base, test_db_name),
            None => format!("{}/{}", db_url, test_db_name),
        };
        let test_pool = PgPoolOptions::new().max_connections(5).connect(&test_db_url).await?;

        Self::create_tables(&test_pool).await?;

        Ok(Some(TestDb {
            pool:          test_pool,
            database_name: test_db_name,
        }))
    }

    /// Create `ta_users` and `ta_orders` with the columns the registry's
    /// pre-compiled Arrow schemas declare — the conversion step matches rows to
    /// that schema by name, so the table must carry those columns.
    async fn create_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            CREATE TABLE ta_users (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                email TEXT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL,
                source_updated_at TIMESTAMPTZ DEFAULT NOW()
            )
            ",
        )
        .execute(pool)
        .await?;

        sqlx::query(
            r"
            CREATE TABLE ta_orders (
                id TEXT PRIMARY KEY,
                total NUMERIC(12, 2) NOT NULL,
                created_at TIMESTAMPTZ NOT NULL,
                customer_name TEXT NOT NULL,
                source_updated_at TIMESTAMPTZ DEFAULT NOW()
            )
            ",
        )
        .execute(pool)
        .await?;

        sqlx::query(
            r"
            INSERT INTO ta_users (id, name, email, created_at)
            VALUES
                ('user-1', 'Alice Johnson', 'alice@example.com', NOW()),
                ('user-2', 'Bob Smith', 'bob@example.com', NOW() - INTERVAL '1 day'),
                ('user-3', 'Charlie Brown', 'charlie@example.com', NOW() - INTERVAL '2 days'),
                ('user-4', 'Diana Prince', 'diana@example.com', NOW() - INTERVAL '3 days'),
                ('user-5', 'Eve Wilson', 'eve@example.com', NOW() - INTERVAL '4 days')
            ",
        )
        .execute(pool)
        .await?;

        sqlx::query(
            r"
            INSERT INTO ta_orders (id, total, created_at, customer_name)
            VALUES
                ('order-1', 99.99, NOW(), 'Alice Johnson'),
                ('order-2', 149.99, NOW() - INTERVAL '1 day', 'Bob Smith'),
                ('order-3', 199.99, NOW() - INTERVAL '2 days', 'Charlie Brown'),
                ('order-4', 299.99, NOW() - INTERVAL '3 days', 'Diana Prince'),
                ('order-5', 399.99, NOW() - INTERVAL '4 days', 'Eve Wilson')
            ",
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    fn connection_string(&self) -> String {
        let db_url = fraiseql_test_support::database_url();
        db_url.rsplit_once('/').map_or_else(
            || format!("{}/{}", db_url, self.database_name),
            |(base, _)| format!("{}/{}", base, self.database_name),
        )
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        let db_name = self.database_name.clone();
        let Some(default_url) = fraiseql_test_support::try_database_url() else {
            return;
        };

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                if let Ok(pool) =
                    PgPoolOptions::new().max_connections(1).connect(&default_url).await
                {
                    let _ = sqlx::query(&format!(
                        "SELECT pg_terminate_backend(pg_stat_activity.pid) FROM pg_stat_activity WHERE pg_stat_activity.datname = '{}' AND pid <> pg_backend_pid()",
                        db_name
                    ))
                    .execute(&pool)
                    .await;

                    let _ = sqlx::query(&format!("DROP DATABASE \"{}\"", db_name))
                        .execute(&pool)
                        .await;
                }
            });
        });
    }
}

// ------------------------------------------------------------- flight harness

/// The adapter shape the shipped server passes to the Flight service.
struct TestFlightAdapter {
    inner: fraiseql_core::db::PostgresAdapter,
}

#[async_trait::async_trait]
impl fraiseql_arrow::ArrowDatabaseAdapter for TestFlightAdapter {
    async fn execute_raw_query(
        &self,
        sql: &str,
    ) -> fraiseql_arrow::db::DatabaseResult<Vec<HashMap<String, serde_json::Value>>> {
        use fraiseql_core::db::traits::DatabaseAdapter as _;
        self.inner
            .execute_raw_query(sql)
            .await
            .map_err(|e| fraiseql_arrow::db::DatabaseError::new(e.to_string()))
    }
}

async fn flight_adapter(conn_string: &str) -> Arc<dyn fraiseql_arrow::ArrowDatabaseAdapter> {
    let pg = fraiseql_core::db::PostgresAdapter::new(conn_string).await.unwrap();
    Arc::new(TestFlightAdapter { inner: pg })
}

fn session_token() -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    struct Claims {
        sub:          String,
        exp:          i64,
        iat:          i64,
        scopes:       Vec<String>,
        session_type: String,
    }

    let now = chrono::Utc::now();
    encode(
        &Header::new(Algorithm::HS256),
        &Claims {
            sub:          "e2e-user".to_string(),
            exp:          (now + chrono::Duration::minutes(5)).timestamp(),
            iat:          now.timestamp(),
            scopes:       vec!["user".to_string()],
            session_type: "flight".to_string(),
        },
        &EncodingKey::from_secret(TEST_FLIGHT_SECRET.as_bytes()),
    )
    .unwrap()
}

/// Serve `service` on an ephemeral port; returns the endpoint URL.
async fn serve(service: FraiseQLFlightService) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        Server::builder()
            .add_service(service.into_server())
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    tokio::task::yield_now().await;
    format!("http://127.0.0.1:{}", addr.port())
}

async fn connect(addr: &str) -> FlightServiceClient<tonic::transport::Channel> {
    let channel = Endpoint::from_shared(addr.to_string()).unwrap().connect().await.unwrap();
    FlightServiceClient::new(channel)
}

/// Send one authenticated `do_get` and decode the whole stream into batches.
///
/// The first `FlightData` is the schema message, the rest are batches — this is
/// the "receive schema message → receive data batches" the old doc comments
/// described and never performed.
async fn do_get(addr: &str, ticket: &FlightTicket) -> Result<Vec<RecordBatch>, tonic::Status> {
    let mut client = connect(addr).await;
    let mut request = tonic::Request::new(Ticket {
        ticket: ticket.encode().unwrap().into(),
    });
    request
        .metadata_mut()
        .insert("authorization", format!("Bearer {}", session_token()).parse().unwrap());

    let mut stream = client.do_get(request).await?.into_inner();
    let mut frames = Vec::new();
    while let Some(data) = stream.message().await? {
        frames.push(data);
    }
    Ok(arrow_flight::utils::flight_data_to_batches(&frames)
        .expect("the server's own stream must decode as Arrow"))
}

fn total_rows(batches: &[RecordBatch]) -> usize {
    batches.iter().map(RecordBatch::num_rows).sum()
}

/// Collect one string column across all batches.
fn column_values(batches: &[RecordBatch], column: &str) -> Vec<String> {
    use arrow::array::{Array as _, StringArray};
    let mut out = Vec::new();
    for batch in batches {
        let Some(idx) = batch.schema().index_of(column).ok() else {
            continue;
        };
        let array = batch
            .column(idx)
            .as_any()
            .downcast_ref::<StringArray>()
            .expect("column is Utf8");
        for i in 0..array.len() {
            out.push(array.value(i).to_string());
        }
    }
    out
}

fn view_ticket(view: &str, limit: Option<usize>) -> FlightTicket {
    FlightTicket::OptimizedView {
        view: view.to_string(),
        filter: None,
        order_by: None,
        limit,
        offset: None,
    }
}

// -------------------------------------------------------------------- tests

/// Complete `DoGet` flow: ticket → request → schema message → data batches →
/// the rows that are actually in the table.
#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn do_get_optimized_view_returns_the_rows_in_the_table() {
    let Some(db) = TestDb::setup().await.unwrap() else {
        return;
    };
    let service = FraiseQLFlightService::new_with_db(flight_adapter(&db.connection_string()).await)
        .with_session_secret(TEST_FLIGHT_SECRET);
    let addr = serve(service).await;

    let batches = do_get(&addr, &view_ticket("ta_users", None)).await.unwrap();

    assert_eq!(total_rows(&batches), 5, "ta_users holds five seeded rows");
    let mut ids = column_values(&batches, "id");
    ids.sort();
    assert_eq!(ids, ["user-1", "user-2", "user-3", "user-4", "user-5"]);
    let names = column_values(&batches, "name");
    assert!(
        names.contains(&"Alice Johnson".to_string()),
        "row values must survive the round trip"
    );
}

/// A cache **hit** serves the earlier result without touching the database.
///
/// Proven by deleting every row between the two requests: with a cold cache the
/// second request would return nothing, so five rows can only have come from the
/// cache.
#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn do_get_serves_a_repeat_request_from_cache() {
    let Some(db) = TestDb::setup().await.unwrap() else {
        return;
    };
    let service =
        FraiseQLFlightService::new_with_cache(flight_adapter(&db.connection_string()).await, 60)
            .with_session_secret(TEST_FLIGHT_SECRET);
    let addr = serve(service).await;
    let ticket = view_ticket("ta_users", None);

    let first = do_get(&addr, &ticket).await.unwrap();
    assert_eq!(total_rows(&first), 5);

    sqlx::query("DELETE FROM ta_users").execute(&db.pool).await.unwrap();

    let second = do_get(&addr, &ticket).await.unwrap();
    assert_eq!(
        total_rows(&second),
        5,
        "the repeat request must be served from cache — the table is now empty"
    );
}

/// A cache **miss** goes to the database.
///
/// The cache is keyed by SQL text, so a different `limit` is a different key. The
/// row inserted after the first request is invisible to the cached entry and must
/// appear under the new one.
#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn a_differently_keyed_request_misses_the_cache_and_reads_the_database() {
    let Some(db) = TestDb::setup().await.unwrap() else {
        return;
    };
    let service =
        FraiseQLFlightService::new_with_cache(flight_adapter(&db.connection_string()).await, 60)
            .with_session_secret(TEST_FLIGHT_SECRET);
    let addr = serve(service).await;

    let warm = do_get(&addr, &view_ticket("ta_users", Some(5))).await.unwrap();
    assert_eq!(total_rows(&warm), 5);

    sqlx::query(
        "INSERT INTO ta_users (id, name, email, created_at)
         VALUES ('user-6', 'Frank Castle', 'frank@example.com', NOW())",
    )
    .execute(&db.pool)
    .await
    .unwrap();

    let cached = do_get(&addr, &view_ticket("ta_users", Some(5))).await.unwrap();
    assert_eq!(total_rows(&cached), 5, "the identical request is still the cached one");

    let fresh = do_get(&addr, &view_ticket("ta_users", Some(6))).await.unwrap();
    assert_eq!(total_rows(&fresh), 6, "a different key must miss and re-read the database");
    assert!(column_values(&fresh, "id").contains(&"user-6".to_string()));
}

/// A `BatchedQueries` ticket returns every query's rows in one stream.
///
/// Raw SQL is disabled by default, so this enables it explicitly — which is also
/// the point: the surface exists only when an operator opts in.
///
/// ⚠ **Single column, deliberately (#1002).** The inferred schema takes its field
/// order from a `HashMap` iteration, so two queries with an identical column set
/// can produce differently-*ordered* schemas and be refused by the #717
/// heterogeneous-schema guard — intermittently, since the order varies per process.
/// A one-column projection has only one possible order, so this test measures the
/// batched-stream path rather than that lottery. Widen it once #1002 lands.
#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn batched_queries_stream_every_query_in_one_response() {
    let Some(db) = TestDb::setup().await.unwrap() else {
        return;
    };
    let service = FraiseQLFlightService::new_with_db(flight_adapter(&db.connection_string()).await)
        .with_session_secret(TEST_FLIGHT_SECRET)
        .with_raw_sql_enabled();
    let addr = serve(service).await;

    let ticket = FlightTicket::BatchedQueries {
        queries: vec![
            "SELECT id FROM ta_users ORDER BY id LIMIT 2".to_string(),
            "SELECT id FROM ta_users ORDER BY id OFFSET 2".to_string(),
        ],
    };

    let batches = do_get(&addr, &ticket).await.unwrap();

    assert_eq!(total_rows(&batches), 5, "both queries' rows must reach the client");
    let mut ids = column_values(&batches, "id");
    ids.sort();
    assert_eq!(ids, ["user-1", "user-2", "user-3", "user-4", "user-5"]);
}

/// A large result set streams in full.
///
/// Note the honest scope: `execute_optimized_view` derives its batch size from the
/// request's `limit` (`limit.unwrap_or(10_000)`), so an `OptimizedView` response is
/// a single batch by construction — the old body's claim to "verify streaming
/// produces multiple batches" was untestable through this path as well as untested.
/// What is real, and what this asserts, is that every one of 1 500 rows arrives.
#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn a_large_result_set_streams_every_row() {
    let Some(db) = TestDb::setup().await.unwrap() else {
        return;
    };
    sqlx::query(
        "INSERT INTO ta_users (id, name, email, created_at)
         SELECT 'bulk-' || g, 'Bulk User ' || g, 'bulk' || g || '@example.com', NOW()
         FROM generate_series(1, 1495) AS g",
    )
    .execute(&db.pool)
    .await
    .unwrap();

    let service = FraiseQLFlightService::new_with_db(flight_adapter(&db.connection_string()).await)
        .with_session_secret(TEST_FLIGHT_SECRET);
    let addr = serve(service).await;

    let batches = do_get(&addr, &view_ticket("ta_users", None)).await.unwrap();

    assert_eq!(total_rows(&batches), 1500, "every row must arrive, none dropped by chunking");
    assert_eq!(column_values(&batches, "id").len(), 1500);
}

/// Ten concurrent clients each get a correct, complete result.
#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn concurrent_do_get_requests_each_return_the_full_result() {
    let Some(db) = TestDb::setup().await.unwrap() else {
        return;
    };
    let service = FraiseQLFlightService::new_with_db(flight_adapter(&db.connection_string()).await)
        .with_session_secret(TEST_FLIGHT_SECRET);
    let addr = serve(service).await;

    let handles: Vec<_> = (0..10)
        .map(|_| {
            let addr = addr.clone();
            tokio::spawn(async move { do_get(&addr, &view_ticket("ta_orders", None)).await })
        })
        .collect();

    for handle in handles {
        let batches = handle.await.unwrap().expect("every concurrent request must succeed");
        assert_eq!(total_rows(&batches), 5, "no request may see a partial or crossed result");
    }
}
