//! Error handling on the Arrow Flight `do_get` path, exercised over a real socket
//! against a real PostgreSQL.
//!
//! Until #1001 these tests asserted schema-registry membership —
//! `assert!(service.schema_registry().contains("ta_users"))`, true for any service
//! with or without a database, since `register_defaults()` hardcodes it — while
//! their doc comments claimed to verify that "request fails gracefully without
//! panicking", "appropriate error is returned to client" and so on. No Flight RPC
//! was ever issued and no error was ever produced. Each one still created a fixture
//! database it never queried.
//!
//! Each test now provokes the failure it names and asserts on the `Status` the
//! client actually receives. `test_ipc_encoding_failure` was **deleted** rather than
//! rewritten: there is no way to force an IPC encoding failure from outside the
//! service, so the honest options were a lie or nothing.
//!
//! `#[ignore]` — needs `DATABASE_URL`. Named explicitly by the Dagger
//! `integration` leg (`observers` suite, which binds a Postgres). Run with:
//! `cargo test -p fraiseql-arrow --all-features --test flight_error_handling_test --
//! --ignored --test-threads=1`.
#![allow(clippy::unwrap_used, clippy::print_stdout, clippy::print_stderr)] // Reason: test code, panics are acceptable

use std::{collections::HashMap, sync::Arc};

use arrow_flight::{Ticket, flight_service_client::FlightServiceClient};
use fraiseql_arrow::{FlightTicket, flight_server::FraiseQLFlightService};
use sqlx::postgres::PgPoolOptions;
use tonic::{
    Code,
    transport::{Endpoint, Server},
};

const TEST_FLIGHT_SECRET: &str = "flight-error-handling-session-secret";

/// Test database setup and teardown.
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
            format!("fraiseql_arrow_err_{}", uuid::Uuid::new_v4().to_string().replace('-', "_"));
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
            INSERT INTO ta_users (id, name, email, created_at)
            VALUES
                ('user-1', 'Alice Johnson', 'alice@example.com', NOW()),
                ('user-2', 'Bob Smith', 'bob@example.com', NOW() - INTERVAL '1 day')
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

async fn flight_adapter(conn: &str) -> Arc<dyn fraiseql_arrow::ArrowDatabaseAdapter> {
    let pg = fraiseql_core::db::PostgresAdapter::new(conn).await.unwrap();
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
            sub:          "error-handling-user".to_string(),
            exp:          (now + chrono::Duration::minutes(5)).timestamp(),
            iat:          now.timestamp(),
            scopes:       vec!["user".to_string()],
            session_type: "flight".to_string(),
        },
        &EncodingKey::from_secret(TEST_FLIGHT_SECRET.as_bytes()),
    )
    .unwrap()
}

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

/// Issue one authenticated `do_get` and drain the stream, returning the first
/// error the client sees (errors can surface at call time or mid-stream).
async fn do_get(addr: &str, ticket: &FlightTicket) -> Result<usize, tonic::Status> {
    let channel = Endpoint::from_shared(addr.to_string()).unwrap().connect().await.unwrap();
    let mut client = FlightServiceClient::new(channel);

    let mut request = tonic::Request::new(Ticket {
        ticket: ticket.encode().unwrap().into(),
    });
    request
        .metadata_mut()
        .insert("authorization", format!("Bearer {}", session_token()).parse().unwrap());

    let mut stream = client.do_get(request).await?.into_inner();
    let mut frames = 0;
    while let Some(_data) = stream.message().await? {
        frames += 1;
    }
    Ok(frames)
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

/// A ticket naming a view the registry does not know is refused with `NotFound`,
/// the message names the view, and the service serves the next request normally.
#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn an_unknown_view_is_refused_and_the_service_survives_it() {
    let Some(db) = TestDb::setup().await.unwrap() else {
        return;
    };
    let service = FraiseQLFlightService::new_with_db(flight_adapter(&db.connection_string()).await)
        .with_session_secret(TEST_FLIGHT_SECRET);
    let addr = serve(service).await;

    let status = do_get(&addr, &view_ticket("nonexistent_view", None))
        .await
        .expect_err("an unknown view must be refused");

    assert_eq!(status.code(), Code::NotFound, "got: {status:?}");
    assert!(
        status.message().contains("nonexistent_view"),
        "the error must name the view the client asked for, got: {}",
        status.message()
    );

    // Recovery: the same server still answers a valid request.
    let frames = do_get(&addr, &view_ticket("ta_users", None)).await.unwrap();
    assert!(frames >= 2, "schema message plus at least one batch");
}

