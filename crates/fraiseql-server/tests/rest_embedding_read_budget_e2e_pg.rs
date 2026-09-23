//! `[rest] max_embedded_reads` — the aggregate bound on what one `?select=` request reads.
//!
//! Embedding resolves a relationship with **one sub-read per parent row**, recursing per
//! row, so the reads one request performs are the *product* of the page sizes at each
//! level. Every control that existed before this one sees a single sub-read — the cost
//! gate scores one, `[validation] max_response_bytes` charges one, `[rest] max_page_size`
//! clamps one — and each sub-read here is individually cheap, so the product was
//! unbounded: `?select=a(b(c))&limit=1000` is ~10⁹ sequential sub-reads, each holding a
//! pool connection.
//!
//! **Why a real database, and why this file exists at all.** The budget's arithmetic is
//! unit-tested next to the type. What cannot be unit-tested is the thing that actually
//! goes wrong: whether one tally is *shared* by every level and both passes of a request.
//! `EmbeddingRequest` is rebuilt for each nested level (`embedding/mod.rs`) and
//! `EmbedCtx` is rebuilt for the rows pass and again for the counts pass, and a budget
//! rebuilt along with either of them would bound each piece and leave their product
//! unbounded — a control that reads as enforced and is not. Only a served request can
//! tell the two apart, so the sharing is asserted here, through the wire:
//!
//! * `rows_and_counts_of_one_request_share_one_tally` fails if the counts pass gets its own budget
//! * `every_nesting_level_charges_the_same_tally` fails if the nested request gets its own budget
//!
//! Both were confirmed to fail under exactly those mutations — see the commit body.
//!
//! The refusal is a `413`, before the response is assembled, rather than a short answer:
//! an embed served in part is indistinguishable from a parent that genuinely has fewer
//! related rows, which is #1230's failure shape under a `200`.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** each rig drops and recreates its own `p1351_budget` schema → run
//! `--test-threads=1`.
#![cfg(feature = "rest")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::sync::Arc;

use axum::body::Body;
use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::{
    db::postgres::PostgresAdapter, prelude::DatabaseAdapter as _, runtime::Executor,
    schema::CompiledSchema,
};
use fraiseql_server::routes::{
    graphql::AppState,
    rest::{RestMountConfig, rest_query_router},
};
use fraiseql_test_support::try_database_url;
use http::{Request, StatusCode};
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;

const SCHEMA: &str = "p1351_budget";

/// Two users with two orders each, and relationships declared in both directions.
///
/// The second direction is what makes a two-level embed reachable at all:
/// `users?select=orders(user(...))` walks `User -> Order -> User`, which is the shape
/// whose read count is a product rather than a sum. The fixture's sizes are chosen so
/// every request below has an exactly known cost:
///
/// | request | sub-reads |
/// |---|---|
/// | `?select=id,orders(id)` | 2 — one per user |
/// | `?select=id,orders.count` | 2 — one per user |
/// | `?select=id,orders(id),orders.count` | 4 — both passes, one each per user |
/// | `?select=id,orders(id,user(name))` | 6 — 2 for the orders, then one per order |
fn fraiseql_toml(max_embedded_reads: u64) -> String {
    format!(
        r#"
[schema]
name = "budget-1351"
version = "1.0.0"
database_target = "postgresql"

[rest]
enabled = true
max_embedded_reads = {max_embedded_reads}

[types.User]
sql_source = "{SCHEMA}.v_user"
fields.id = {{ type = "Int" }}
fields.name = {{ type = "String" }}

[types.User.relationships.orders]
target_type = "Order"
cardinality = "OneToMany"
foreign_key = "fk_user"
referenced_key = "id"

[types.Order]
sql_source = "{SCHEMA}.v_order"
fields.id = {{ type = "Int" }}
fields.fk_user = {{ type = "Int" }}
fields.total = {{ type = "Int" }}

[types.Order.relationships.user]
target_type = "User"
cardinality = "ManyToOne"
foreign_key = "fk_user"
referenced_key = "id"

[queries.users]
return_type = "User"
return_array = true
sql_source = "{SCHEMA}.v_user"

[queries.orders]
return_type = "Order"
return_array = true
sql_source = "{SCHEMA}.v_order"
"#
    )
}

