//! #811 regression: a REST export must emit every row exactly once, and terminate.
//!
//! The NDJSON, CSV and XLSX batch loops each advanced pagination by writing `limit` and
//! `offset` into a **clone of `variables`**. `execute_query_direct` reads limit/offset
//! only from `query_match.arguments` — its `variables` parameter feeds `enforce_authz`
//! and nothing else — so every batch re-issued the identical first-page query.
//!
//! One bug, two failure modes, selected by whether the page happens to fill:
//!
//! * **Truncation.** `rows.len() < batch_size` on the first pass, so the loop stops after one page.
//!   A 10,000-row export returns `default_page_size` rows with HTTP 200 and no error line — the
//!   caller believes it exported everything and has 1% of the data.
//! * **Non-termination.** `rows.len() == batch_size`, so the loop never sets `done`, re-issues the
//!   same query and re-emits the same rows forever. The response body grows without bound while
//!   pinning a database connection and a worker.
//!
//! **Why no existing test could see it:** `streaming/tests.rs` drives no batch loop at
//! all. Every test there is a pure-function unit test over `extract_rows`,
//! `error_ndjson_line` or `accepts_ndjson`. The loop needs a real database with more rows
//! than one page to express itself, and nothing in the suite had one.
//!
//! Both assertions here are about **distinct** ids, not row counts: a duplicate-emitting
//! loop and a correct loop produce the same count for the first page, and only the
//! identity of the rows separates them.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tf_p13_export` fixture → run
//! `--test-threads=1`.
#![cfg(feature = "rest")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{collections::HashSet, sync::Arc, time::Duration};

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    schema::{CompiledSchema, FieldType, RestConfig},
};
use fraiseql_server::server_config::ServerConfig;
use fraiseql_test_support::try_database_url;
use fraiseql_test_utils::schema_builder::{
    TestFieldBuilder, TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder,
};
use serde_json::Value;

mod common;

use crate::common::server_harness::TestServer;

const TABLE: &str = "tf_p13_export";
const VIEW: &str = "v_p13_export";

/// Rows seeded. Must exceed `PAGE` by enough to need many batches — the defect is
/// invisible whenever one page covers the whole table.
const ROWS: usize = 10_000;
/// Both the batch size and the default page size, so the export needs `ROWS / PAGE`
/// round-trips and the *non-termination* mode is reachable with `?limit=PAGE`.
const PAGE: u64 = 100;

/// Any single export must finish well inside this. It exists so the non-termination
/// mode **fails** the test rather than hanging the suite.
const EXPORT_TIMEOUT: Duration = Duration::from_mins(1);

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

/// Create the table + view and seed `ROWS` rows with distinct ids.
async fn seed(adapter: &PostgresAdapter) {
    let stmts = vec![
        format!("DROP VIEW IF EXISTS {VIEW}"),
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!("CREATE TABLE {TABLE} (id bigint PRIMARY KEY, data jsonb NOT NULL)"),
        // Seeded in one statement: 10,000 individual INSERTs over the wire dominates
        // the test's runtime and teaches nothing.
        format!(
            "INSERT INTO {TABLE} (id, data) SELECT g, jsonb_build_object('id', g, 'label', \
             'row-' || g) FROM generate_series(1, {ROWS}) g"
        ),
        format!("CREATE VIEW {VIEW} AS SELECT id, data FROM {TABLE}"),
    ];

    for stmt in stmts {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup");
    }
}

