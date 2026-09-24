//! The per-request bounds on one `?select=` request: `[validation] max_response_bytes` on
//! the bytes it returns and `[security.cost_budget] per_request_max` on the work it asks
//! for.
//!
//! **One statement.** An embed is composed into the parent's statement — each embedded
//! level a correlated `LATERAL` subquery with its own page — so a request is one read,
//! and the two ceilings bound it the way they bound any read. Until this change it was a
//! parent read plus one sub-read per parent row per level, and this suite's job was to
//! prove that three budgets (those two and `[rest] max_embedded_reads`, a tally of the
//! sub-reads) were *shared* by every read of the request. There is nothing left to share
//! a budget with, and the tally is retired with the fan-out it counted.
//!
//! What remains to prove is that the one read is charged for **everything it composes**:
//!
//! * the cost of the whole tree, scored before the statement is sent — every nesting level, and
//!   every embedded `.count`, which the fan-out's `count_rows` never charged;
//! * the bytes of the embedded rows, not only the parent's.
//!
//! **Every figure is bracketed.** Each ceiling test serves the request at exactly the
//! predicted charge and refuses it one below, so the number is pinned from both sides and
//! read off the running system rather than restated from the arithmetic that predicted it.
//!
//! **The composed score is a bound, not a measurement.** A composed statement is charged
//! its full page at every level, whatever rows exist. The fan-out stopped at the rows
//! that existed, so on a small table it charged less: `users?select=id,orders(id,total)`
//! over this fixture's two users was charged 503 and is now charged 10 201 — a full
//! default page of 100 users, each with a full `default_embed_page_size` page of 50
//! orders. At a **full** page the two are equal by construction (`4369e56e2`), which
//! `the_composed_statement_is_charged_what_the_fan_out_was` reads off the fixture;
//! `an_unpaged_embed_is_charged_its_full_page` states the other half, so the change in
//! what a deployment's ceiling admits is pinned rather than discovered.
//!
//! The cost refusal is a `400` for the reason its variant documents — a per-request
//! ceiling is permanent for the request as issued, where a spent rolling window would be
//! a retryable `429`. The bytes refusal is a `413`, before the response is assembled:
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

use std::{collections::HashMap, sync::Arc};

