//! #953 — the Flight `do_exchange` Upload write gate, against a real PostgreSQL.
//!
//! Before this gate, any caller holding a valid Flight session token could send
//! `RequestType::Upload { table, batch }` with an **arbitrary client-supplied table
//! name** and have the rows land: `handle_upload` went straight from session
//! validation to `build_insert_query` to `adapter.execute_raw_query`, with no table
//! allow-list, no tenant scoping, no authorizer, no audit event and no change-log
//! row. The escaping was correct, so this was never SQL injection — it was an
//! unauthorized-write surface that bypassed every control the mutation pipeline
//! enforces, including writes to `_system`, audit and outbox tables.
//!
//! These tests drive the **real** service over a **real** socket with a **real**
//! session token — not the handler in isolation — because the defect is reachable
//! in the shipped binary (`create_flight_service` passes the live adapter) and only
//! the mounted path proves it.
//!
//! `#[ignore]` — needs `DATABASE_URL` (a real Postgres). Named explicitly by the
//! Dagger `integration` leg (`observers` suite, which binds a Postgres), so it either
//! runs or the leg fails; it can
//! never self-skip into a false green. Run with:
//! `cargo test -p fraiseql-arrow --all-features --test flight_upload_gate_pg --
//! --ignored --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)] // Reason: test code, panics acceptable
#![allow(clippy::items_after_statements)] // Reason: test helper items defined near use site

use std::{collections::HashMap, sync::Arc};

use arrow::{
    array::{ArrayRef, StringArray},
    datatypes::{DataType, Field, Schema},
    record_batch::RecordBatch,
};
use arrow_flight::{FlightData, flight_service_client::FlightServiceClient};
use fraiseql_arrow::{
    db::{ArrowDatabaseAdapter, DatabaseError, DatabaseResult},
    exchange_protocol::{ExchangeMessage, RequestType},
    flight_server::FraiseQLFlightService,
};
use sqlx::{
    Row,
    postgres::{PgPool, PgPoolOptions},
};
use tonic::transport::{Endpoint, Server};

const TEST_FLIGHT_SECRET: &str = "flight-upload-gate-session-secret";

// ---------------------------------------------------------------- fixtures

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for --ignored runs");
    PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap()
}

/// A table this test owns outright, so a run can never depend on — or damage —
/// the shared fixtures the other suites assert on.
fn unique_table(kind: &str) -> String {
    format!("ta_upload_gate_{}_{}", kind, uuid::Uuid::new_v4().simple())
}

async fn create_table(pool: &PgPool, table: &str) {
    sqlx::query(&format!("CREATE TABLE \"{table}\" (id TEXT PRIMARY KEY, note TEXT)"))
        .execute(pool)
        .await
        .unwrap();
}

async fn drop_table(pool: &PgPool, table: &str) {
    let _ = sqlx::query(&format!("DROP TABLE IF EXISTS \"{table}\"")).execute(pool).await;
}

async fn row_count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query(&format!("SELECT count(*) AS n FROM \"{table}\""))
        .fetch_one(pool)
        .await
        .unwrap()
        .get::<i64, _>("n")
}

/// One row: `('gate-1', 'planted by an unauthorized Upload')`.
fn one_row_batch() -> RecordBatch {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("note", DataType::Utf8, true),
    ]));
    let columns: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from(vec!["gate-1"])),
        Arc::new(StringArray::from(vec!["planted by an unauthorized Upload"])),
    ];
    RecordBatch::try_new(schema, columns).unwrap()
}

/// Arrow IPC stream bytes, the encoding `decode_upload_batch` expects.
fn encode_batch(batch: &RecordBatch) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut writer =
            arrow::ipc::writer::StreamWriter::try_new(&mut buf, batch.schema().as_ref()).unwrap();
        writer.write(batch).unwrap();
        writer.finish().unwrap();
    }
    buf
}

/// The adapter the shipped server passes to the Flight service: a real
/// PostgreSQL connection, no sandbox and no allow-list of its own.
struct PgFlightAdapter {
    inner: fraiseql_core::db::PostgresAdapter,
}

#[async_trait::async_trait]
impl ArrowDatabaseAdapter for PgFlightAdapter {
    async fn execute_raw_query(
        &self,
        sql: &str,
    ) -> DatabaseResult<Vec<HashMap<String, serde_json::Value>>> {
        use fraiseql_core::db::traits::DatabaseAdapter as _;
        self.inner
            .execute_raw_query(sql)
            .await
            .map_err(|e| DatabaseError::new(e.to_string()))
    }
}

async fn flight_adapter() -> Arc<dyn ArrowDatabaseAdapter> {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for --ignored runs");
    let inner = fraiseql_core::db::PostgresAdapter::new(&url).await.unwrap();
    Arc::new(PgFlightAdapter { inner })
}

