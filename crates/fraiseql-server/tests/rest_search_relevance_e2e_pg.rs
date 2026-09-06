//! #1284: `?search=` without `?sort=` must rank rows, not answer `400`.
//!
//! The handler used to add an implicit ordering of its own whenever a search was
//! active and the client had named no sort:
//!
//! ```text
//! arguments["orderBy"] = [{"_relevance": "desc"}]
//! ```
//!
//! `OrderByClause::from_graphql_json`'s array branch requires `{"field": …}`, so
//! that shape fails to parse and every representation answered
//! `400 orderBy item missing 'field' string`. The documented default path of the
//! full-text search feature — the OpenAPI description this server generates
//! promises "Results are ranked by relevance unless `sort` is specified" — was
//! the one spelling that could not succeed. `?search=x&sort=field` worked.
//!
//! And had the shape parsed it would still not have ranked: no `ts_rank` existed
//! anywhere in the workspace, and `_relevance` passes `validate_graphql_identifier`
//! (a leading `_` is legal), so it would have reached the SQL as an ordinary field
//! and emitted `ORDER BY data->>'_relevance' DESC` — NULL on every row, no
//! ordering at all, under a `200`. The malformed shape is what made a silent
//! non-feature a loud one.
//!
//! **Why this needs a database.** The handler-level half is pinned on the required
//! `test` leg (`routes::rest::handler::search_relevance_tests`): a search with no
//! sort carries a relevance ordering, and it is not in the argument map. What only
//! PostgreSQL can answer is whether the SQL that becomes actually *orders* rows —
//! and that is the assertion that separates this fix from the alternative of
//! deleting the implicit ordering altogether, which would also stop the `400`.
//!
//! **Why the fixture discriminates.** Three rows carry the search term once, twice
//! and three times, in ascending `id`. `ts_rank` rises with term frequency, so the
//! ranked answer is `[3, 2, 1]` — the exact reverse of both the id order and the
//! insertion order. A read that dropped the ordering, or that fell back to any
//! natural order, answers `[1, 2, 3]` and fails. Measured on the rig before the
//! fixture was written: ranks 0.0827, 0.0760, 0.0608.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tf_p17_rank` fixture → run
//! `--test-threads=1`.

#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{sync::Arc, time::Duration};

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

const TABLE: &str = "tf_p17_rank";
const VIEW: &str = "v_p17_rank";

/// Any single request must finish well inside this.
const TIMEOUT: Duration = Duration::from_secs(30);

/// The ids of the three matching rows, most relevant first.
///
/// The reverse of their id order, deliberately: an unordered read, or one whose
/// ordering was dropped, answers them the other way round.
const BY_RELEVANCE: [i64; 3] = [3, 2, 1];

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

/// Seed rows whose relevance for `zebra` is strictly ordered, plus two rows the
/// search must not match at all.
async fn seed(adapter: &PostgresAdapter) {
    let stmts = vec![
        format!("DROP VIEW IF EXISTS {VIEW}"),
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!("CREATE TABLE {TABLE} (id bigint PRIMARY KEY, data jsonb NOT NULL)"),
        format!(
            "INSERT INTO {TABLE} (id, data) VALUES \
             (1, jsonb_build_object('id', 1, 'label', 'zebra one')), \
             (2, jsonb_build_object('id', 2, 'label', 'zebra zebra two')), \
             (3, jsonb_build_object('id', 3, 'label', 'zebra zebra zebra three')), \
             (4, jsonb_build_object('id', 4, 'label', 'giraffe four')), \
             (5, jsonb_build_object('id', 5, 'label', 'giraffe five'))"
        ),
        format!("CREATE VIEW {VIEW} AS SELECT id, data FROM {TABLE}"),
    ];

    for stmt in stmts {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup");
    }
}

