//! #1303: an offset page is a slice of a *sequence*, and an unordered read is
//! not a sequence.
//!
//! `LIMIT`/`OFFSET` asks the database for rows *n* through *m* of an ordering.
//! With no `ORDER BY` there is no ordering, so PostgreSQL is free to return the
//! same relation differently for each page — a client walking `?offset=` sees
//! some rows twice and never sees others, under `200`, with no error anywhere.
//! #1287 closed the half of that where the client ordered by something non-unique;
//! this closes the half where it ordered by nothing at all, by having the compiler
//! record a total order per query (`QueryDefinition::pagination_order`) and the
//! runtime apply it.
//!
//! **Why this needs a database.** The lowering itself is pinned on the required
//! `test` leg (`query_runner_tests::pagination_order`): which clause reaches the
//! adapter, for which request shape. Three things only PostgreSQL can answer:
//!
//! 1. that the emitted SQL *executes* — in particular `ORDER BY pk_p18doc ASC`, the native-column
//!    branch, whose whole value is that it is a real column;
//! 2. that a walk of three pages returns every row once, which is the property the issue is about
//!    and which no unit test can observe;
//! 3. that a view's own `ORDER BY` survives when the author declares `pagination_order = "none"` —
//!    the one case this change would otherwise make *worse*, and the reason the opt-out exists.
//!
//! **Why the fixture discriminates.** Rows are inserted in an order deliberately
//! unrelated to their identity: `seq` counts 1…300 in insertion order while `id`
//! is `doc-001`…`doc-300` assigned by a fixed permutation, so physical order and
//! identity order disagree on all but a handful of rows. A read that emits no
//! ordering answers in physical order and fails the very first assertion; one that
//! orders by the declared native `pk_p18doc` answers in insertion order, which
//! differs from identity order — so the two branches cannot pass each other's
//! cases. `status` takes four values over 300 rows, which is the shape #1287
//! measured on 2 000 rows: three pages of 100 returned 300 rows of which **156
//! were distinct**. Measured on *this* fixture with both the runtime lowering and
//! the renderer's tie-breaker disabled: **288 distinct of 300** — the overlap
//! reproduces, and the walk is not passing because 300 rows happen to be stable.
//!
//! Identity ordering is TEXT ordering (`data->>'id'`), which is why the ids are
//! zero-padded: `doc-002` before `doc-010` either way, so the assertion pins the
//! ordering rather than a lexicographic accident.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tf_p18_page` fixture → run
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

const TABLE: &str = "tf_p18_page";
const VIEW: &str = "v_p18_page";
const ORDERED_VIEW: &str = "v_p18_ordered";

/// Rows in the fixture, and the walk that must return each exactly once.
const ROWS: usize = 300;
const PAGE: usize = 100;

/// Any single request must finish well inside this.
const TIMEOUT: Duration = Duration::from_secs(30);

/// The permutation that decouples insertion order from identity order.
///
/// `97` is coprime with 300, so `n ↦ (n·97 mod 300) + 1` is a bijection on
/// 1…300 with no short cycles: insertion order begins `doc-001`, `doc-098`,
/// `doc-195`, `doc-292`, `doc-089` — only a handful of rows land on their own
/// index, which is what makes physical order and identity order distinguishable.
fn identity_of(insertion_index: usize) -> String {
    format!("doc-{:03}", (insertion_index * 97) % ROWS + 1)
}

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

async fn seed(adapter: &PostgresAdapter) {
    let values: Vec<String> = (0..ROWS)
        .map(|i| {
            let seq = i + 1;
            let id = identity_of(i);
            // Four statuses over 300 rows: the non-unique sort key whose ties a
            // page boundary is free to reorder.
            let status = ["open", "closed", "pending", "archived"][i % 4];
            format!("({seq}, jsonb_build_object('id', '{id}', 'seq', {seq}, 'status', '{status}'))")
        })
        .collect();

    let stmts = vec![
        format!("DROP VIEW IF EXISTS {ORDERED_VIEW}"),
        format!("DROP VIEW IF EXISTS {VIEW}"),
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!(
            "CREATE TABLE {TABLE} (pk_p18doc bigint PRIMARY KEY, data jsonb NOT NULL, \
             id text GENERATED ALWAYS AS (data->>'id') STORED)"
        ),
        format!("INSERT INTO {TABLE} (pk_p18doc, data) VALUES {}", values.join(", ")),
        format!("CREATE VIEW {VIEW} AS SELECT pk_p18doc, id, data FROM {TABLE}"),
        // A view that orders itself — the compiler's own documented remedy for
        // `order_by = false`, and the shape a default page ordering destroys.
        format!(
            "CREATE VIEW {ORDERED_VIEW} AS SELECT pk_p18doc, id, data FROM {TABLE} \
             ORDER BY (data->>'seq')::int DESC"
        ),
    ];

    for stmt in stmts {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup");
    }
}