use axum::body::Body;
use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    graphql::{DirectReadProjection, estimate_direct_read_cost},
    prelude::DatabaseAdapter as _,
    runtime::Executor,
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
/// `users?select=orders(user(...))` walks `User -> Order -> User`. Two users is what
/// makes `?limit=2` a **full** page, where the composed charge is exact rather than a
/// bound.
fn fraiseql_toml(max_response_bytes: Option<u64>, per_request_max: Option<u64>) -> String {
    // Omitted rather than set to a sentinel when absent: "no ceiling declared" is the
    // shape the engine distinguishes, and a rig that always wrote a number could not
    // exercise it.
    let validation = match max_response_bytes {
        Some(bytes) => format!("\n[validation]\nmax_response_bytes = {bytes}\n"),
        None => String::new(),
    };
    let cost_budget = match per_request_max {
        Some(max) => format!("\n[security.cost_budget]\nper_request_max = {max}\n"),
        None => String::new(),
    };
    format!(
        r#"
[schema]
name = "budget-1351"
version = "1.0.0"
database_target = "postgresql"
{validation}{cost_budget}
[rest]
enabled = true

[types.User]
sql_source = "{SCHEMA}.v_user"
fields.id = {{ type = "ID" }}
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

/// The rig, with a `[security.cost_budget] per_request_max` ceiling and no other bound.
async fn rig_cost(per_request_max: u64) -> Option<Rig> {
    rig_with(None, Some(per_request_max)).await
}

/// The rig, with whichever of the two ceilings the caller declares.
///
/// Compiles the document with the real compiler rather than hand-building a config
/// deliberately: the defect class these controls belong to is a knob an operator can
/// write, that parses, and that does nothing. A rig that set the field directly would pass
/// against exactly that.
async fn rig_with(max_response_bytes: Option<u64>, per_request_max: Option<u64>) -> Option<Rig> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    seed(&adapter).await;

    let (schema, temp_dir) = compile_document(max_response_bytes, per_request_max).await;

    // The schema-derived runtime config, not the default one. `Executor::new` takes
    // `RuntimeConfig::default()`, whose `max_response_bytes` is `None` however the
    // document declared it — so a rig built that way asserts a bytes ceiling that is not
    // in force, and passes just as happily with the charging removed altogether. Same
    // reason this rig compiles the document instead of hand-building a `RestConfig`.
    let runtime_config = fraiseql_core::runtime::RuntimeConfig::from_compiled_schema(&schema)
        .expect("the compiled document must yield a runtime config");
    let executor = Arc::new(Executor::with_config(schema, adapter, runtime_config));
    let state = AppState::new(executor);
    let router = rest_query_router(&state, &RestMountConfig::default()).expect("REST router");

    Some(Rig {
        router,
        _temp_dir: temp_dir,
    })
}

/// Compile the document above and load it the way a served artifact is loaded.
///
/// Its own function, and free of the database, so that
/// `the_document_loads_without_a_database` loads exactly what `rig_with` serves.
async fn compile_document(
    max_response_bytes: Option<u64>,
    per_request_max: Option<u64>,
) -> (CompiledSchema, TempDir) {
    let temp_dir = TempDir::new().expect("temp dir");
    let toml_path = temp_dir.path().join("fraiseql.toml");
    std::fs::write(&toml_path, fraiseql_toml(max_response_bytes, per_request_max))
        .expect("write fraiseql.toml");

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
    (schema, temp_dir)
}

/// Both levels of a two-level embed execute, and each serves the right rows — the
/// correctness baseline the ceilings below are asserted against.
#[tokio::test]
async fn a_nested_embed_is_served_in_full() {
    let Some(rig) = rig_with(None, None).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/users?select=id,orders(id,user(name))").await;
    assert_eq!(status, StatusCode::OK, "no ceiling declared: {body}");

    let rows = body.get("data").and_then(Value::as_array).unwrap();
    assert_eq!(rows.len(), 2, "both users: {body}");
    for (user, name) in [(1, "alice"), (2, "bob")] {
        let row = rows
            .iter()
            .find(|r| r.get("id").and_then(Value::as_i64) == Some(user))
            .unwrap_or_else(|| panic!("user {user}: {body}"));
        let orders = row.get("orders").and_then(Value::as_array).unwrap();
        assert_eq!(orders.len(), 2, "every parent's orders, and only its own: {body}");
        for order in orders {
            assert_eq!(
                order.get("user").and_then(|u| u.get("name")).and_then(Value::as_str),
                Some(name),
                "the second level resolved against its own parent row: {body}"
            );
        }
    }
}

// ── `[validation] max_response_bytes` ──

/// The refusal a crossed bytes ceiling owes the client.
fn assert_too_large(status: StatusCode, body: &Value) {
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "expected the bytes refusal: {body}");
    assert_eq!(
        body.get("error").and_then(|e| e.get("code")).and_then(Value::as_str),
        Some("RESPONSE_TOO_LARGE"),
        "the code agrees with the status rather than reporting a server fault: {body}"
    );
    assert!(
        body.get("error")
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
            .is_some_and(|m| m.contains("max_response_bytes")),
        "the message names the knob an operator would raise: {body}"
    );
}

/// **The embedded rows are charged, not only the parent's.**
///
/// `[validation] max_response_bytes` bounds *a response*, and this representation's
/// response is the parent rows plus everything embedded into them. On this fixture the
/// parent rows alone are charged 84 bytes and the composed statement 424 — the parent's
/// documents plus each order's `id` and `total`. Served at exactly 424 and refused at 423,
/// with the parent read alone served at that same 423: a statement charged for anything
/// less than its embeds fits under it.
///
/// (The fan-out this replaces was charged 436 for the same answer: each sub-read returned
/// the whole order document, `fk_user` included. An embedded level now returns the keys
/// selected and nothing else.)
#[tokio::test]
async fn the_embedded_rows_are_charged_with_the_parent_rows() {
    let uri = "/rest/v1/users?select=id,orders(id,total)";

    let Some(rig) = rig_with(Some(424), None).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let (status, body) = rig.get(uri).await;
    assert_eq!(status, StatusCode::OK, "424 bytes fit under 424: {body}");

    let Some(rig) = rig_with(Some(423), None).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let (status, body) = rig.get("/rest/v1/users?select=id").await;
    assert_eq!(status, StatusCode::OK, "the parent rows alone fit under 423: {body}");
    let (status, body) = rig.get(uri).await;
    assert_too_large(status, &body);
}