/// When the database is gone, the client gets an error — not a panic, not an
/// empty-but-successful stream that reads as "no rows".
#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn a_failing_database_surfaces_as_an_error_not_an_empty_success() {
    let Some(db) = TestDb::setup().await.unwrap() else {
        return;
    };
    let service = FraiseQLFlightService::new_with_db(flight_adapter(&db.connection_string()).await)
        .with_session_secret(TEST_FLIGHT_SECRET);
    let addr = serve(service).await;

    // Take the table away underneath the running service.
    sqlx::query("DROP TABLE ta_users").execute(&db.pool).await.unwrap();

    let status = do_get(&addr, &view_ticket("ta_users", None))
        .await
        .expect_err("a query against a missing table must not answer success");

    assert_eq!(status.code(), Code::Internal, "got: {status:?}");
    assert!(
        status.message().contains("Database query failed"),
        "the error must say the database failed, got: {}",
        status.message()
    );
}

/// `limit = 0` is refused up front (H38) rather than reaching the chunker, where a
/// zero-sized chunk used to panic.
#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn a_zero_limit_is_refused_before_it_reaches_the_chunker() {
    let Some(db) = TestDb::setup().await.unwrap() else {
        return;
    };
    let service = FraiseQLFlightService::new_with_db(flight_adapter(&db.connection_string()).await)
        .with_session_secret(TEST_FLIGHT_SECRET);
    let addr = serve(service).await;

    let status = do_get(&addr, &view_ticket("ta_users", Some(0)))
        .await
        .expect_err("limit = 0 is a meaningless request and must be refused");

    assert_eq!(status.code(), Code::InvalidArgument, "got: {status:?}");
    assert!(
        status.message().contains("greater than 0"),
        "the error must tell the client what is wrong with the limit, got: {}",
        status.message()
    );
}

/// An empty `BatchedQueries` ticket is refused rather than answered with an empty
/// stream.
#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn an_empty_batched_queries_ticket_is_refused() {
    let Some(db) = TestDb::setup().await.unwrap() else {
        return;
    };
    let service = FraiseQLFlightService::new_with_db(flight_adapter(&db.connection_string()).await)
        .with_session_secret(TEST_FLIGHT_SECRET)
        .with_raw_sql_enabled();
    let addr = serve(service).await;

    let status = do_get(&addr, &FlightTicket::BatchedQueries { queries: vec![] })
        .await
        .expect_err("an empty batch is not a request");

    assert_eq!(status.code(), Code::InvalidArgument, "got: {status:?}");
}

/// One bad query fails the **whole** batch — the client is never handed a partial
/// result it cannot tell apart from a complete one.
///
/// The old doc comment claimed the opposite ("partial results are streamed
/// correctly", "error doesn't break the entire batch"). It asserted neither, and
/// the service does neither: a Flight stream carries one schema header, so there is
/// no way to signal "query 2 of 3 failed" mid-stream. All-or-nothing is the correct
/// behaviour and this pins it.
#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn one_bad_query_fails_the_whole_batch_rather_than_returning_part_of_it() {
    let Some(db) = TestDb::setup().await.unwrap() else {
        return;
    };
    let service = FraiseQLFlightService::new_with_db(flight_adapter(&db.connection_string()).await)
        .with_session_secret(TEST_FLIGHT_SECRET)
        .with_raw_sql_enabled();
    let addr = serve(service).await;

    let ticket = FlightTicket::BatchedQueries {
        queries: vec![
            "SELECT id FROM ta_users ORDER BY id".to_string(),
            "SELECT id FROM table_that_does_not_exist".to_string(),
        ],
    };

    let status = do_get(&addr, &ticket)
        .await
        .expect_err("a batch containing a failing query must fail as a whole");

    assert_eq!(status.code(), Code::Internal, "got: {status:?}");
    assert!(
        status.message().contains("Database query failed"),
        "the error must name the database failure, got: {}",
        status.message()
    );
}