async fn seed(adapter: &PostgresAdapter) {
    let stmts = vec![
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!("CREATE TABLE {SCHEMA}.tb_user (id bigint PRIMARY KEY, name text NOT NULL)"),
        format!(
            "CREATE TABLE {SCHEMA}.tb_order (id bigint PRIMARY KEY, fk_user bigint NOT NULL, \
             total bigint NOT NULL)"
        ),
        format!("INSERT INTO {SCHEMA}.tb_user VALUES (1, 'alice'), (2, 'bob')"),
        format!(
            "INSERT INTO {SCHEMA}.tb_order VALUES (10, 1, 100), (11, 1, 101), (20, 2, 200), \
             (21, 2, 201)"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_user AS SELECT id, jsonb_build_object('id', id, 'name', name) \
             AS data FROM {SCHEMA}.tb_user ORDER BY id"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_order AS SELECT id, jsonb_build_object('id', id, 'fk_user', \
             fk_user, 'total', total) AS data FROM {SCHEMA}.tb_order ORDER BY id"
        ),
    ];

    for stmt in stmts {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup");
    }
}

struct Rig {
    router:    axum::Router,
    _temp_dir: TempDir,
}

impl Rig {
    async fn get(&self, uri: &str) -> (StatusCode, Value) {
        let response = self
            .router
            .clone()
            .oneshot(Request::builder().method("GET").uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json = serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"raw": String::from_utf8_lossy(&bytes)}));
        (status, json)
    }
}

/// Compile the document above with the real compiler and mount the REST router on it.
///
/// Goes through the compiler rather than hand-building a `RestConfig` deliberately: the
/// defect class this control belongs to is a knob an operator can write, that parses, and
/// that does nothing. A rig that set the field directly would pass against exactly that.
async fn rig(max_embedded_reads: u64) -> Option<Rig> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    seed(&adapter).await;

    let temp_dir = TempDir::new().expect("temp dir");
    let toml_path = temp_dir.path().join("fraiseql.toml");
    std::fs::write(&toml_path, fraiseql_toml(max_embedded_reads)).expect("write fraiseql.toml");

    let (compiled, _) = compile_to_schema(CompileOptions {
        skip_hash: true,
        ..CompileOptions::new(toml_path.to_str().expect("utf-8 path"))
    })
    .await
    .expect("the authored document must compile");

    let mut schema = CompiledSchema::from_json(
        &compiled.schema.to_json().expect("serialize the compiled artifact"),
        false,
    )
    .expect("the compiler's own output must survive load");
    schema.build_indexes();

    let executor = Arc::new(Executor::new(schema.clone(), adapter));
    let state = AppState::new(executor);
    let router = rest_query_router(&state, &RestMountConfig::default()).expect("REST router");

    Some(Rig {
        router,
        _temp_dir: temp_dir,
    })
}

/// The refusal a crossed budget owes the client.
fn assert_refused(status: StatusCode, body: &Value) {
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "expected the budget refusal: {body}");
    assert_eq!(
        body.get("error").and_then(|e| e.get("code")).and_then(Value::as_str),
        Some("TOO_MANY_EMBEDDED_READS"),
        "refused by the budget rather than by something else: {body}"
    );
    assert!(
        body.get("error")
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
            .is_some_and(|m| m.contains("max_embedded_reads")),
        "the message names the knob an operator would raise: {body}"
    );
}