/// The accepted half. A ceiling the whole request fits under serves every parent's orders
/// in full — so the shared budget bounds the response rather than truncating it, which is
/// #1230's shape under a `200`.
#[tokio::test]
async fn an_embed_inside_the_bytes_ceiling_is_served_in_full() {
    let Some(rig) = rig_with(Some(10_000), None).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/users?select=id,orders(id,total)").await;
    assert_eq!(status, StatusCode::OK, "424 bytes against a ceiling of 10 000: {body}");

    let rows = body.get("data").and_then(Value::as_array).unwrap();
    assert_eq!(rows.len(), 2, "both users: {body}");
    for row in rows {
        assert_eq!(
            row.get("orders").and_then(Value::as_array).map(Vec::len),
            Some(2),
            "every parent's orders are all there: {body}"
        );
    }
}

// ── `[security.cost_budget] per_request_max` ──

/// The refusal a crossed cost ceiling owes the client.
fn assert_cost_refused(status: StatusCode, body: &Value) {
    assert_eq!(status, StatusCode::BAD_REQUEST, "expected the cost refusal: {body}");
    assert_eq!(
        body.get("error").and_then(|e| e.get("code")).and_then(Value::as_str),
        Some("BAD_REQUEST"),
        "permanent for the request as issued, not the 429 a spent rolling window earns: {body}"
    );
    assert!(
        body.get("error")
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
            .is_some_and(|m| m.contains("per_request_max")),
        "the message names the knob an operator would raise: {body}"
    );
}

/// Serve `uri` at a cost ceiling of exactly `charge` and refuse it at one below.
async fn assert_charged(uri: &str, charge: u64) {
    let Some(rig) = rig_cost(charge).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let (status, body) = rig.get(uri).await;
    assert_eq!(status, StatusCode::OK, "{uri} fits under its own charge of {charge}: {body}");

    let Some(rig) = rig_cost(charge - 1).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let (status, body) = rig.get(uri).await;
    assert_cost_refused(status, &body);
}

/// **The composed statement is charged what the fan-out it replaces was.**
///
/// `4369e56e2` measured the fan-out's total for this request through the wire — 405, a
/// parent read of `1 + 1×2` and two sub-reads of `1 + 2×100` — and showed that the
/// document path's arithmetic scores the composed shape at the same number. This is that
/// measurement repeated against the statement that replaced the fan-out: the ceiling an
/// operator wrote means the same request it did.
///
/// `?limit=2` against a fixture holding exactly two users is what makes the page **full**,
/// the case in which the two are equal. The fan-out's sub-reads were charged a page of 100
/// (the scorer caps a page's multiplier there), so the embed asks for that page,
/// `?orders.limit=100`; without it the level takes `default_embed_page_size`.
#[tokio::test]
async fn the_composed_statement_is_charged_what_the_fan_out_was() {
    let composed = DirectReadProjection {
        leaf_fields: 1,
        limit:       Some(2),
        nested:      vec![DirectReadProjection::flat(2, Some(100))],
    };
    let predicted = u64::try_from(estimate_direct_read_cost(
        "users",
        &HashMap::<String, usize>::new(),
        &composed,
    ))
    .expect("a cost fits in u64");
    assert_eq!(
        predicted, 405,
        "parent 1 + 1x2 = 3, plus two sub-reads of 1 + 2x100 = 201; stated as well as \
         derived, so a change that moved both sides of the identity still fails something"
    );

    assert_charged("/rest/v1/users?select=id,orders(id,total)&limit=2&orders.limit=100", predicted)
        .await;
}

/// **Every nesting level is in the score.**
///
/// `users?select=id,orders(id,user(name))&limit=2` is charged 5 205: two users, each with
/// a default page of 50 orders, each with a default page of 50 users. The first level
/// alone is 105, so a score that dropped the second level — or charged each level on its
/// own — serves this request at 5 204.
#[tokio::test]
async fn every_nesting_level_is_in_the_score() {
    assert_charged("/rest/v1/users?select=id,orders(id,user(name))&limit=2", 5_205).await;
}

/// **An embedded count is charged with the statement.**
///
/// A count is a level projecting nothing, which scores 1, so it adds one per parent row:
/// 107 where the same request without it is 105. The fan-out never charged it — counts
/// went through `count_rows`, which has no cost gate — and composed into the statement,
/// the work is the statement's.
#[tokio::test]
async fn an_embedded_count_is_charged_with_the_statement() {
    assert_charged("/rest/v1/users?select=id,orders(id),orders.count&limit=2", 107).await;
}