/// Four routes over the same rows, differing only in the ordering their query
/// declares — which is the variable under test.
fn build_schema() -> CompiledSchema {
    // The derived default: no author override, no `--database`, so the compiler's
    // answer is the JSONB identity.
    let mut paged = TestQueryBuilder::new("pagedDocs", "P18Doc")
        .returns_list(true)
        .with_sql_source(VIEW)
        .build();
    paged.rest_path = Some("/paged".to_string());

    // An authored native column — the branch a compile with `--database` reaches,
    // and the one whose SQL only a database can validate.
    let mut pk_paged = TestQueryBuilder::new("pkPagedDocs", "P18Doc")
        .returns_list(true)
        .with_sql_source(VIEW)
        .pagination_order("pk_p18doc")
        .build();
    pk_paged.rest_path = Some("/pkpaged".to_string());

    // The declared opt-out over a self-ordering view.
    let mut view_ordered = TestQueryBuilder::new("viewOrderedDocs", "P18Doc")
        .returns_list(true)
        .with_sql_source(ORDERED_VIEW)
        .build();
    view_ordered.rest_path = Some("/vieworder".to_string());
    view_ordered.pagination_order = None;

    // The same view *without* the opt-out, which is what makes the case above
    // mean something: if both answered the same rows, the opt-out would be
    // decoration.
    let mut view_identity = TestQueryBuilder::new("viewIdentityDocs", "P18Doc")
        .returns_list(true)
        .with_sql_source(ORDERED_VIEW)
        .build();
    view_identity.rest_path = Some("/viewidentity".to_string());

    let mut schema = TestSchemaBuilder::new()
        .with_type(
            TestTypeBuilder::new("P18Doc", VIEW)
                .with_field(TestFieldBuilder::new("id", FieldType::String).build())
                .with_field(TestFieldBuilder::new("seq", FieldType::Int).build())
                .with_field(TestFieldBuilder::new("status", FieldType::String).build())
                .build(),
        )
        .with_query(paged)
        .with_query(pk_paged)
        .with_query(view_ordered)
        .with_query(view_identity)
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

async fn get(base: &str, route: &str, query: &str) -> (reqwest::StatusCode, String) {
    let response = reqwest::Client::new()
        .get(format!("{base}/rest/v1/{route}{query}"))
        .header("accept", "application/json")
        .timeout(TIMEOUT)
        .send()
        .await
        .unwrap_or_else(|e| panic!("{route}{query}: request failed: {e}"));
    let status = response.status();
    (status, response.text().await.expect("response body"))
}

/// The `id`s of a JSON collection response, in the order they were served.
fn ids(body: &str) -> Vec<String> {
    let parsed: Value =
        serde_json::from_str(body).unwrap_or_else(|e| panic!("not JSON ({e}): {body}"));
    parsed["data"]
        .as_array()
        .unwrap_or_else(|| panic!("no data array: {body}"))
        .iter()
        .map(|row| {
            row.get("id")
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("row carries no id: {row}"))
                .to_string()
        })
        .collect()
}

/// Walk the whole relation in pages of [`PAGE`] and return every row served.
async fn walk(base: &str, route: &str, extra: &str) -> Vec<String> {
    let mut seen = Vec::new();
    for offset in (0..ROWS).step_by(PAGE) {
        let (status, body) =
            get(base, route, &format!("?limit={PAGE}&offset={offset}{extra}")).await;
        assert_eq!(status, reqwest::StatusCode::OK, "{route} page at {offset}: {body}");
        let page = ids(&body);
        assert_eq!(page.len(), PAGE, "{route} page at {offset} was short: {body}");
        seen.extend(page);
    }
    seen
}

fn distinct(rows: &[String]) -> usize {
    rows.iter().collect::<std::collections::BTreeSet<_>>().len()
}

// ---------------------------------------------------------------------------
// The contract
// ---------------------------------------------------------------------------

/// A walk with no sort of its own returns every row exactly once, in identity
/// order.
///
/// Both halves are load-bearing. *Exactly once* is the property #1303 is about.
/// *In identity order* is what makes the case discriminate on this fixture: rows
/// were inserted in an order unrelated to their identity, so an unordered read
/// answers in physical order and fails, while a read that merely happened to be
/// stable would pass a distinctness check alone.
#[tokio::test]
async fn an_unsorted_walk_returns_every_row_once_in_identity_order() {
    let Some(server) = start().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let walked = walk(&server.url, "paged", "").await;
    assert_eq!(walked.len(), ROWS);
    assert_eq!(
        distinct(&walked),
        ROWS,
        "three pages returned {} distinct rows of {ROWS}; the pre-fix engine measured 156",
        distinct(&walked)
    );

    let expected: Vec<String> = (1..=ROWS).map(|n| format!("doc-{n:03}")).collect();
    assert_eq!(
        walked,
        expected,
        "the walk must be in identity order; insertion order starts {:?}",
        (0..5).map(identity_of).collect::<Vec<_>>()
    );
}

