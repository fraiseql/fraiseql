//! #1355 — a Flight `DoPut` upload writes its rows **and** their Change Spine outbox
//! rows, in one transaction, against a real PostgreSQL.
//!
//! #953 moved the Flight upload off `execute_raw_query` and onto `execute_gated_upload`
//! so the rows and their `core.tb_entity_change_log` outbox rows commit together. It
//! reached **one of the two upload verbs**. `DoExchange`'s `Upload` carried the fix;
//! `DoPut` kept building an INSERT with `build_insert_query` and dispatching it with
//! `execute_raw_query`, batch by batch — so every `DoPut` upload was invisible to the
//! Change Spine, and a subscriber tailing the change log saw the same rows in the same
//! table appear or not appear depending on which Flight verb wrote them.
//!
//! ## Why the suite is shaped this way
//!
//! Both verbs look identical from outside: the rows land either way. The only
//! observer that can tell them apart is one reading the change log, so every
//! assertion here is about the **outbox**, not the target table — a test that merely
//! counted rows in the target table passed throughout the defect.
//!
//! The `DoExchange` twin is not decoration. It drives the *same* batch, through the
//! *same* adapter, into a sibling table, and asserts the *same* outbox rows. Before
//! the fix it passes and the `DoPut` cases fail; that difference is the whole finding,
//! and it is what proves the `DoPut` failure is about `DoPut` rather than about the
//! provisioning, the fixture, the adapter or the assertion. Keeping the twin means a
//! future change that quietly breaks the harness reddens both, not neither.
//!
//! ## Why it lives in `fraiseql-server`
//!
//! `FlightDatabaseAdapter` is the only adapter in the tree that implements
//! `execute_gated_upload`, and it lives here. `fraiseql-arrow`'s `flight_upload_gate_pg`
//! covers the decision half over a real Flight socket with an adapter that *cannot*
//! write atomically, and so can only assert refusals. This suite is the write half:
//! the real adapter, a real socket, a real session token.
//!
//! `#[ignore]` — needs `DATABASE_URL` (a real Postgres). Named explicitly by the Dagger
//! `integration` leg (`observers` suite, which binds a Postgres), so it either runs or
//! the leg fails; it can never self-skip into a false green. Run with:
//! `cargo test -p fraiseql-server --features arrow --test flight_do_put_outbox_pg --
//! --ignored --test-threads=1`.

// PostgreSQL build only, for the same reason as `flight_upload_outbox_pg`: under
// `wire-backend`, `FlightDatabaseAdapter` wraps a `FraiseWireAdapter`, which does not
// implement `execute_gated_upload`, so an allow-listed upload is refused for want of an
// atomic write path and there is no outbox behaviour to assert. ⚠ The leg that runs this
// suite must **not** pass `wire-backend`, or the binary compiles to zero tests and reads
// green; the Dagger line uses `--features arrow` for exactly that reason.
#![cfg(all(feature = "arrow", not(feature = "wire-backend")))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // Reason: test code, panics acceptable

use std::sync::Arc;

use fraiseql_arrow::{
    ExchangeMessage, RequestType,
    arrow::{
        array::{ArrayRef, StringArray},
        datatypes::{DataType, Field, Schema},
        record_batch::RecordBatch,
    },
    arrow_flight::{
        FlightData, FlightDescriptor, flight_descriptor::DescriptorType,
        flight_service_client::FlightServiceClient,
    },
    db::ArrowDatabaseAdapter,
    flight_server::FraiseQLFlightService,
    tonic::transport::{Endpoint, Server},
};
use fraiseql_server::arrow::FlightDatabaseAdapter;
use sqlx::{
    Row,
    postgres::{PgPool, PgPoolOptions},
};

/// The subject of the session token every case here authenticates with. Asserted on
/// the outbox row, so provenance that is hard-coded rather than carried from the
/// session cannot pass.
const UPLOAD_SUBJECT: &str = "do-put-outbox-user";

const TEST_FLIGHT_SECRET: &str = "flight-do-put-outbox-session-secret";

// ---------------------------------------------------------------- fixtures

async fn pool() -> PgPool {
    let url = fraiseql_test_support::database_url();
    PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap()
}

/// The change-log table from the ONE shared provisioner (#942/#982) — the migration-08
/// contract byte-for-byte, so this suite cannot assert against a shape no deployment has.
async fn provision_changelog(pool: &PgPool) {
    sqlx::raw_sql(&fraiseql_test_support::changelog::entity_change_log_provision_sql())
        .execute(pool)
        .await
        .unwrap();
}

/// A table this suite owns outright, so a run can never depend on — or damage — the
/// shared fixtures the other suites assert on.
fn unique_table(kind: &str) -> String {
    format!("ta_do_put_outbox_{}_{}", kind, uuid::Uuid::new_v4().simple())
}

async fn create_table(pool: &PgPool, table: &str) {
    sqlx::query(&format!("CREATE TABLE \"{table}\" (id UUID PRIMARY KEY, note TEXT)"))
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

/// Outbox rows the Change Spine holds for `table`.
async fn outbox_count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query("SELECT count(*) AS n FROM core.tb_entity_change_log WHERE object_type = $1")
        .bind(table)
        .fetch_one(pool)
        .await
        .unwrap()
        .get::<i64, _>("n")
}