/// **An unpaged embed is charged its full page.**
///
/// The behavioural change this composition makes, pinned rather than discovered. Without
/// `?limit=` the parent page is the REST default of 100, and without `?orders.limit=` each
/// parent's embed is `default_embed_page_size`, 50; the composed statement is charged for
/// both pages whatever the table holds: `1 + 100 × (1 + 1 + 2 × 50)` = 10 201, where the
/// fan-out over this fixture's two users was charged 503. The ceiling is a bound, so the
/// composed statement is charged the most the request could read.
///
/// Before the embed had a default of its own it took `max_page_size`, and this request
/// was charged 20 201 — a full page of related rows for every parent, which no client
/// asked for.
#[tokio::test]
async fn an_unpaged_embed_is_charged_its_full_page() {
    assert_charged("/rest/v1/users?select=id,orders(id,total)", 10_201).await;
}

/// **A client that wants a bigger embed page asks for it, and is charged for it.**
///
/// `?orders.limit=100` pages the embed at 100: `1 + 100 × (1 + 1 + 2 × 100)` = 20 201.
/// The parent's `?limit=` is not the embed's page, so this is the only way to reach it.
#[tokio::test]
async fn an_embed_page_the_client_asks_for_is_charged() {
    assert_charged("/rest/v1/users?select=id,orders(id,total)&orders.limit=100", 20_201).await;
}

/// **The parent's `?limit=` does not page its embeds.**
///
/// `?limit=2` pages the users; each user's orders are still the default page of 50:
/// `1 + 2 × (1 + 1 + 2 × 50)` = 205. A `?limit=` that also paged the embed would charge
/// `1 + 2 × (1 + 1 + 2 × 2)` = 13 — and serve two orders per user to a client that asked
/// for two users.
#[tokio::test]
async fn the_parents_limit_does_not_page_its_embeds() {
    assert_charged("/rest/v1/users?select=id,orders(id,total)&limit=2", 205).await;
}

/// **Above the ceiling, an embed page is refused, not clamped.**
///
/// `[rest] max_page_size` is 1 000 here (the default). A clamp would serve 1 000 rows to
/// a client that asked for 1 001 under a `200`, indistinguishable from a relation that
/// has 1 000.
#[tokio::test]
async fn an_embed_page_above_the_ceiling_is_refused() {
    let Some(rig) = rig_with(None, None).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let (status, body) = rig.get("/rest/v1/users?select=id,orders(id)&orders.limit=1001").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body.to_string()
            .contains("`orders.limit` 1001 exceeds the maximum page size of 1000"),
        "{body}"
    );

    let (status, body) = rig.get("/rest/v1/users?select=id,orders(id)&orders.limit=1000").await;
    assert_eq!(status, StatusCode::OK, "at the ceiling is served: {body}");
}

/// The accepted half. A ceiling the whole request fits under serves every level in full —
/// so the charge bounds the request rather than truncating it, which is #1230's shape
/// under a `200`.
#[tokio::test]
async fn an_embed_inside_the_cost_ceiling_is_served_in_full() {
    let Some(rig) = rig_cost(5_205).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/users?select=id,orders(id,user(name))&limit=2").await;
    assert_eq!(status, StatusCode::OK, "5 205 against a ceiling of 5 205: {body}");

    let rows = body.get("data").and_then(Value::as_array).unwrap();
    assert_eq!(rows.len(), 2, "both users: {body}");
    let alice = rows
        .iter()
        .find(|r| r.get("id").and_then(Value::as_i64) == Some(1))
        .unwrap_or_else(|| panic!("alice: {body}"));
    let orders = alice.get("orders").and_then(Value::as_array).unwrap();
    assert_eq!(orders.len(), 2, "every parent's orders are all there: {body}");
    assert_eq!(
        orders[0].get("user").and_then(|u| u.get("name")).and_then(Value::as_str),
        Some("alice"),
        "the second level really executed: {body}"
    );
}

/// The document this suite serves loads, checked with no database.
///
/// Every other test here reaches the document only after `try_database_url()`, so in a
/// run without a database they skip before it is compiled and a load-time refusal of
/// it reports as a pass (`2b843cd27`: 12 tests red for a session under a green preflight).
/// This one needs nothing but the compiler, so that refusal cannot hide.
///
/// Both shapes the rigs write: every optional section absent, and every one declared.
#[tokio::test]
async fn the_document_loads_without_a_database() {
    compile_document(None, None).await;
    compile_document(Some(10_000), Some(1000)).await;
}
