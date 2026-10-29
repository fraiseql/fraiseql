//! A PostgreSQL `numeric` reaches an Arrow Flight client with its exact digits (#1766).
//!
//! Each column is typed by what PostgreSQL says it is, never by a sampled value: a
//! `numeric(p, s)` with `p <= 38` is `Decimal128(p, s)`, and any other `numeric` is `Utf8`
//! holding the text PostgreSQL printed. Checked on every Flight path that reads rows
//! through the shipped adapter: `OptimizedView` (and the `GetSchema` that advertises its
//! schema), `BulkExport`, and `BatchedQueries` with and without the query cache.
//!
//! `#[ignore]` — needs `DATABASE_URL`, and panics without it rather than skipping. Named
//! explicitly by the Dagger `integration` leg (the `observers` suite, which binds a
//! Postgres), so it either runs or the leg fails. Run with: `cargo test -p fraiseql-server
//! --features arrow --test flight_decimal_pg -- --ignored --test-threads=1`.

#![cfg(feature = "arrow")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // Reason: test code, panics acceptable

use std::sync::Arc;

use fraiseql_arrow::{
    FlightTicket, FraiseQLFlightService,
    arrow::{
        array::{Array as _, Decimal128Array, StringArray},
        datatypes::{DataType, Field, Schema},
        record_batch::RecordBatch,
    },
    arrow_flight::{FlightDescriptor, Ticket, flight_service_server::FlightService as _},
};
use fraiseql_core::db::{DatabaseAdapter as _, postgres::PostgresAdapter};
use fraiseql_server::arrow::{FlightDatabaseAdapter, create_flight_service, register_flight_views};
use futures::StreamExt as _;

const SECRET: &str = "flight-decimal-session-secret";

/// The table, a view over it, and a view whose `numeric(10, 2)` holds `NaN`.
const SETUP: &[&str] = &[
    "DROP VIEW IF EXISTS p_flight_dec_v",
    "DROP VIEW IF EXISTS p_flight_dec_nan",
    "DROP TABLE IF EXISTS p_flight_dec_t",
    "DROP TABLE IF EXISTS p_flight_dec_nan_t",
    "CREATE TABLE p_flight_dec_t (id int PRIMARY KEY, amount numeric(20, 2), ratio numeric, \
     wide numeric(40, 2))",
    "INSERT INTO p_flight_dec_t VALUES \
     (1, 12345678901234567.89, 1.50, 1234567890123456789012345678901234567.89), \
     (2, NULL, NULL, NULL)",
    "CREATE VIEW p_flight_dec_v AS \
     SELECT id, amount, ratio, wide, ratio::numeric(6, 3) AS ratio3, amount * 2 AS doubled \
     FROM p_flight_dec_t",
    "CREATE VIEW p_flight_dec_nan AS SELECT 1 AS id, 'NaN'::numeric(10, 2) AS amount",
    "CREATE TABLE p_flight_dec_nan_t (id int PRIMARY KEY, amount numeric(10, 2))",
    "INSERT INTO p_flight_dec_nan_t VALUES (1, 'NaN')",
];

const WIDE: &str = "1234567890123456789012345678901234567.89";

async fn adapter() -> Arc<PostgresAdapter> {
    let url = fraiseql_test_support::database_url();
    let pg = Arc::new(PostgresAdapter::with_pool_size(&url, 2).await.unwrap());
    for statement in SETUP {
        pg.execute_raw_query(statement).await.unwrap();
    }
    pg
}

fn session_token() -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};

    let now = chrono::Utc::now().timestamp();
    encode(
        &Header::new(Algorithm::HS256),
        &serde_json::json!({
            "sub": "flight-decimal",
            "exp": now + 300,
            "iat": now,
            "scopes": ["user"],
            "session_type": "flight",
        }),
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .unwrap()
}

fn authorized<T>(message: T) -> tonic::Request<T> {
    let mut request = tonic::Request::new(message);
    request
        .metadata_mut()
        .insert("authorization", format!("Bearer {}", session_token()).parse().unwrap());
    request
}

/// Every `FlightData` of one `do_get`, or the status that refused it.
async fn frames(
    service: &FraiseQLFlightService,
    ticket: &FlightTicket,
) -> Result<Vec<fraiseql_arrow::arrow_flight::FlightData>, tonic::Status> {
    let ticket = Ticket {
        ticket: ticket.encode().unwrap().into(),
    };
    let mut stream = service.do_get(authorized(ticket)).await?.into_inner();
    let mut frames = Vec::new();
    while let Some(frame) = stream.next().await {
        frames.push(frame?);
    }
    Ok(frames)
}