/// The accepted half of the pair. A budget that refused every embed would pass a test
/// that only asserted the refusal, so the same request under a sufficient budget is
/// asserted to serve the *right rows* — not merely a 200.
#[tokio::test]
async fn an_embed_within_its_budget_is_served_in_full() {
    let Some(rig) = rig(2).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/users?select=id,orders(id,total)").await;
    assert_eq!(status, StatusCode::OK, "two reads against a budget of two: {body}");

    let rows = body.get("data").and_then(Value::as_array).unwrap();
    assert_eq!(rows.len(), 2, "both users: {body}");
    for row in rows {
        assert_eq!(
            row.get("orders").and_then(Value::as_array).map(Vec::len),
            Some(2),
            "every parent's orders are all there — a budget must not truncate: {body}"
        );
    }
}

/// The rejected half. One read fewer than the request needs, and it is refused rather
/// than answered with the one parent that fitted.
#[tokio::test]
async fn an_embed_that_would_cross_its_budget_is_refused() {
    let Some(rig) = rig(1).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/users?select=id,orders(id,total)").await;
    assert_refused(status, &body);
}

/// **The counts pass charges the same tally as the rows pass.**
///
/// Four sub-reads against a budget of two. Each pass on its own would fit, so this is
/// refused only if they share one budget — which is the point: they are reads issued
/// against the same pool on behalf of the same request. A `RestHandler` that built a
/// second budget for `execute_embedding_counts` answers `200` here.
#[tokio::test]
async fn rows_and_counts_of_one_request_share_one_tally() {
    let Some(rig) = rig(2).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    // Each pass alone fits in two.
    let (status, body) = rig.get("/rest/v1/users?select=id,orders(id)").await;
    assert_eq!(status, StatusCode::OK, "the rows pass alone fits: {body}");
    let (status, body) = rig.get("/rest/v1/users?select=id,orders.count").await;
    assert_eq!(status, StatusCode::OK, "the counts pass alone fits: {body}");

    // Together they are four, and four does not.
    let (status, body) = rig.get("/rest/v1/users?select=id,orders(id),orders.count").await;
    assert_refused(status, &body);
}

/// **Every nesting level charges the same tally.**
///
/// Six sub-reads against a budget of **four**: two for the users' orders, then one per
/// order for its user.
///
/// The budget is four rather than two on purpose, and the number is the whole test.
/// Each level fits in four on its own — level one needs two, level two needs four — so
/// only a tally *shared across levels* refuses this request. At a budget of two the test
/// would pass against a budget rebuilt per level too, since level two alone would already
/// exceed it; that version was written first and confirmed to survive the mutation, which
/// is what this comment exists to stop someone re-introducing.
#[tokio::test]
async fn every_nesting_level_charges_the_same_tally() {
    let Some(rig) = rig(4).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    // Level one on its own is two reads, well inside four.
    let (status, body) = rig.get("/rest/v1/users?select=id,orders(id)").await;
    assert_eq!(status, StatusCode::OK, "one level alone fits in four: {body}");

    // Both levels are six, and six does not.
    let (status, body) = rig.get("/rest/v1/users?select=id,orders(id,user(name))").await;
    assert_refused(status, &body);
}

/// `0` is the documented no-bound setting, and it has to actually mean it: the six-read
/// request above is served in full, nested embed included.
#[tokio::test]
async fn a_zero_budget_is_unbounded() {
    let Some(rig) = rig(0).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/users?select=id,orders(id,user(name))").await;
    assert_eq!(status, StatusCode::OK, "zero means no bound: {body}");

    let rows = body.get("data").and_then(Value::as_array).unwrap();
    let alice = rows
        .iter()
        .find(|r| r.get("id").and_then(Value::as_i64) == Some(1))
        .unwrap_or_else(|| panic!("alice: {body}"));
    let first_order = alice.get("orders").and_then(Value::as_array).unwrap().first().unwrap();
    assert_eq!(
        first_order.get("user").and_then(|u| u.get("name")).and_then(Value::as_str),
        Some("alice"),
        "the second level really executed: {body}"
    );
}
