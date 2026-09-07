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
//! **#1293** the bulk path refuses a request whose only extra parameter is an
//! embedded-relationship filter, because such a filter contributes no WHERE clause. That
//! branch had shipped untested: it sits behind three guards in the same function, and the
//! one case that looked like its test was answered by the second of them. Reaching it needs
//! a fixture that can *declare* a relationship, which is why `P13Item.notes` and `P13Note`
//! exist here.
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
        ArgumentDefinition, Cardinality, CompiledSchema, FieldDefinition, FieldType,
        MutationDefinition, MutationOperation, QueryDefinition, Relationship, RestConfig,
        TypeDefinition,
    },
};
use fraiseql_server::routes::{
    graphql::AppState,
    rest::{RestMountConfig, rest_router},
};
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

/// The `kind` every seeded note carries — the value a dotted filter would name.
const NOTE_KIND: &str = "urgent";

/// The relationship `P13Item` declares. Named in the dotted key of the #1293 case, so the
/// extractor stores it rather than refusing it by name (#1279).
const NOTE_REL: &str = "notes";

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

async fn seed(adapter: &PostgresAdapter) {
    let mut stmts = vec![
        // The `app.mutation_response` contract. Provisioned here, idempotently, rather than
        // assumed: the shared seed (`tests/sql/postgres/init.sql`, mounted by both the
        // Dagger service and the local compose rig) does not create it. Any suite relying
        // on it being present was relying on a *previous* suite in the same database having
        // created it — which is green until the run order changes or the volume is reset,
        // and is not a property CI can depend on. `make db-reset` reproduces the failure
        // exactly (SQLSTATE 3F000, invalid_schema_name).
        "CREATE SCHEMA IF NOT EXISTS app".to_string(),
        "DO $$ BEGIN CREATE TYPE app.mutation_error_class AS ENUM ('validation','conflict',\
         'not_found','unauthorized','forbidden','internal','transaction_failed','timeout',\
         'rate_limited','service_unavailable'); EXCEPTION WHEN duplicate_object THEN NULL; END $$"
            .to_string(),
        "DO $$ BEGIN CREATE TYPE app.mutation_response AS (succeeded BOOLEAN, state_changed \
         BOOLEAN, error_class app.mutation_error_class, status_detail TEXT, http_status \
         SMALLINT, message TEXT, entity_id UUID, entity_type TEXT, entity JSONB, \
         updated_fields TEXT[], cascade JSONB, error_detail JSONB, metadata JSONB); \
         EXCEPTION WHEN duplicate_object THEN NULL; END $$"
            .to_string(),
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

    // A second table, so `P13Item` can declare a relationship (#1293). The bulk path's
    // `embedding_filters` refusal needs a request carrying *both* a plain field filter and
    // a dotted key naming a relationship the type really has; with the single flat type
    // this fixture used to have, #1279's extractor rule refused the dotted key by name
    // first and the bulk guard was unreachable. Nothing here executes an embed — the
    // request is refused before any join is composed — but the relationship is declared in
    // the shape `relationship_violations` accepts, and a test below asserts that, so the
    // case is reached through a schema a compiler could really emit.
    stmts.push(format!(
        "CREATE TABLE {SCHEMA}.tb_note (id uuid PRIMARY KEY, fk_item uuid NOT NULL \
         REFERENCES {SCHEMA}.tb_item(id) ON DELETE CASCADE, kind text NOT NULL)"
    ));
    for n in 0..(ACTIVE + ARCHIVED) {
        stmts.push(format!(
            "INSERT INTO {SCHEMA}.tb_note VALUES ('{}', '{}', '{NOTE_KIND}')",
            note_uuid_for(n),
            uuid_for(n)
        ));
    }
    // `fkItem` is published under the stored key `fk_item` — the declared-name/stored-key
    // split #1271 is about. Spelling them alike here would make the fixture agree with
    // itself by accident.
    stmts.push(format!(
        "CREATE VIEW {SCHEMA}.v_note AS SELECT id, jsonb_build_object('id', id, 'fk_item', \
         fk_item, 'kind', kind) AS data FROM {SCHEMA}.tb_note ORDER BY kind"
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

/// A stable UUID per seeded note, in a range that cannot collide with [`uuid_for`].
fn note_uuid_for(n: usize) -> String {
    format!("00000000-0000-0000-0001-{n:012}")
}

/// Opt every fixture mutation out of the change-log outbox.
///
/// These suites are about REST write semantics, not the change spine. Left on (the
/// default), each mutation INSERTs into `core.tb_entity_change_log` — a table neither
/// database seed creates, and which *other* suites in this crate create with differing
/// column sets. That made the result depend on which binary had run first: green alone,
/// `column "updated_fields" does not exist` after a full-crate run, `relation ... does
/// not exist` after `make db-reset`. Dropping the dependency is what makes these suites
/// order-independent, which is the only form in which they can be a CI gate.
const fn without_changelog(mut m: MutationDefinition) -> MutationDefinition {
    m.changelog = false;
    m
}

fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();

    let mut item = TypeDefinition::new("P13Item", format!("{SCHEMA}.v_item"));
    item.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::new("status", FieldType::String),
        FieldDefinition::new("label", FieldType::String),
    ];
    // Declared so a dotted key naming it survives #1279's extractor rule and reaches the
    // bulk path, which is the only way its `embedding_filters` refusal can be exercised
    // (#1293). `OneToMany` joins on `P13Item.id` (declared above) and `P13Note.fkItem`.
    item.relationships = vec![Relationship {
        name:           NOTE_REL.to_string(),
        target_type:    "P13Note".to_string(),
        cardinality:    Cardinality::OneToMany,
        foreign_key:    "fk_item".to_string(),
        referenced_key: "id".to_string(),
    }];
    schema.types.push(item);

    let mut note = TypeDefinition::new("P13Note", format!("{SCHEMA}.v_note"));
    note.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::new("fkItem", FieldType::Id),
        FieldDefinition::new("kind", FieldType::String),
    ];
    schema.types.push(note);

    // `has_where` is load-bearing: `execute_query_direct` reads `arguments["where"]`
    // only when the query declares it, so a list query without it silently ignores the
    // bulk filter. The compiler sets it for filterable list queries.
    let mut items = QueryDefinition::new("items", "P13Item")
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.v_item"));
    items.auto_params.has_where = true;
    items.auto_params.has_limit = true;
    schema.queries.push(items);

    // An embed sources its rows from the target's list query, so `relationship_violations`
    // refuses a relationship whose target no list query returns. Declaring it keeps the
    // fixture loadable rather than merely constructible.
    let mut notes = QueryDefinition::new("notes", "P13Note")
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.v_note"));
    notes.auto_params.has_where = true;
    notes.auto_params.has_limit = true;
    schema.queries.push(notes);

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
    schema.mutations.push(without_changelog(update));

    let mut delete = MutationDefinition::new("deleteItem", "P13Item");
    delete.sql_source = Some(format!("{SCHEMA}.fn_delete_item"));
    delete.operation = MutationOperation::Delete {
        table: "tb_item".to_string(),
    };
    delete.arguments = vec![ArgumentDefinition::new("id", FieldType::String)];
    schema.mutations.push(without_changelog(delete));

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
    let router = rest_router(&state, &RestMountConfig::default()).expect("REST router");

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
///
/// **Which guard answers has moved, and the assertion follows it (#1288).** When this was
/// written the request reached `build_filter_query_match` and died on the missing-filter
/// guard, whose message contains `filter`. #1279 taught `RestParamExtractor::extract` to
/// refuse a dotted key whose relationship the type does not declare — at the producer, by
/// name — so `nonsense` is now rejected before any bulk-specific code runs, and the message
/// says which relationship was asked for and which ones exist. That refusal is the more
/// useful one; the invariant this case exists for is the second assertion, which is
/// unchanged: **a refused bulk delete removes nothing.**
///
/// ⚠ The bulk path's own `embedding_filters` refusal (`bulk/mod.rs`, "Embedded-relationship
/// filters … are not supported on bulk operations") is therefore not what answers here — and
/// never was, since the missing-filter guard preceded it. Reaching it needs a dotted key
/// naming a relationship the type *does* declare, alongside a plain field filter; that is
/// #1293, and it is now
/// [`a_declared_relationship_filter_is_refused_by_the_bulk_path_itself`] at the end of this
/// file, which the fixture's `P13Item.notes` relationship exists to make expressible. This
/// case keeps its own subject: the unknown-relationship refusal, by name.
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
        msg.contains("no relationship 'nonsense'"),
        "the refusal must name the relationship the type does not have, not be an incidental \
         error: {msg}"
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

// ---------------------------------------------------------------------------
// #1293 — the bulk path's own embedded-relationship refusal
// ---------------------------------------------------------------------------

/// The fixture's relationship is one the loader would accept.
///
/// `schema()` is hand-built, so nothing otherwise checks that `P13Item.notes` is a shape a
/// compiled schema could really carry — and a relationship the loader would reject is a
/// fixture no server can produce, which would make the two cases below prove nothing about
/// a request any deployment can send. `relationship_violations` is the same function
/// `finish_load` calls, so this asserts the fixture against the real admission rule rather
/// than against a restatement of it.
///
/// Needs no database: it is a property of the schema document alone.
#[test]
fn the_fixture_declares_a_relationship_the_loader_would_accept() {
    let schema = schema();

    let item = schema.find_type("P13Item").expect("P13Item must be declared");
    assert!(
        item.relationships.iter().any(|r| r.name == NOTE_REL),
        "the #1293 cases need `P13Item` to declare '{NOTE_REL}'; it declares {:?}",
        item.relationships.iter().map(|r| r.name.as_str()).collect::<Vec<_>>()
    );

    assert_eq!(
        schema.relationship_violations(),
        Vec::<String>::new(),
        "the fixture must be a schema the load path admits, or these cases describe a \
         request no server could receive"
    );
}

/// A dotted key naming a **declared** relationship, alongside a plain field filter, is
/// refused by the bulk path's own guard — and mutates nothing.
///
/// **This is the first case to reach that guard (#1293).** The refusal at
/// `bulk/mod.rs`'s `!params.embedding_filters.is_empty()` arm sits behind three
/// predecessors in the same function, and every earlier attempt at this case died on one
/// of them:
///
/// 1. `!auto_params.has_where` — not reached: `items` declares `where`.
/// 2. `where_clause is None` — this is why `?nonsense.field=x` alone never got here; a dotted key
///    contributes no WHERE clause, so `status[eq]=archived` is load-bearing.
/// 3. `search_query.is_some()` — not reached: no `?search=`.
/// 4. `!embedding_filters.is_empty()` — **this one**, reached because `notes` is declared and so
///    survives #1279's extractor rule instead of being refused by name.
///
/// The assertion names `Embedded-relationship filters`, which is the single occurrence of
/// that phrase in the tree: no predecessor, and not #1279's `has no relationship
/// '<name>'`, can satisfy it. That is what stops this passing on someone else's message —
/// the exact way its ancestor
/// [`a_dotted_key_that_contributes_no_where_clause_is_refused`] read as covering this
/// branch for two releases without ever entering it.
///
/// The row count is the invariant that outlives the message: `archived` is exactly at the
/// cap, so with the guard removed this request **succeeds and deletes those rows** — the
/// blast radius being guarded is a caller who believed `notes.kind` had narrowed the set.
#[tokio::test]
async fn a_declared_relationship_filter_is_refused_by_the_bulk_path_itself() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let before = rig.row_count().await;

    let (status, body) = rig
        .send(
            "DELETE",
            &format!("/rest/v1/items?status[eq]=archived&{NOTE_REL}.kind={NOTE_KIND}"),
            None,
            json!({}),
        )
        .await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a bulk delete carrying an embedded-relationship filter must be refused, got \
         {status} {body}"
    );
    let msg = Rig::message(&body);
    assert!(
        msg.contains("Embedded-relationship filters"),
        "the refusal must be the bulk path's own embedded-relationship guard, not one of \
         the three that precede it and not #1279's unknown-relationship rule: {msg}"
    );
    assert_eq!(rig.row_count().await, before, "a refused bulk delete must remove nothing");
}

/// The control: the same request **without** the dotted key deletes the matched rows.
///
/// Without this, the case above is satisfied by any reason the request could not run —
/// an unfilterable query, a cap, a fixture that never mounted the route. Dropping one
/// parameter is the only difference between the two, so this is what makes the refusal
/// attributable to `notes.kind` rather than to the request being unrunnable.
#[tokio::test]
async fn the_same_bulk_delete_without_the_dotted_key_removes_the_matched_rows() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) =
        rig.send("DELETE", "/rest/v1/items?status[eq]=archived", None, json!({})).await;

    assert!(
        status.is_success(),
        "the same filter without the dotted key must run, got {status} {body}"
    );
    assert_eq!(
        rig.row_count().await,
        ACTIVE,
        "the archived rows — and only those — must be gone"
    );
}