/// A session token of exactly the shape `validate_session_token` accepts —
/// this is the "valid Flight session token" the finding turns on.
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
    let claims = Claims {
        // An ordinary user, deliberately: no admin scope, nothing privileged.
        sub:          "upload-gate-user".to_string(),
        exp:          (now + chrono::Duration::minutes(5)).timestamp(),
        iat:          now.timestamp(),
        scopes:       vec!["user".to_string()],
        session_type: "flight".to_string(),
    };

    encode(
        &Header::new(Algorithm::HS256),
        &claims,
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

/// Drive one `Upload` through `do_exchange` and return the server's
/// `ExchangeMessage::Response` result — `Ok(message)` or `Err(message)`.
async fn upload(addr: &str, table: &str, batch: &RecordBatch) -> Result<String, String> {
    let channel = Endpoint::from_shared(addr.to_string()).unwrap().connect().await.unwrap();
    let mut client = FlightServiceClient::new(channel);

    let request_msg = ExchangeMessage::Request {
        correlation_id: "gate-corr-1".to_string(),
        request_type:   RequestType::Upload {
            table: table.to_string(),
            batch: encode_batch(batch),
        },
    };
    let outbound = tokio_stream::iter(vec![FlightData {
        app_metadata: request_msg.to_json_bytes().unwrap().into(),
        ..Default::default()
    }]);

    let mut request = tonic::Request::new(outbound);
    request
        .metadata_mut()
        .insert("authorization", format!("Bearer {}", session_token()).parse().unwrap());

    let mut inbound = client.do_exchange(request).await.unwrap().into_inner();

    while let Some(data) = inbound.message().await.unwrap() {
        if let Ok(ExchangeMessage::Response { result, .. }) =
            ExchangeMessage::from_json_bytes(data.app_metadata.as_ref())
        {
            return result.map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
        }
    }
    panic!("do_exchange closed without answering the Upload");
}

// ------------------------------------------------------------------- tests

/// **The finding.** A session-authenticated caller names a table it has no
/// business writing — the stand-in here for `_system`, an audit table or the CDC
/// outbox — and the rows must not land.
///
/// Before the gate this passed the rows straight through: the assertion that
/// fails first is the row count, which is the whole point of #953. The refusal
/// must also *say* why, so an operator who allow-listed nothing does not debug a
/// silent write.
#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn upload_into_a_non_allow_listed_table_is_refused_and_writes_nothing() {
    let pool = pool().await;
    let forbidden = unique_table("forbidden");
    create_table(&pool, &forbidden).await;

    let service = FraiseQLFlightService::new_with_db(flight_adapter().await)
        .with_session_secret(TEST_FLIGHT_SECRET);
    let addr = serve(service).await;

    let result = upload(&addr, &forbidden, &one_row_batch()).await;

    let landed = row_count(&pool, &forbidden).await;
    drop_table(&pool, &forbidden).await;

    assert_eq!(
        landed, 0,
        "a session token alone must not be able to INSERT into an arbitrary table — {landed} \
         row(s) landed in a table no operator allow-listed"
    );
    let message = result.expect_err("the Upload must be refused, not answered with a row count");
    assert!(
        message.contains(&forbidden),
        "the refusal must name the table it refused, got: {message}"
    );
}

/// Allow-listing is necessary but not sufficient: an adapter that cannot write the
/// rows and their change-log outbox rows in one transaction must refuse, even for a
/// table the operator named.
///
/// `PgFlightAdapter` here implements only `execute_raw_query` — exactly the shape
/// every `ArrowDatabaseAdapter` had before #953 — so it takes
/// `execute_gated_upload`'s default. That default **refuses**. The fail-open
/// alternative (run the INSERT, skip the outbox) would write rows the Change Spine
/// never sees, silently, on every adapter that had not been updated.
///
/// The counterweight — an allow-listed Upload that really does write its rows and
/// its outbox rows — lives in `fraiseql-server`'s `flight_upload_outbox_pg`, where
/// the adapter that implements the seam lives.
#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn an_allow_listed_upload_is_still_refused_by_an_adapter_that_cannot_write_atomically() {
    let pool = pool().await;
    let allowed = unique_table("allowed");
    create_table(&pool, &allowed).await;

    let service = FraiseQLFlightService::new_with_db(flight_adapter().await)
        .with_session_secret(TEST_FLIGHT_SECRET)
        .with_upload_tables([allowed.clone()]);
    let addr = serve(service).await;

    let result = upload(&addr, &allowed, &one_row_batch()).await;

    let landed = row_count(&pool, &allowed).await;
    drop_table(&pool, &allowed).await;

    assert_eq!(
        landed, 0,
        "an adapter with no atomic-upload seam must write nothing, not write the rows \
         and skip the change-log"
    );
    let message = result.expect_err("the Upload must be refused");
    assert!(
        message.contains("execute_gated_upload"),
        "the refusal must name what the adapter is missing, got: {message}"
    );
}

/// Allow-listing one table must not allow-list its neighbours. The gate is a
/// membership test, not an on/off switch for the whole surface.
#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn an_allow_list_admits_only_the_tables_it_names() {
    let pool = pool().await;
    let allowed = unique_table("allowed");
    let other = unique_table("other");
    create_table(&pool, &allowed).await;
    create_table(&pool, &other).await;

    let service = FraiseQLFlightService::new_with_db(flight_adapter().await)
        .with_session_secret(TEST_FLIGHT_SECRET)
        .with_upload_tables([allowed.clone()]);
    let addr = serve(service).await;

    let result = upload(&addr, &other, &one_row_batch()).await;

    let landed = row_count(&pool, &other).await;
    drop_table(&pool, &allowed).await;
    drop_table(&pool, &other).await;

    assert_eq!(landed, 0, "a table absent from the allow-list must stay untouched");
    result.expect_err("an unlisted table must be refused even when the allow-list is non-empty");
}