/// A streamable route whose query declares `auto_params.has_where`.
///
/// That flag is what lets a client filter — and therefore lets the full-text
/// clause `?search=` builds reach the SQL at all (`resolve_direct_read` reads
/// `arguments["where"]` only when it is set). `TestQueryBuilder` leaves
/// `AutoParams::default()`, every field `false`, so a route that forgets it
/// accepts the search, validates it, drops it, and answers the whole relation
/// under a `200` — a fixture that would agree with a broken engine.
fn build_schema() -> CompiledSchema {
    let mut ranked = TestQueryBuilder::new("rankedDocs", "P17Doc")
        .returns_list(true)
        .with_sql_source(VIEW)
        .rest_stream(true)
        .build();
    ranked.rest_path = Some("/ranked".to_string());
    ranked.auto_params.has_where = true;

    let mut schema = TestSchemaBuilder::new()
        .with_type(
            TestTypeBuilder::new("P17Doc", VIEW)
                .with_field(TestFieldBuilder::new("id", FieldType::Int).build())
                .with_field(TestFieldBuilder::new("label", FieldType::String).build())
                .build(),
        )
        .with_query(ranked)
        .build();

    schema.rest_config = Some(RestConfig {
        enabled: true,
        ..RestConfig::default()
    });
    schema.build_indexes();
    schema
}

async fn start() -> Option<TestServer> {
    let url = try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.expect("connect to the test database");
    seed(&adapter).await;

    let config = ServerConfig {
        // #874: production validate() refuses cors_enabled=true + empty origins
        cors_enabled: false,
        ..ServerConfig::default()
    };

    Some(Box::pin(TestServer::start_with_config(config, build_schema(), Arc::new(adapter))).await)
}

async fn get(base: &str, query: &str, accept: &str) -> (reqwest::StatusCode, String) {
    let response = reqwest::Client::new()
        .get(format!("{base}/rest/v1/ranked{query}"))
        .header("accept", accept)
        .timeout(TIMEOUT)
        .send()
        .await
        .unwrap_or_else(|e| panic!("{accept} {query}: request failed: {e}"));
    let status = response.status();
    (status, response.text().await.expect("response body"))
}

/// The ids of a JSON collection response, in the order they were served.
fn json_ids(body: &str) -> Vec<i64> {
    let parsed: Value =
        serde_json::from_str(body).unwrap_or_else(|e| panic!("not JSON ({e}): {body}"));
    parsed["data"]
        .as_array()
        .unwrap_or_else(|| panic!("no data array: {body}"))
        .iter()
        .map(|row| {
            row.get("id")
                .and_then(Value::as_i64)
                .unwrap_or_else(|| panic!("row carries no id: {row}"))
        })
        .collect()
}

/// The `id` of every NDJSON line, in order.
fn ndjson_ids(body: &str) -> Vec<i64> {
    body.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let row: Value = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("NDJSON line is not JSON ({e}): {line}"));
            assert!(row.get("error").is_none(), "the export emitted an error line: {line}");
            row.get("id")
                .and_then(Value::as_i64)
                .unwrap_or_else(|| panic!("NDJSON row carries no id: {line}"))
        })
        .collect()
}

