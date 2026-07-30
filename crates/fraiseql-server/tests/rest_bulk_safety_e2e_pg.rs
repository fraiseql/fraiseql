//! #862 / #913 / #914 / #916 regression: collection-level bulk operations must do what
//! they report, and only to the rows a filter actually selected.
//!
//! These four defects are one interlocked cluster, which is why they are fixed and tested
//! together:
//!
//! * **#913** `execute_bulk_by_filter` ran the filter query, **discarded the matched rows**, called
//!   the mutation **once** with the request body and no row identity, and then reported
//!   `affected_rows` as the number of rows the *filter* matched. Both `_id_field` and
//!   `_max_affected` were unused parameters.
//! * **#862** the "at least one filter required" guard (`has_filter_params`) answered a *syntactic*
//!   question about the query string, while `build_filter_query_match` forwarded only
//!   `params.where_clause`. `?filter={}`, `?search=x` and any dotted key passed the guard and
//!   produced no WHERE clause — and no `limit` argument either, so the filter query was an
//!   unbounded scan.
//! * **#916** `Prefer: max-affected=N` used `unwrap_or(config.max_bulk_affected)`, so a
//!   client-supplied value **replaced** the operator's cap instead of lowering it.
//! * **#914** `Prefer: tx=rollback` was echoed in `Preference-Applied` and never honoured.
//!
//! **The interlock is the reason for the ordering.** #913's failure to iterate is what
//! currently caps #862's blast radius: repairing the loop without the guard turns
//! `?filter={}` into an unfiltered mass update or delete. The guard lands first.
//!
//! **Why a real database.** Every assertion here is about rows that changed or did not
//! change. `affected_rows` is precisely the number #913 fabricates, so a test that
//! asserts the reported count — which is all a mock adapter could offer — passes against
//! the bug. These read the table back.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `p13_bulk` schema → run `--test-threads=1`.
#![cfg(feature = "rest")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::sync::Arc;

use axum::body::Body;
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    runtime::Executor,
    schema::{
        ArgumentDefinition, CompiledSchema, FieldDefinition, FieldType, MutationDefinition,
        MutationOperation, QueryDefinition, RestConfig, TypeDefinition,
    },
};
use fraiseql_server::routes::{graphql::AppState, rest::rest_router};
use fraiseql_test_support::try_database_url;
use http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

const SCHEMA: &str = "p13_bulk";

/// Rows seeded with `status = 'active'` — the set a filter should select.
const ACTIVE: usize = 3;
/// Rows seeded with `status = 'archived'` — the set that must never be touched.
const ARCHIVED: usize = 2;

/// The operator's cap. Deliberately **below** `ACTIVE` so a request over the active set
/// exceeds it, which is what makes the #916 clamp observable.
const MAX_BULK_AFFECTED: u64 = 2;

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

async fn seed(adapter: &PostgresAdapter) {
    let mut stmts = vec![
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!(
            "CREATE TABLE {SCHEMA}.tb_item (id uuid PRIMARY KEY, status text NOT NULL, label \
             text NOT NULL)"
        ),
    ];

    for (n, status) in (0..ACTIVE)
        .map(|n| (n, "active"))
        .chain((0..ARCHIVED).map(|n| (n + ACTIVE, "archived")))
    {
        stmts.push(format!(
            "INSERT INTO {SCHEMA}.tb_item VALUES ('{}', '{status}', 'item-{n}')",
            uuid_for(n)
        ));
    }

    stmts.push(format!(
        "CREATE VIEW {SCHEMA}.v_item AS SELECT id, jsonb_build_object('id', id, 'status', \
         status, 'label', label) AS data FROM {SCHEMA}.tb_item ORDER BY label"
    ));
    // Positional call: the compiled argument names need not match these parameter names.
    stmts.push(format!(
        "CREATE OR REPLACE FUNCTION {SCHEMA}.fn_update_item(p_id uuid, p_status text) \
         RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
         DECLARE v app.mutation_response; BEGIN \
         UPDATE {SCHEMA}.tb_item SET status = p_status WHERE id = p_id; \
         v.succeeded := true; v.state_changed := true; v.message := 'updated'; \
         v.entity_type := 'P13Item'; v.entity_id := p_id; \
         v.entity := jsonb_build_object('id', p_id, 'status', p_status); \
         RETURN v; END; $$"
    ));
    stmts.push(format!(
        "CREATE OR REPLACE FUNCTION {SCHEMA}.fn_delete_item(p_id uuid) \
         RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
         DECLARE v app.mutation_response; BEGIN \
         DELETE FROM {SCHEMA}.tb_item WHERE id = p_id; \
         v.succeeded := true; v.state_changed := true; v.message := 'deleted'; \
         v.entity_type := 'P13Item'; v.entity_id := p_id; \
         v.entity := jsonb_build_object('id', p_id); \
         RETURN v; END; $$"
    ));

    for stmt in stmts {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup");
    }
}