/// UUID keys, so the outbox row's `object_id` is populated rather than NULL — the
/// adapter's CTE only fills it when the row's `id` parses as a UUID.
fn batch_of(rows: &[(&str, &str)]) -> RecordBatch {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("note", DataType::Utf8, false),
    ]));
    let ids: ArrayRef =
        Arc::new(StringArray::from(rows.iter().map(|(id, _)| *id).collect::<Vec<_>>()));
    let notes: ArrayRef =
        Arc::new(StringArray::from(rows.iter().map(|(_, n)| *n).collect::<Vec<_>>()));
    RecordBatch::try_new(schema, vec![ids, notes]).unwrap()
}

/// Arrow IPC stream bytes — the encoding both `decode_flight_data_to_batch` (DoPut)
/// and `decode_upload_batch` (DoExchange) expect, so the twin really does send the
/// same bytes.
fn encode_batch(batch: &RecordBatch) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut writer = fraiseql_arrow::arrow::ipc::writer::StreamWriter::try_new(
            &mut buf,
            batch.schema().as_ref(),
        )
        .unwrap();
        writer.write(batch).unwrap();
        writer.finish().unwrap();
    }
    buf
}

/// The adapter the shipped server passes to the Flight service — the one that
/// implements `execute_gated_upload`.
async fn flight_adapter() -> Arc<dyn ArrowDatabaseAdapter> {
    let url = fraiseql_test_support::database_url();
    Arc::new(FlightDatabaseAdapter::new(
        fraiseql_core::db::PostgresAdapter::new(&url).await.unwrap(),
    ))
}

/// A session token of exactly the shape `validate_session_token` accepts.
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
        sub:          UPLOAD_SUBJECT.to_string(),
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