fn build_schema() -> CompiledSchema {
    let query = TestQueryBuilder::new("exports", "P13Export")
        .returns_list(true)
        .with_sql_source(VIEW)
        // #958: the streaming representations are a per-route opt-in.
        .rest_stream(true)
        .build();

    // The same rows behind a route that did **not** opt in (#958), so one server
    // serves both and the difference is the flag rather than the fixture.
    let mut plain = TestQueryBuilder::new("plainExports", "P13Plain")
        .returns_list(true)
        .with_sql_source(VIEW)
        .build();
    plain.rest_path = Some("/plain".to_string());

    let mut schema = TestSchemaBuilder::new()
        .with_type(
            TestTypeBuilder::new("P13Export", VIEW)
                .with_field(TestFieldBuilder::new("id", FieldType::Int).build())
                .with_field(TestFieldBuilder::new("label", FieldType::String).build())
                .build(),
        )
        .with_type(
            TestTypeBuilder::new("P13Plain", VIEW)
                .with_field(TestFieldBuilder::new("id", FieldType::Int).build())
                .with_field(TestFieldBuilder::new("label", FieldType::String).build())
                .build(),
        )
        .with_query(query)
        .with_query(plain)
        .build();

    schema.rest_config = Some(RestConfig {
        enabled: true,
        default_page_size: PAGE,
        ndjson_batch_size: PAGE,
        ..RestConfig::default()
    });
    schema.build_indexes();
    schema
}

async fn start() -> Option<TestServer> {
    start_with_export(fraiseql_server::routes::rest::export_config::ExportConfig::default()).await
}

/// Start a server whose `[export]` table is `export`.
///
/// #917: until this phase there was no way to write such a test, because there was no
/// deserialization site for `ExportConfig` anywhere — every consumer built its own
/// `::default()`, so an operator's `[export]` table reached nothing.
async fn start_with_export(
    export: fraiseql_server::routes::rest::export_config::ExportConfig,
) -> Option<TestServer> {
    let url = try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.expect("connect to the test database");
    seed(&adapter).await;

    let config = ServerConfig {
        export,
        // #874: production validate() refuses cors_enabled=true + empty origins
        cors_enabled: false,
        ..ServerConfig::default()
    };

    Some(Box::pin(TestServer::start_with_config(config, build_schema(), Arc::new(adapter))).await)
}

/// Issue an export and return the raw body, failing the test if it does not terminate.
///
/// The timeout is the point: against the non-terminating mode this returns an error
/// instead of hanging the run.
async fn export(base: &str, accept: &str, query: &str) -> String {
    let request = reqwest::Client::new()
        .get(format!("{base}/rest/v1/exports{query}"))
        .header("accept", accept)
        .timeout(EXPORT_TIMEOUT)
        .send();

    let response = tokio::time::timeout(EXPORT_TIMEOUT, request)
        .await
        .unwrap_or_else(|_| panic!("{accept}: export did not terminate within {EXPORT_TIMEOUT:?}"))
        .unwrap_or_else(|e| panic!("{accept}: export request failed: {e}"));

    assert!(
        response.status().is_success(),
        "{accept}: export should succeed, got {}",
        response.status()
    );

    tokio::time::timeout(EXPORT_TIMEOUT, response.text())
        .await
        .unwrap_or_else(|_| {
            panic!("{accept}: export body did not terminate within {EXPORT_TIMEOUT:?}")
        })
        .unwrap_or_else(|e| panic!("{accept}: reading the export body failed: {e}"))
}

/// The `id` of every NDJSON line, in order.
fn ndjson_ids(body: &str) -> Vec<i64> {
    body.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let row: Value = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("NDJSON line is not JSON ({e}): {line}"));
            assert!(row.get("error").is_none(), "export emitted an error line: {line}");
            row.get("id")
                .and_then(Value::as_i64)
                .unwrap_or_else(|| panic!("NDJSON row carries no id: {line}"))
        })
        .collect()
}

/// The first CSV column of every data row (header skipped), parsed as an id.
///
/// Gated to match its only caller: without it, `--features rest` alone (no `export-csv`)
/// leaves this dead and `-D dead-code` fails the build. No CI leg runs that exact
/// combination today, which is what made it latent rather than red.
#[cfg(feature = "export-csv")]
fn csv_ids(body: &str) -> Vec<i64> {
    let mut lines = body.trim_start_matches('\u{feff}').lines().filter(|l| !l.trim().is_empty());
    let header = lines.next().unwrap_or_else(|| panic!("CSV export carried no header"));
    let id_col = header
        .split(',')
        .position(|c| c.trim().trim_matches('"') == "id")
        .unwrap_or_else(|| panic!("CSV header has no `id` column: {header}"));

    lines
        .map(|line| {
            let cell =
                line.split(',').nth(id_col).unwrap_or_else(|| panic!("short CSV row: {line}"));
            cell.trim()
                .trim_matches('"')
                .parse()
                .unwrap_or_else(|e| panic!("CSV id cell is not a number ({e}): {line}"))
        })
        .collect()
}