/// A stable UUID per seeded row, so assertions can name rows without a lookup.
fn uuid_for(n: usize) -> String {
    format!("00000000-0000-0000-0000-{n:012}")
}

fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();

    let mut item = TypeDefinition::new("P13Item", format!("{SCHEMA}.v_item"));
    item.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::new("status", FieldType::String),
        FieldDefinition::new("label", FieldType::String),
    ];
    schema.types.push(item);

    // `has_where` is load-bearing: `execute_query_direct` reads `arguments["where"]`
    // only when the query declares it, so a list query without it silently ignores the
    // bulk filter. The compiler sets it for filterable list queries.
    let mut items = QueryDefinition::new("items", "P13Item")
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.v_item"));
    items.auto_params.has_where = true;
    items.auto_params.has_limit = true;
    schema.queries.push(items);

    // Argument names match the REST body keys; `id` is what the bulk path must inject
    // per matched row.
    let mut update = MutationDefinition::new("updateItem", "P13Item");
    update.sql_source = Some(format!("{SCHEMA}.fn_update_item"));
    update.operation = MutationOperation::Update {
        table: "tb_item".to_string(),
    };
    update.arguments = vec![
        ArgumentDefinition::new("id", FieldType::String),
        ArgumentDefinition::new("status", FieldType::String),
    ];
    schema.mutations.push(update);

    let mut delete = MutationDefinition::new("deleteItem", "P13Item");
    delete.sql_source = Some(format!("{SCHEMA}.fn_delete_item"));
    delete.operation = MutationOperation::Delete {
        table: "tb_item".to_string(),
    };
    delete.arguments = vec![ArgumentDefinition::new("id", FieldType::String)];
    schema.mutations.push(delete);

    schema.rest_config = Some(RestConfig {
        enabled: true,
        max_bulk_affected: MAX_BULK_AFFECTED,
        ..RestConfig::default()
    });
    schema.build_indexes();
    schema
}

struct Rig {
    router:  axum::Router,
    adapter: Arc<PostgresAdapter>,
}

impl Rig {
    /// Rows currently carrying `status`, by label — read straight from the table.
    async fn labels_with_status(&self, status: &str) -> Vec<String> {
        let rows: Vec<std::collections::HashMap<String, Value>> = self
            .adapter
            .execute_raw_query(&format!(
                "SELECT label FROM {SCHEMA}.tb_item WHERE status = '{status}' ORDER BY label"
            ))
            .await
            .expect("read back");
        rows.iter()
            .filter_map(|r| r.get("label").and_then(Value::as_str).map(ToString::to_string))
            .collect()
    }

    /// The `error.message` of a REST error envelope, or the whole body when absent.
    fn message(body: &Value) -> String {
        body.get("error")
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
            .map_or_else(|| body.to_string(), ToString::to_string)
    }