/// Serve a Flight service that allows uploads into `tables`; returns the endpoint URL.
async fn serve(tables: &[&str]) -> String {
    let service = FraiseQLFlightService::new_with_db(flight_adapter().await)
        .with_session_secret(TEST_FLIGHT_SECRET)
        .with_upload_tables(tables.iter().copied());

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

fn authorized<T>(body: T) -> tonic::Request<T> {
    let mut request = tonic::Request::new(body);
    request
        .metadata_mut()
        .insert("authorization", format!("Bearer {}", session_token()).parse().unwrap());
    request
}

/// Drive one `DoPut` upload: a descriptor message naming `table`, then one message per
/// batch. Returns the server's `PutResult` metadata strings, in order.
async fn do_put(addr: &str, table: &str, batches: &[RecordBatch]) -> Vec<String> {
    let channel = Endpoint::from_shared(addr.to_string()).unwrap().connect().await.unwrap();
    let mut client = FlightServiceClient::new(channel);

    let mut messages = vec![FlightData {
        flight_descriptor: Some(FlightDescriptor {
            r#type: DescriptorType::Path as i32,
            path: vec![table.to_string()],
            ..Default::default()
        }),
        ..Default::default()
    }];
    messages.extend(batches.iter().map(|batch| FlightData {
        data_body: encode_batch(batch).into(),
        ..Default::default()
    }));

    let mut inbound = client
        .do_put(authorized(futures::stream::iter(messages)))
        .await
        .unwrap()
        .into_inner();

    let mut results = Vec::new();
    while let Some(result) = inbound.message().await.unwrap() {
        results.push(String::from_utf8_lossy(result.app_metadata.as_ref()).into_owned());
    }
    results
}

/// Drive the same bytes through `DoExchange`'s `Upload` — the verb #953 fixed.
async fn do_exchange_upload(
    addr: &str,
    table: &str,
    batch: &RecordBatch,
) -> Result<String, String> {
    let channel = Endpoint::from_shared(addr.to_string()).unwrap().connect().await.unwrap();
    let mut client = FlightServiceClient::new(channel);

    let request_msg = ExchangeMessage::Request {
        correlation_id: "do-put-twin-corr-1".to_string(),
        request_type:   RequestType::Upload {
            table: table.to_string(),
            batch: encode_batch(batch),
        },
    };
    let outbound = futures::stream::iter(vec![FlightData {
        app_metadata: request_msg.to_json_bytes().unwrap().into(),
        ..Default::default()
    }]);

    let mut inbound = client.do_exchange(authorized(outbound)).await.unwrap().into_inner();
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

/// **The finding.** An allow-listed `DoPut` upload lands its rows, and the Change Spine
/// must record every one of them.
///
/// Before the fix the row count passed and the outbox count was zero: `DoPut` dispatched
/// the INSERT through `execute_raw_query`, which cannot write the outbox row, so the
/// upload committed with no record of itself.
#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn a_do_put_upload_writes_one_outbox_row_per_uploaded_row() {
    let pool = pool().await;
    provision_changelog(&pool).await;
    let table = unique_table("put");
    create_table(&pool, &table).await;
    let addr = serve(&[&table]).await;

    let batch = batch_of(&[
        ("11111111-1111-4111-8111-111111111111", "one"),
        ("22222222-2222-4222-8222-222222222222", "two"),
    ]);
    let results = do_put(&addr, &table, std::slice::from_ref(&batch)).await;

    let rows = row_count(&pool, &table).await;
    let logged = outbox_count(&pool, &table).await;
    let log_row = sqlx::query(
        "SELECT object_id, object_data, modification_type, extra_metadata
         FROM core.tb_entity_change_log WHERE object_type = $1 ORDER BY object_id LIMIT 1",
    )
    .bind(&table)
    .fetch_optional(&pool)
    .await
    .unwrap();
    drop_table(&pool, &table).await;

    assert_eq!(rows, 2, "the DoPut upload's rows must land");
    assert_eq!(
        logged, 2,
        "the Change Spine must record one outbox row per uploaded row — got {logged} for \
         {rows} row(s), so a subscriber tailing the change log never sees this upload"
    );

    // Present is not enough: the row has to be usable by a consumer, and its provenance
    // has to have come from the session rather than from a constant.
    let log_row = log_row.expect("an outbox row must exist to inspect");
    assert_eq!(
        log_row.get::<String, _>("modification_type"),
        "INSERT",
        "an upload is an INSERT to the Change Spine"
    );
    assert_eq!(
        log_row.get::<uuid::Uuid, _>("object_id").to_string(),
        "11111111-1111-4111-8111-111111111111",
        "the outbox row must key on the uploaded row's own id"
    );
    let data: serde_json::Value = log_row.get("object_data");
    assert_eq!(
        data.get("note").and_then(serde_json::Value::as_str),
        Some("one"),
        "the outbox row must carry the uploaded row, not an empty envelope"
    );
    let metadata: serde_json::Value = log_row.get("extra_metadata");
    assert_eq!(
        metadata.get("transport").and_then(serde_json::Value::as_str),
        Some("flight"),
        "the outbox row must name the transport that wrote it"
    );
    assert_eq!(
        metadata.get("flight_user_id").and_then(serde_json::Value::as_str),
        Some(UPLOAD_SUBJECT),
        "the outbox row must carry the authenticated Flight subject, not a constant"
    );

    assert!(
        results.iter().any(|r| r.contains("2 total rows")),
        "the client must still be told what landed, got: {results:?}"
    );
}

/// Every batch of a multi-batch `DoPut` is recorded, not only the first or the last.
///
/// Per-batch dispatch is deliberate — see the handler's comment — so the contract this
/// pins is that each acknowledged batch is committed *together with* its outbox rows.
#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn every_batch_of_a_multi_batch_do_put_is_recorded() {
    let pool = pool().await;
    provision_changelog(&pool).await;
    let table = unique_table("multi");
    create_table(&pool, &table).await;
    let addr = serve(&[&table]).await;

    let batches = [
        batch_of(&[
            ("33333333-3333-4333-8333-333333333333", "b1r1"),
            ("44444444-4444-4444-8444-444444444444", "b1r2"),
        ]),
        batch_of(&[("55555555-5555-4555-8555-555555555555", "b2r1")]),
    ];
    do_put(&addr, &table, &batches).await;

    let rows = row_count(&pool, &table).await;
    let logged = outbox_count(&pool, &table).await;
    drop_table(&pool, &table).await;

    assert_eq!(rows, 3, "both batches' rows must land");
    assert_eq!(
        logged, 3,
        "every batch must be recorded, not just one — {logged} outbox row(s) for {rows} \
         uploaded row(s) means a batch committed unrecorded"
    );
}

/// The discriminator. The same bytes, the same adapter, the same socket — through the
/// verb #953 fixed.
///
/// This case passes both before and after the `DoPut` fix. That is its purpose: at RED
/// it is the evidence that the provisioning, the fixture, the adapter and the outbox
/// assertions above are all capable of observing an outbox row, so the `DoPut` failure
/// is a statement about `DoPut`. If this ever goes red alongside the cases above, the
/// harness broke — not the handler.
#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn the_do_exchange_twin_of_the_same_upload_writes_the_same_outbox_rows() {
    let pool = pool().await;
    provision_changelog(&pool).await;
    let table = unique_table("exchange");
    create_table(&pool, &table).await;
    let addr = serve(&[&table]).await;

    let batch = batch_of(&[
        ("66666666-6666-4666-8666-666666666666", "one"),
        ("77777777-7777-4777-8777-777777777777", "two"),
    ]);
    let answer = do_exchange_upload(&addr, &table, &batch).await;

    let rows = row_count(&pool, &table).await;
    let logged = outbox_count(&pool, &table).await;
    drop_table(&pool, &table).await;

    answer.expect("the allow-listed DoExchange upload must succeed");
    assert_eq!(rows, 2, "the DoExchange upload's rows must land");
    assert_eq!(
        logged, 2,
        "DoExchange has recorded its uploads since #953; if this is 0 the harness is \
         broken, not the handler under test"
    );
}