/// The same walk over a non-unique client sort — #1287's shape, now tie-broken by
/// the *declared* identity rather than a hard-coded `data->>'id'`.
///
/// `status` takes four values over 300 rows, so every page boundary falls inside a
/// tie. Measured on the pre-#1287 engine: 300 rows walked, 156 distinct.
#[tokio::test]
async fn a_walk_over_a_non_unique_sort_returns_every_row_once() {
    let Some(server) = start().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let walked = walk(&server.url, "paged", "&sort=status").await;
    assert_eq!(walked.len(), ROWS);
    assert_eq!(
        distinct(&walked),
        ROWS,
        "{} distinct rows of {ROWS} — the pages overlap, which is #1287",
        distinct(&walked)
    );
}

/// A declared native column orders the pages, and the SQL it emits runs.
///
/// The assertion is insertion order — `pk_p18doc` counts 1…300 in insertion
/// order, which the fixture made differ from identity order — so this case cannot
/// be passed by the JSONB identity, and the identity case cannot be passed by
/// this. That is the whole point of recording the column at compile time: it is
/// the index-scannable one, and nothing at request time can see it.
#[tokio::test]
async fn a_declared_native_column_orders_the_pages() {
    let Some(server) = start().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let walked = walk(&server.url, "pkpaged", "").await;
    assert_eq!(walked.len(), ROWS);
    assert_eq!(distinct(&walked), ROWS, "{} distinct of {ROWS}", distinct(&walked));

    let by_insertion: Vec<String> = (0..ROWS).map(identity_of).collect();
    assert_eq!(
        walked, by_insertion,
        "ordered by pk_p18doc, which is insertion order — not the identity's"
    );
}

/// `pagination_order = "none"` keeps a self-ordering view's own `ORDER BY`.
///
/// This is the one case the default makes worse, and the reason the opt-out is
/// declared rather than inferred: the compiler cannot read a view's body, and a
/// default ordering *replaces* the author's rather than adding to it. The view
/// orders by `seq` descending, so its first page is the last rows inserted.
#[tokio::test]
async fn a_declared_none_keeps_the_views_own_order() {
    let Some(server) = start().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = get(&server.url, "vieworder", "?limit=5").await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");

    let expected: Vec<String> = (ROWS - 5..ROWS).rev().map(identity_of).collect();
    assert_eq!(ids(&body), expected, "the view's own ORDER BY must survive: {body}");
}

/// The same view *without* the opt-out answers differently — which is what makes
/// the case above an assertion rather than a description of the fixture.
///
/// Without this, a build in which the ordering was never applied at all would pass
/// `a_declared_none_keeps_the_views_own_order` and read as a working opt-out.
#[tokio::test]
async fn without_the_opt_out_the_same_view_is_reordered() {
    let Some(server) = start().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = get(&server.url, "viewidentity", "?limit=5").await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");

    let expected: Vec<String> = (1..=5).map(|n| format!("doc-{n:03}")).collect();
    assert_eq!(ids(&body), expected, "the identity ordering must win here: {body}");

    let (_, opted_out) = get(&server.url, "vieworder", "?limit=5").await;
    assert_ne!(
        ids(&body),
        ids(&opted_out),
        "the two routes read the same view and must answer differently, or neither \
         assertion means anything"
    );
}

/// Over REST there is **no unpaged read**, so every list route is ordered.
///
/// `resolve_pagination` fills an absent `?limit=` with `RestConfig::default_page_size`
/// (100), so a bare `GET /rest/v1/<resource>` is already a page — the first one —
/// and the ordering applies to it. Measured here rather than assumed, because it
/// decides the reach of this change: the breaking surface is every REST list
/// route, not only the ones a client paginates explicitly.
///
/// The runtime gate that leaves a genuinely unpaged read alone still exists and
/// is what bounds the cost; it is reachable over GraphQL, where `limit` is only
/// what the client sent, and is pinned there
/// (`query_runner_tests::pagination_order::an_unpaged_read_is_still_unordered`).
#[tokio::test]
async fn a_bare_rest_read_is_a_page_and_is_ordered() {
    let Some(server) = start().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = get(&server.url, "viewidentity", "").await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");

    let served = ids(&body);
    assert_eq!(served.len(), PAGE, "a bare GET is the first page, not the whole relation");

    let expected: Vec<String> = (1..=PAGE).map(|n| format!("doc-{n:03}")).collect();
    assert_eq!(
        served, expected,
        "and it is ordered by the identity, not by the view's own ORDER BY: {body}"
    );
}