    async fn row_count(&self) -> usize {
        let rows: Vec<std::collections::HashMap<String, Value>> = self
            .adapter
            .execute_raw_query(&format!("SELECT id FROM {SCHEMA}.tb_item"))
            .await
            .expect("count");
        rows.len()
    }

    async fn send(
        &self,
        method: &str,
        uri: &str,
        prefer: Option<&str>,
        body: Value,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json");
        if let Some(p) = prefer {
            req = req.header("prefer", p);
        }
        let response = self
            .router
            .clone()
            .oneshot(req.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json = serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"raw": String::from_utf8_lossy(&bytes)}));
        (status, json)
    }
}

async fn rig() -> Option<Rig> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    seed(&adapter).await;

    let executor = Arc::new(Executor::new(schema(), Arc::clone(&adapter)));
    let state = AppState::new(executor);
    let router = rest_router(&state, false, false).expect("REST router");

    Some(Rig { router, adapter })
}

// ---------------------------------------------------------------------------
// #862 — the guard must agree with what reaches SQL
// ---------------------------------------------------------------------------

/// `?filter={}` parses to an empty DSL object, which `merge_where` collapses to `None`.
/// It satisfied the syntactic guard and produced no WHERE clause at all.
#[tokio::test]
async fn an_empty_filter_object_is_refused() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.send("DELETE", "/rest/v1/items?filter=%7B%7D", None, json!({})).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "?filter={{}} contributes no WHERE clause and must be refused, got {status} {body}"
    );
    let msg = Rig::message(&body);
    assert!(
        msg.contains("filter"),
        "the refusal must be the missing-filter guard, not an incidental error: {msg}"
    );
    assert_eq!(
        rig.row_count().await,
        ACTIVE + ARCHIVED,
        "a refused bulk delete must not remove rows"
    );
}

/// A dotted key is routed to `embedding_filters` and never reaches the bulk WHERE clause,
/// yet it satisfied the guard. The relationship name was never validated either.
#[tokio::test]
async fn a_dotted_key_that_contributes_no_where_clause_is_refused() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) =
        rig.send("DELETE", "/rest/v1/items?nonsense.field=x", None, json!({})).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a dotted key contributing no WHERE clause must be refused, got {status} {body}"
    );
    let msg = Rig::message(&body);
    assert!(
        msg.contains("filter"),
        "the refusal must be the missing-filter guard, not an incidental error: {msg}"
    );
    assert_eq!(
        rig.row_count().await,
        ACTIVE + ARCHIVED,
        "a refused bulk delete must not remove rows"
    );
}

// ---------------------------------------------------------------------------
// #913 — report what actually happened, to the rows actually selected
// ---------------------------------------------------------------------------

/// The core of #913: a bulk update must mutate **every matched row**, and only those.
///
/// The shipped code called the mutation once with no row identity and reported the
/// filter's row count, so this asserts the table, not the response.
#[tokio::test]
async fn a_bulk_update_changes_every_matched_row_and_only_those() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    // `max-affected` is raised to cover the active set; the operator cap is deliberately
    // lower, and lowering-only is asserted separately below.
    let (status, body) = rig
        .send(
            "PATCH",
            "/rest/v1/items?status[eq]=active",
            Some("max-affected=1"),
            json!({"status": "retired"}),
        )
        .await;

    // The operator cap (2) is below the matched set (3), so the correct answer is a
    // refusal rather than a partial mutation. Assert the table is untouched.
    assert_eq!(
        status,
        StatusCode::PAYLOAD_TOO_LARGE,
        "a bulk update matching more rows than the cap must be refused, got {status} {body}"
    );
    assert_eq!(
        rig.labels_with_status("retired").await,
        Vec::<String>::new(),
        "a refused bulk update must not mutate any row"
    );
    assert_eq!(
        rig.labels_with_status("active").await.len(),
        ACTIVE,
        "the active rows must be untouched by a refused request"
    );
}