async fn batches(
    service: &FraiseQLFlightService,
    ticket: &FlightTicket,
) -> Result<Vec<RecordBatch>, tonic::Status> {
    let frames = frames(service, ticket).await?;
    Ok(fraiseql_arrow::arrow_flight::utils::flight_data_to_batches(&frames).unwrap())
}

/// One column across `batches`: its Arrow type, and each cell as the client reads it.
fn column(batches: &[RecordBatch], name: &str) -> (DataType, Vec<Option<String>>) {
    let mut data_type = None;
    let mut cells = Vec::new();
    for batch in batches {
        let array = batch.column_by_name(name).unwrap_or_else(|| panic!("no column `{name}`"));
        data_type = Some(array.data_type().clone());
        for i in 0..array.len() {
            let cell = if array.is_null(i) {
                None
            } else if let Some(decimals) = array.as_any().downcast_ref::<Decimal128Array>() {
                Some(decimals.value_as_string(i))
            } else if let Some(strings) = array.as_any().downcast_ref::<StringArray>() {
                Some(strings.value(i).to_string())
            } else {
                panic!("`{name}` arrived as {:?}", array.data_type())
            };
            cells.push(cell);
        }
    }
    (data_type.expect("at least one batch"), cells)
}

/// The columns of `p_flight_dec_v` / `p_flight_dec_t`, read in `id` order.
fn assert_exact(path: &str, batches: &[RecordBatch], with_ratio3: bool) {
    let some = |s: &str| Some(s.to_string());
    let mut expected = vec![
        ("amount", DataType::Decimal128(20, 2), vec![some("12345678901234567.89"), None]),
        ("ratio", DataType::Utf8, vec![some("1.50"), None]),
        ("wide", DataType::Utf8, vec![some(WIDE), None]),
    ];
    if with_ratio3 {
        expected.push(("ratio3", DataType::Decimal128(6, 3), vec![some("1.500"), None]));
    }
    for (name, data_type, cells) in expected {
        assert_eq!(column(batches, name), (data_type, cells), "{path}: column `{name}`");
    }
}

fn view_ticket(view: &str) -> FlightTicket {
    FlightTicket::OptimizedView {
        view:     view.to_string(),
        filter:   None,
        order_by: Some("id".to_string()),
        limit:    None,
        offset:   None,
    }
}

/// A view's `numeric(p, s)` columns are `Decimal128`, from a `numeric(20, 2)` the view reads
/// from its table and from a cast the view makes, and its unconstrained or wider ones are
/// `Utf8`: in the schema `GetSchema` advertises and in the rows `do_get` sends.
#[tokio::test]
#[ignore = "needs DATABASE_URL; run by the Dagger integration leg"]
async fn an_optimized_view_serves_each_numeric_with_its_digits() {
    let pg = adapter().await;
    let service = create_flight_service(Arc::clone(&pg), &[]).with_session_secret(SECRET);
    let served = register_flight_views(&service, &["p_flight_dec_v".to_string()]).await;
    assert_eq!(served, ["p_flight_dec_v"]);

    let descriptor = FlightDescriptor::new_path(vec![
        String::from_utf8(view_ticket("p_flight_dec_v").encode().unwrap()).unwrap(),
    ]);
    let advertised = service.get_schema(authorized(descriptor)).await.unwrap().into_inner();
    let advertised = Schema::try_from(&advertised).unwrap();
    for (name, data_type) in [
        ("amount", DataType::Decimal128(20, 2)),
        ("ratio", DataType::Utf8),
        ("wide", DataType::Utf8),
        ("ratio3", DataType::Decimal128(6, 3)),
    ] {
        assert_eq!(
            advertised.field_with_name(name).unwrap().data_type(),
            &data_type,
            "GetSchema: column `{name}`"
        );
    }

    let batches = batches(&service, &view_ticket("p_flight_dec_v")).await.unwrap();
    assert_exact("OptimizedView", &batches, true);
    // PostgreSQL describes a column the view computes as an unconstrained `numeric`.
    assert_eq!(
        column(&batches, "doubled"),
        (DataType::Utf8, vec![Some("24691357802469135.78".to_string()), None]),
        "OptimizedView: a computed column"
    );
}

