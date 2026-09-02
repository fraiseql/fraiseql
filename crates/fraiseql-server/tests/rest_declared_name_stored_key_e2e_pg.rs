//! #1271: a declared field name and the stored JSONB key it reads are two
//! different strings, and every consumer must derive one from the other the
//! same way.
//!
//! `ProjectionMapper` read the stored blob by the **declared** name while the
//! SQL projection generator and the `where` parser both `to_snake_case` it. A
//! multi-word camelCase field — the default naming convention, and what
//! `examples/basic` ships — was therefore absent from the response on any plan
//! that gets no SQL projection hint, under a 200, with the validator still
//! naming the field as available.
//!
//! **Why this fixture spells the two halves differently.** Every pre-existing
//! REST fixture in the tree declares `fk_author` / `ship_city` — names whose
//! `snake_case` form is themselves — so the declared name and the stored key
//! are the same string and no test could see which rule ran. This one declares
//! `fkUser` and `createdAt` over a view storing `fk_user` and `created_at`, so
//! a projector reading the wrong one returns a visibly different document.
//!
//! **Why a real database.** The failure is a missing key on a 200, which is
//! indistinguishable from "the row genuinely has no such value" unless the
//! response content is asserted against known seeded rows. The two plans also
//! have to be compared against *each other*: the defect survived because the
//! SQL-projected plan is correct and the raw-blob plan is not, so either one
//! alone looks fine.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in
//! the database-free `test` leg and runs in the Dagger `integration: server`
//! suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `p1271_keys` schema → run
//! `--test-threads=1`.
#![cfg(feature = "rest")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::sync::Arc;

use axum::body::Body;
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    runtime::Executor,
    schema::{
        Cardinality, CompiledSchema, FieldDefinition, FieldType, QueryDefinition, Relationship,
        RestConfig, TypeDefinition,
    },
};
use fraiseql_server::routes::{
    graphql::AppState,
    rest::{RestMountConfig, rest_query_router},
};
use fraiseql_test_support::try_database_url;
use http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

const SCHEMA: &str = "p1271_keys";

// ---------------------------------------------------------------------------
// Fixture: two orders, whose stored JSONB keys are the `snake_case` spellings
// of the camelCase names the schema declares.
// ---------------------------------------------------------------------------

async fn seed(adapter: &PostgresAdapter) {
    let stmts = vec![
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!("CREATE TABLE {SCHEMA}.tb_user (id bigint PRIMARY KEY, name text NOT NULL)"),
        format!(
            "CREATE TABLE {SCHEMA}.tb_order (id bigint PRIMARY KEY, fk_user bigint NOT NULL, \
             created_at text NOT NULL, total bigint NOT NULL)"
        ),
        format!("INSERT INTO {SCHEMA}.tb_user VALUES (1, 'alice'), (2, 'bob')"),
        format!(
            "INSERT INTO {SCHEMA}.tb_order VALUES (10, 1, '2026-09-01', 100), \
             (11, 2, '2026-09-02', 200)"
        ),
        // The storage contract: the view builds `data` with the `snake_case`
        // keys the SQL projection generator and the `where` parser derive.
        format!(
            "CREATE VIEW {SCHEMA}.v_user AS SELECT id, jsonb_build_object('id', id, 'name', name) \
             AS data FROM {SCHEMA}.tb_user ORDER BY id"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_order AS SELECT id, jsonb_build_object('id', id, 'fk_user', \
             fk_user, 'created_at', created_at, 'total', total) AS data FROM {SCHEMA}.tb_order \
             ORDER BY id"
        ),
    ];

    for stmt in stmts {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup");
    }
}

fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();

    let mut order = TypeDefinition::new("Order", format!("{SCHEMA}.v_order"));
    // `fkUser` and `createdAt` are the discriminating names: `to_snake_case`
    // of each differs from the name itself. `id` and `total` are the control —
    // they spell the same on both sides, which is all any previous fixture had.
    order.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("fkUser", FieldType::Int),
        FieldDefinition::new("createdAt", FieldType::String),
        FieldDefinition::new("total", FieldType::Int),
    ];
    // The embed's target. `Order.user` joins on the column `fk_user`, which is
    // published as the field `fkUser` — the split this whole file is about,
    // reached through a second mechanism.
    order.relationships = vec![Relationship {
        name:           "user".to_string(),
        target_type:    "User".to_string(),
        cardinality:    Cardinality::ManyToOne,
        foreign_key:    "fk_user".to_string(),
        referenced_key: "id".to_string(),
    }];
    schema.types.push(order);

    let mut user = TypeDefinition::new("User", format!("{SCHEMA}.v_user"));
    user.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("name", FieldType::String),
    ];
    schema.types.push(user);

    let mut q = QueryDefinition::new("orders", "Order")
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.v_order"));
    q.auto_params.has_where = true;
    q.auto_params.has_limit = true;
    schema.queries.push(q);

    // An embed sources its rows from a *list* query on the target type.
    let mut users = QueryDefinition::new("users", "User")
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.v_user"));
    users.auto_params.has_where = true;
    users.auto_params.has_limit = true;
    schema.queries.push(users);

    schema.rest_config = Some(RestConfig {
        enabled: true,
        ..RestConfig::default()
    });
    schema.build_indexes();
    schema
}