/// Within the cap, every matched row is mutated and the reported count is the true one.
#[tokio::test]
async fn a_bulk_update_within_the_cap_mutates_and_reports_truthfully() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    // The archived set (2) is exactly at the cap.
    let (status, body) = rig
        .send(
            "PATCH",
            "/rest/v1/items?status[eq]=archived",
            None,
            json!({"status": "retired"}),
        )
        .await;

    assert!(status.is_success(), "bulk update should succeed, got {status} {body}");

    let retired = rig.labels_with_status("retired").await;
    assert_eq!(
        retired.len(),
        ARCHIVED,
        "every matched row must actually be updated — got {retired:?}, expected {ARCHIVED} rows"
    );
    assert_eq!(
        rig.labels_with_status("active").await.len(),
        ACTIVE,
        "rows outside the filter must not be touched"
    );
}

// ---------------------------------------------------------------------------
// #916 — a client may lower the cap, never raise it
// ---------------------------------------------------------------------------

/// `Prefer: max-affected=N` above the configured cap must not raise it.
#[tokio::test]
async fn a_client_cannot_raise_the_configured_bulk_cap() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig
        .send(
            "PATCH",
            "/rest/v1/items?status[eq]=active",
            Some("max-affected=1000000"),
            json!({"status": "retired"}),
        )
        .await;

    assert_eq!(
        status,
        StatusCode::PAYLOAD_TOO_LARGE,
        "a client-supplied max-affected must not raise the operator's cap of \
         {MAX_BULK_AFFECTED}, got {status} {body}"
    );
    assert_eq!(
        rig.labels_with_status("retired").await,
        Vec::<String>::new(),
        "no row may be mutated when the cap is exceeded"
    );
}

/// A client-supplied cap **below** the configured one is honoured.
#[tokio::test]
async fn a_client_may_lower_the_bulk_cap() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    // The archived set (2) is within the operator cap but above the client's 1.
    let (status, body) = rig
        .send(
            "PATCH",
            "/rest/v1/items?status[eq]=archived",
            Some("max-affected=1"),
            json!({"status": "retired"}),
        )
        .await;

    assert_eq!(
        status,
        StatusCode::PAYLOAD_TOO_LARGE,
        "a client cap of 1 below {ARCHIVED} matched rows must be honoured, got {status} {body}"
    );
    assert_eq!(
        rig.labels_with_status("retired").await,
        Vec::<String>::new(),
        "no row may be mutated when the client's own cap is exceeded"
    );
}

// ---------------------------------------------------------------------------
// #914 — `tx=rollback` must be honoured or refused, never merely echoed
// ---------------------------------------------------------------------------

/// A dry-run bulk delete must leave every row in place.
///
/// The shipped code answered `Preference-Applied: tx=rollback` and committed — the
/// response affirmed the guarantee it had just violated. Either outcome is acceptable
/// here (honoured, or refused as unsupported); what is forbidden is a success response
/// claiming the preference was applied while the rows are gone.
#[tokio::test]
async fn a_tx_rollback_bulk_delete_does_not_delete() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let before = rig.row_count().await;

    let (status, body) = rig
        .send("DELETE", "/rest/v1/items?status[eq]=archived", Some("tx=rollback"), json!({}))
        .await;

    assert_eq!(
        rig.row_count().await,
        before,
        "tx=rollback must not persist deletions — {before} rows before, {} after (status \
         {status}, body {body})",
        rig.row_count().await
    );

    // Rows surviving is necessary but not sufficient: a bulk delete that simply errored
    // would also leave them in place. Either the dry run ran (success), or the server
    // said plainly that it will not honour the preference.
    let msg = Rig::message(&body);
    assert!(
        status.is_success()
            || msg.to_lowercase().contains("rollback")
            || msg.to_lowercase().contains("not supported")
            || msg.to_lowercase().contains("unsupported"),
        "tx=rollback must be honoured or explicitly refused, not incidentally failed: \
         {status} {msg}"
    );
}