/// Assert an id list is exactly `1..=ROWS`, each appearing once.
fn assert_complete_and_distinct(kind: &str, ids: &[i64]) {
    let distinct: HashSet<i64> = ids.iter().copied().collect();

    assert_eq!(
        ids.len(),
        distinct.len(),
        "{kind}: export emitted {} rows but only {} distinct ids — the batch loop is \
         re-issuing the same page",
        ids.len(),
        distinct.len()
    );
    assert_eq!(
        ids.len(),
        ROWS,
        "{kind}: expected all {ROWS} rows, got {} — the batch loop stopped after one page",
        ids.len()
    );
    let expected: HashSet<i64> = (1..=i64::try_from(ROWS).unwrap()).collect();
    assert_eq!(distinct, expected, "{kind}: the exported id set is not the seeded id set");
}

// ---------------------------------------------------------------------------
// The contract
// ---------------------------------------------------------------------------

/// #811: an NDJSON export must emit every row exactly once.
#[tokio::test]
async fn an_ndjson_export_emits_every_row_exactly_once() {
    let Some(server) = start().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let body = export(&server.url, "application/x-ndjson", "").await;
    assert_complete_and_distinct("NDJSON", &ndjson_ids(&body));
}

/// #811's non-termination mode: `?limit=` equal to the batch size must still terminate.
///
/// This is the shape that made a single unauthenticated GET a denial of service: with
/// `rows.len() == batch_size` the loop never set `done`, so it re-emitted the same page
/// forever. `?limit=100` is legal under stock defaults and was asserted permitted by an
/// existing test.
#[tokio::test]
async fn an_export_whose_limit_equals_the_batch_size_terminates() {
    let Some(server) = start().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let body = export(&server.url, "application/x-ndjson", &format!("?limit={PAGE}")).await;
    let ids = ndjson_ids(&body);
    let distinct: HashSet<i64> = ids.iter().copied().collect();

    assert_eq!(
        ids.len(),
        distinct.len(),
        "an export bounded by ?limit={PAGE} re-emitted rows: {} emitted, {} distinct",
        ids.len(),
        distinct.len()
    );
    assert_eq!(
        ids.len(),
        usize::try_from(PAGE).unwrap(),
        "?limit={PAGE} must bound the export to {PAGE} rows, got {}",
        ids.len()
    );
}

/// #811: `?limit=` bounds the export total, and the bound is honoured across batches.
#[tokio::test]
async fn an_export_limit_bounds_the_total_across_batches() {
    let Some(server) = start().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    // Deliberately not a multiple of the page size: the final partial batch is where an
    // off-by-one in the remaining-rows arithmetic shows up.
    let limit = PAGE * 3 + 7;
    let body = export(&server.url, "application/x-ndjson", &format!("?limit={limit}")).await;
    let ids = ndjson_ids(&body);

    assert_eq!(
        ids.len(),
        usize::try_from(limit).unwrap(),
        "?limit={limit} must yield exactly {limit} rows, got {}",
        ids.len()
    );
    assert_eq!(
        ids.iter().copied().collect::<HashSet<i64>>().len(),
        usize::try_from(limit).unwrap(),
        "?limit={limit} yielded duplicate rows"
    );
}

/// #811: the CSV export shares the defect and must share the fix.
#[cfg(feature = "export-csv")]
#[tokio::test]
async fn a_csv_export_emits_every_row_exactly_once() {
    let Some(server) = start().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let body = export(&server.url, "text/csv", "").await;
    assert_complete_and_distinct("CSV", &csv_ids(&body));
}