/// `BulkExport` writes each `numeric` with the digits PostgreSQL stored, and types a
/// `numeric(p, s)` as `Decimal128`: a `NaN` in one refuses the export, naming the column.
#[tokio::test]
#[ignore = "needs DATABASE_URL; run by the Dagger integration leg"]
async fn a_bulk_export_writes_each_numeric_with_its_digits() {
    let pg = adapter().await;
    let service = create_flight_service(Arc::clone(&pg), &[])
        .with_session_secret(SECRET)
        .with_bulk_export_tables(["p_flight_dec_t"]);
    let ticket = FlightTicket::BulkExport {
        table:  "p_flight_dec_t".to_string(),
        filter: None,
        limit:  None,
        format: Some("csv".to_string()),
    };
    let csv: String = frames(&service, &ticket)
        .await
        .unwrap()
        .iter()
        .map(|frame| String::from_utf8(frame.data_body.to_vec()).unwrap())
        .collect();
    let exact = ["12345678901234567.89", "1.50", WIDE];
    assert!(
        csv.lines().any(|line| {
            let cells: Vec<&str> = line.split(',').collect();
            exact.iter().all(|digits| cells.contains(digits))
        }),
        "BulkExport: one row must hold each of {exact:?} as a cell, got: {csv}"
    );

    let service = create_flight_service(Arc::clone(&pg), &[])
        .with_session_secret(SECRET)
        .with_bulk_export_tables(["p_flight_dec_nan_t"]);
    let ticket = FlightTicket::BulkExport {
        table:  "p_flight_dec_nan_t".to_string(),
        filter: None,
        limit:  None,
        format: Some("csv".to_string()),
    };
    let refused = frames(&service, &ticket).await.unwrap_err();
    assert!(
        refused.message().contains("amount") && refused.message().contains("NaN"),
        "BulkExport: the refusal must name the column and the value: {refused:?}"
    );
}

/// `BatchedQueries` serves each `numeric` with its digits, read from the database and read
/// back from the query cache.
#[tokio::test]
#[ignore = "needs DATABASE_URL; run by the Dagger integration leg"]
async fn batched_queries_serve_each_numeric_with_its_digits() {
    let pg = adapter().await;
    let flight_adapter = || Arc::new(FlightDatabaseAdapter::from_arc(Arc::clone(&pg)));
    let ticket = FlightTicket::BatchedQueries {
        queries: vec!["SELECT id, amount, ratio, wide FROM p_flight_dec_t ORDER BY id".into()],
    };

    let plain = FraiseQLFlightService::new_with_db(flight_adapter())
        .with_session_secret(SECRET)
        .with_raw_sql_enabled();
    assert_exact("BatchedQueries", &batches(&plain, &ticket).await.unwrap(), false);

    let cached = FraiseQLFlightService::new_with_cache(flight_adapter(), 300)
        .with_session_secret(SECRET)
        .with_raw_sql_enabled();
    for read in ["cache miss", "cache hit"] {
        let batches = batches(&cached, &ticket).await.unwrap();
        assert_exact(&format!("BatchedQueries, {read}"), &batches, false);
    }
}

/// `Decimal128` has no `NaN` or infinity, so a `numeric(p, s)` holding `NaN` refuses the
/// read, naming the column and the value. An unconstrained `numeric` is `Utf8`, so its
/// `NaN` and `Infinity` arrive as that text.
#[tokio::test]
#[ignore = "needs DATABASE_URL; run by the Dagger integration leg"]
async fn a_numeric_decimal128_cannot_hold_is_refused_naming_it() {
    let pg = adapter().await;
    let service = create_flight_service(Arc::clone(&pg), &[])
        .with_session_secret(SECRET)
        .with_raw_sql_enabled();
    let served = register_flight_views(&service, &["p_flight_dec_nan".to_string()]).await;
    assert_eq!(served, ["p_flight_dec_nan"]);

    let refused = batches(&service, &view_ticket("p_flight_dec_nan")).await.unwrap_err();
    assert!(
        refused.message().contains("amount") && refused.message().contains("NaN"),
        "the refusal must name the column and the value: {refused:?}"
    );

    let ticket = FlightTicket::BatchedQueries {
        queries: vec!["SELECT 'NaN'::numeric AS a, 'Infinity'::numeric AS b".into()],
    };
    let batches = batches(&service, &ticket).await.unwrap();
    assert_eq!(column(&batches, "a"), (DataType::Utf8, vec![Some("NaN".to_string())]));
    assert_eq!(column(&batches, "b"), (DataType::Utf8, vec![Some("Infinity".to_string())]));
}

/// A schema registered for a view that types a `numeric` column as a float would round
/// its digits, so `do_get` refuses it, naming the view, the column and the fix.
#[tokio::test]
#[ignore = "needs DATABASE_URL; run by the Dagger integration leg"]
async fn a_registered_schema_that_narrows_a_numeric_is_refused() {
    let pg = adapter().await;
    let service = create_flight_service(Arc::clone(&pg), &[]).with_session_secret(SECRET);
    service.schema_registry().register(
        "p_flight_dec_v",
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, true),
            Field::new("amount", DataType::Float64, true),
        ])),
    );

    let refused = batches(&service, &view_ticket("p_flight_dec_v")).await.unwrap_err();
    let message = refused.message();
    assert!(
        message.contains("p_flight_dec_v")
            && message.contains("amount")
            && message.contains("Decimal128"),
        "the refusal must name the view, the column and the fix: {refused:?}"
    );
}