struct Rig {
    router:   axum::Router,
    /// Held so the same schema and connection can be read through the GraphQL
    /// surface as well, which is the other half of the parity assertion.
    executor: Arc<Executor<PostgresAdapter>>,
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

async fn rig() -> Option<Rig> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    seed(&adapter).await;

    let executor = Arc::new(Executor::new(schema(), adapter));
    let state = AppState::new(Arc::clone(&executor));
    let router = rest_query_router(&state, &RestMountConfig::default()).expect("REST router");

    Some(Rig { router, executor })
}

/// The row with `id == 10`, or a panic naming the document that lacked it.
fn row_10(body: &Value) -> Value {
    body.get("data")
        .and_then(Value::as_array)
        .and_then(|rows| rows.iter().find(|r| r.get("id").and_then(Value::as_i64) == Some(10)))
        .cloned()
        .unwrap_or_else(|| panic!("no row with id 10 in {body}"))
}

// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_full_read_carries_every_declared_field() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/orders").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    assert_eq!(
        row_10(&body),
        json!({ "id": 10, "fkUser": 1, "createdAt": "2026-09-01", "total": 100 }),
        "every declared field must be served, under the name the schema declares"
    );
}

#[tokio::test]
async fn a_selected_camel_case_field_is_served() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/orders?select=id,fkUser,createdAt").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    assert_eq!(
        row_10(&body),
        json!({ "id": 10, "fkUser": 1, "createdAt": "2026-09-01" }),
        "a selected camelCase field must be present, not silently dropped"
    );
}

#[tokio::test]
async fn graphql_and_rest_serve_the_same_row() {
    // The property the defect broke, and the reason it survived: the two
    // surfaces project the *same stored row* through different code. The
    // GraphQL runner builds an SQL projection hint, so `jsonb_build_object`
    // resolved the snake_case key and emitted the camelCase response key — and
    // GraphQL was right. The REST runner "reads the whole `data` document and
    // projects in Rust" (`ResolvedDirectRead::projection_request`), so it took
    // the projector that read the declared name verbatim — and REST was wrong.
    //
    // Asserting REST against GraphQL rather than against a literal is what
    // makes this a parity pin: it fails if either surface moves.
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let gql = rig
        .executor
        .execute("{ orders { id fkUser createdAt } }", None)
        .await
        .expect("graphql read");
    let gql_row = gql
        .get("data")
        .and_then(|d| d.get("orders"))
        .and_then(Value::as_array)
        .and_then(|rows| rows.iter().find(|r| r.get("id").and_then(Value::as_i64) == Some(10)))
        .cloned()
        .unwrap_or_else(|| panic!("no order 10 in the GraphQL response {gql}"));

    let (status, body) = rig.get("/rest/v1/orders?select=id,fkUser,createdAt").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rest_row = row_10(&body);

    assert_eq!(gql_row, rest_row, "GraphQL and REST must serve one stored row identically");
    // Pinned against the seeded values too, so the two agreeing on a *wrong*
    // answer cannot pass — agreement alone is satisfied by both being empty.
    assert_eq!(rest_row, json!({ "id": 10, "fkUser": 1, "createdAt": "2026-09-01" }));
}

#[tokio::test]
async fn the_declared_name_is_the_only_name_the_surface_accepts() {
    // The half that already worked, pinned so a fix cannot "resolve" the split
    // by teaching the validator to accept the stored spelling as well. The
    // published surface is the declared name; `fk_user` is storage.
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/orders?select=id,fk_user").await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "the stored spelling is not a public field name: {body}"
    );
}

#[tokio::test]
async fn a_filter_on_a_camel_case_field_selects_the_right_rows() {
    // The `where` parser derives the same stored key. Pinned alongside the
    // projector so the two cannot drift apart again in the other direction:
    // a filter that matched nothing would be the mirror of a field that
    // returned nothing.
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/orders?fkUser[eq]=2").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let rows = body.get("data").and_then(Value::as_array).cloned().unwrap_or_default();
    let ids: Vec<i64> = rows.iter().filter_map(|r| r.get("id").and_then(Value::as_i64)).collect();
    assert_eq!(ids, vec![11], "a filter on a camelCase field must reach its stored key: {body}");
}

#[tokio::test]
async fn an_embed_follows_a_join_column_published_under_a_camel_case_name() {
    // The #1266 end-to-end test spells its join column `fk_user` on both sides
    // and records why: "a `fkUser`/`fk_user` split does not survive a REST read
    // at all". That was true, and #1271 is the reason — not the embed machinery.
    //
    // `extract_join_key` reads the key off the **already-projected** parent row
    // under its declared name (`declared_key` → `field_for_column`). While the
    // projector dropped `fkUser` from that row, the lookup found nothing and the
    // embed answered `null` under a 200 — indistinguishable from an order that
    // genuinely has no user. With the key present, the same unchanged machinery
    // resolves it, so this pins the caveat as retired rather than merely fixed.
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/orders?select=id,user(name)").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    assert_eq!(
        row_10(&body),
        json!({ "id": 10, "user": { "name": "alice" } }),
        "the embed must resolve through a join column published as `fkUser`"
    );
}