/// #811: the XLSX builder shares the defect and must share the fix.
///
/// **This assertion is deliberately coarser than its NDJSON and CSV siblings, and the
/// reason is worth stating.** An `.xlsx` is a zip archive; asserting an exact row count
/// means parsing the workbook, and the only zip crate in the tree arrives via
/// `rust_xlsxwriter` under `export-xlsx` — making it a dev-dependency would compile zip
/// into every test build of this crate, including the ones that never touch XLSX.
///
/// So this test pins the two properties that the shared pagination driver actually puts
/// at risk, both of which are fully visible in the response envelope:
///
/// * **termination** — the duplicating loop never returns, so the timeout catches it;
/// * **not truncated** — a single-page (`PAGE`-row) workbook is roughly two orders of magnitude
///   smaller than a `ROWS`-row one, so a floor well above the former and well below the latter
///   separates them unambiguously.
///
/// Exact-count coverage for the driver itself lives in the unit tests over
/// `set_export_page`, and end-to-end in the two text formats above, which exercise the
/// same driver.
#[cfg(feature = "export-xlsx")]
#[tokio::test]
async fn an_xlsx_export_is_neither_truncated_nor_endless() {
    /// A `PAGE`-row workbook measures a few `KiB`; a `ROWS`-row one measures hundreds.
    /// Any value between the two separates truncated from complete.
    const TRUNCATION_FLOOR: usize = 50_000;

    let Some(server) = start().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let request = reqwest::Client::new()
        .get(format!("{}/rest/v1/exports", server.url))
        .header("accept", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")
        .timeout(EXPORT_TIMEOUT)
        .send();

    let response = tokio::time::timeout(EXPORT_TIMEOUT, request)
        .await
        .expect("XLSX export did not terminate")
        .expect("XLSX export request failed");
    assert!(
        response.status().is_success(),
        "XLSX export should succeed, got {}",
        response.status()
    );

    let bytes = tokio::time::timeout(EXPORT_TIMEOUT, response.bytes())
        .await
        .expect("XLSX body did not terminate")
        .expect("reading the XLSX body failed");

    // `PK` magic — a sanity check that we measured a workbook and not an error page.
    assert_eq!(bytes.get(..2), Some(&b"PK"[..]), "XLSX export did not return a zip archive");
    assert!(
        bytes.len() > TRUNCATION_FLOOR,
        "XLSX export is {} bytes — consistent with a single {PAGE}-row page rather than \
         all {ROWS} rows",
        bytes.len()
    );
}

// ---------------------------------------------------------------------------
// #917 — the `[export]` table must reach the transport
// ---------------------------------------------------------------------------

/// A configured `csv_delimiter` must appear in the served CSV.
///
/// The CSV handler built its own `ExportConfig::default()`, under a comment conceding
/// that "TOML-driven `ExportConfig` loading is a later phase" — so this key, and the six
/// beside it, were accepted by the config parser and then ignored. The assertion is on
/// the delimiter *in the bytes*, because the config object being correct proves nothing
/// about the config object the handler used.
#[cfg(feature = "export-csv")]
#[tokio::test]
async fn a_configured_csv_delimiter_reaches_the_served_csv() {
    use fraiseql_server::routes::rest::export_config::ExportConfig;

    let Some(server) = start_with_export(ExportConfig {
        csv_delimiter: ';',
        ..ExportConfig::default()
    })
    .await
    else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let body = export(&server.url, "text/csv", "?limit=5").await;
    let header = body
        .trim_start_matches('\u{feff}')
        .lines()
        .next()
        .expect("CSV export carried no header");

    assert!(
        header.contains(';'),
        "the configured ';' delimiter did not reach the CSV writer; header: {header:?}"
    );
    assert!(
        !header.contains(','),
        "the default ',' delimiter is still in use; header: {header:?}"
    );
}

/// The `export_formats` kill-switch must actually refuse a disabled format.
///
/// This is the field's first consumer. `406` rather than `404`: the resource exists and
/// the route answers — it is the requested representation the server declines.
#[cfg(all(feature = "export-csv", feature = "export-xlsx"))]
#[tokio::test]
async fn a_disabled_export_format_is_refused_while_the_enabled_one_still_serves() {
    use fraiseql_server::routes::rest::export_config::{ExportConfig, ExportFormat};

    let Some(server) = start_with_export(ExportConfig {
        export_formats: vec![ExportFormat::Csv],
        ..ExportConfig::default()
    })
    .await
    else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let client = reqwest::Client::builder().timeout(EXPORT_TIMEOUT).build().unwrap();

    let refused = client
        .get(format!("{}/rest/v1/exports?limit=5", server.url))
        .header("accept", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")
        .send()
        .await
        .expect("XLSX request");
    let status = refused.status();
    let body = refused.text().await.unwrap_or_default();
    assert_eq!(
        status,
        reqwest::StatusCode::NOT_ACCEPTABLE,
        "a disabled format must be refused, not served; body: {body}"
    );
    assert!(
        body.contains("EXPORT_FORMAT_DISABLED"),
        "the refusal must name its reason; body: {body}"
    );

    // The control: the enabled format still serves, so the test above cannot be
    // satisfied by a server that refuses every export.
    let allowed = client
        .get(format!("{}/rest/v1/exports?limit=5", server.url))
        .header("accept", "text/csv")
        .send()
        .await
        .expect("CSV request");
    assert_eq!(
        allowed.status(),
        reqwest::StatusCode::OK,
        "the format left enabled must still serve"
    );
}

// ---------------------------------------------------------------------------
// `rest_stream` — the per-route opt-in (#958)
// ---------------------------------------------------------------------------

/// One request for a streaming representation on a route that did not opt in.
async fn request_export(base: &str, path: &str, accept: &str) -> (reqwest::StatusCode, String) {
    let response = reqwest::Client::new()
        .get(format!("{base}/rest/v1{path}"))
        .header("accept", accept)
        .timeout(EXPORT_TIMEOUT)
        .send()
        .await
        .unwrap_or_else(|e| panic!("{accept} {path}: request failed: {e}"));
    let status = response.status();
    (status, response.text().await.expect("response body"))
}

/// A route without `rest_stream = true` refuses every streaming representation
/// with `406`, and says which flag would offer it.
///
/// Refusing rather than substituting the JSON envelope is the point. A client that
/// sends `Accept: application/x-ndjson` is asking to be handed a dataset; quietly
/// answering with one page of a different representation is #811's failure mode
/// wearing a different header — the caller believes it exported everything.
#[tokio::test]
async fn a_route_without_rest_stream_refuses_every_streaming_representation() {
    let Some(server) = start().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    for accept in [
        "application/x-ndjson",
        "text/csv",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    ] {
        let (status, body) = request_export(&server.url, "/plain", accept).await;
        assert_eq!(
            status,
            reqwest::StatusCode::NOT_ACCEPTABLE,
            "{accept}: a route that did not opt in must refuse, got {status} {body}"
        );
        assert!(
            body.contains("rest_stream"),
            "{accept}: the refusal must name the flag that would offer it, got {body}"
        );
    }
}

/// The opt-in gates the *streaming* representations only: the same route still
/// serves its JSON envelope. A flag that turned the route off would be a different,
/// much blunter feature.
#[tokio::test]
async fn a_route_without_rest_stream_still_serves_json() {
    let Some(server) = start().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = request_export(&server.url, "/plain", "application/json").await;
    assert_eq!(
        status,
        reqwest::StatusCode::OK,
        "the JSON representation must be unaffected: {body}"
    );
    let parsed: Value = serde_json::from_str(&body).expect("JSON body");
    assert!(
        parsed
            .get("data")
            .and_then(Value::as_array)
            .is_some_and(|rows| !rows.is_empty()),
        "the JSON representation must still return rows: {body}"
    );
}