/// The first CSV column of every data row (header skipped), parsed as an id.
fn csv_ids(body: &str) -> Vec<i64> {
    let mut lines = body.trim_start_matches('\u{feff}').lines().filter(|l| !l.trim().is_empty());
    let header = lines.next().unwrap_or_else(|| panic!("the CSV export carried no header"));
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

// ---------------------------------------------------------------------------
// The contract
// ---------------------------------------------------------------------------

/// The defect itself, on the representation it was measured on.
///
/// Two assertions, and both are load-bearing: the status, because `400` is what
/// the request answered; and the *order*, because the cheapest way to stop the
/// `400` is to emit no ordering at all — which would leave the OpenAPI document
/// this server generates promising a ranking it does not perform.
#[tokio::test]
async fn a_search_with_no_sort_is_ranked_not_refused() {
    let Some(server) = start().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = get(&server.url, "?search=zebra", "application/json").await;
    assert_eq!(
        status,
        reqwest::StatusCode::OK,
        "`?search=` with no `?sort=` is the documented default path: {body}"
    );
    assert_eq!(
        json_ids(&body),
        BY_RELEVANCE.to_vec(),
        "rows come back most relevant first; id order ({:?}) means the ranking was dropped: \
         {body}",
        [1, 2, 3]
    );
}

/// The same request on the streaming representations, which the issue measured
/// answering the identical `400`.
///
/// The ordering is decided before a representation is chosen, so these cannot
/// differ from the JSON case — which is exactly why they are here: a repair that
/// touched only the JSON path would pass the case above.
#[tokio::test]
async fn every_representation_ranks_the_same_search() {
    let Some(server) = start().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = get(&server.url, "?search=zebra", "application/x-ndjson").await;
    assert_eq!(status, reqwest::StatusCode::OK, "NDJSON: {body}");
    assert_eq!(ndjson_ids(&body), BY_RELEVANCE.to_vec(), "NDJSON: {body}");

    let (status, body) = get(&server.url, "?search=zebra", "text/csv").await;
    assert_eq!(status, reqwest::StatusCode::OK, "CSV: {body}");
    assert_eq!(csv_ids(&body), BY_RELEVANCE.to_vec(), "CSV: {body}");
}

/// A client that named a sort gets that sort — "ranked by relevance **unless**
/// `sort` is specified", as the generated document says.
///
/// This is the half that must not move. It is the spelling that worked before the
/// fix, and a repair that ranked unconditionally would break it while passing
/// every case above.
#[tokio::test]
async fn an_explicit_sort_still_wins() {
    let Some(server) = start().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = get(&server.url, "?search=zebra&sort=id", "application/json").await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    assert_eq!(
        json_ids(&body),
        vec![1, 2, 3],
        "the client asked for id order, which is the reverse of the ranking: {body}"
    );
}

/// The search still narrows: ranking every row and filtering none would satisfy
/// the order assertions above on this fixture only by accident, and would be a
/// far worse defect than the one being fixed.
#[tokio::test]
async fn the_ranked_read_still_filters() {
    let Some(server) = start().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = get(&server.url, "?search=zebra", "application/json").await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    assert_eq!(json_ids(&body).len(), 3, "the two `giraffe` rows must not match: {body}");

    let (status, body) = get(&server.url, "?search=giraffe", "application/json").await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    assert_eq!(json_ids(&body), vec![4, 5], "a different term selects the other rows: {body}");
}

/// The search text is a **bound parameter**, not part of the ORDER BY string.
///
/// The ranking is the first place a client's raw text reaches an `ORDER BY`, and
/// an `ORDER BY` is one of the few SQL positions that takes no parameter in most
/// hand-rolled builders — which is how such a value ends up escaped into the
/// statement. A term full of quotes and semicolons must be answered as a term:
/// `200`, no rows, and a database that is still there afterwards.
#[tokio::test]
async fn a_hostile_search_term_is_a_parameter_and_not_sql() {
    let Some(server) = start().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = get(
        &server.url,
        "?search=%27%29%3B%20DROP%20TABLE%20tf_p17_rank%3B%20--",
        "application/json",
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "a hostile term is a term: {body}");
    assert!(json_ids(&body).is_empty(), "it matches nothing: {body}");

    // The fixture is intact, which is the assertion the case exists for.
    let (status, body) = get(&server.url, "?search=zebra", "application/json").await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    assert_eq!(json_ids(&body), BY_RELEVANCE.to_vec(), "{body}");
}

/// The ranking survives pagination, which is where the bound parameter could
/// collide.
///
/// `ORDER BY` sits between `WHERE` and `LIMIT`/`OFFSET`, so introducing a
/// parameter there renumbers everything after it. Get that wrong and the search
/// term binds as the limit — or the limit binds as the search term — and the
/// answer is a wrong page under a `200` rather than an error. The first case
/// above already exercises `WHERE → ORDER BY → LIMIT` (the default page size is
/// always applied); this adds the `OFFSET` placeholder behind it.
#[tokio::test]
async fn a_ranked_page_binds_its_parameters_in_order() {
    let Some(server) = start().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = get(&server.url, "?search=zebra&limit=2", "application/json").await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    assert_eq!(json_ids(&body), vec![3, 2], "the two most relevant: {body}");

    let (status, body) =
        get(&server.url, "?search=zebra&limit=2&offset=1", "application/json").await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    assert_eq!(json_ids(&body), vec![2, 1], "the ranking, one row in: {body}");
}
