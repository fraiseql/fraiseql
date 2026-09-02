//! #1266: an embed works end to end from a schema `fraiseql compile` actually produced.
//!
//! Every other test of the REST embedding surface — including the four that fixed #863,
//! #864, #1170 and #1230 — hand-builds a `CompiledSchema` in Rust. That was not a stylistic
//! choice: `TypeDefinition.relationships` had **no authoring producer**, so a hand-built
//! struct was the only way to reach the code at all. Four silent wrong answers were found
//! and fixed inside a surface no user could get to.
//!
//! This test is the one that closes that loop. It authors `fraiseql.toml`, runs the real
//! `compile_to_schema`, and serves `?select=orders(...)` from the artifact that comes out.
//! A hand-built schema cannot fail the way this can: it skips the IR, the converter, the
//! TOML emitter and the compile-time check, which is precisely the stretch #1266 was about.
//!
//! The fixture spells the join column the same on both sides (`fk_user` declared and
//! stored), which keeps this test about the compile path rather than about casing. The
//! split case — a column `fk_user` published as `fkUser` — used not to survive a REST read
//! at all (#1271: `ProjectionMapper` read the stored JSONB by the declared name while the
//! SQL projection generator and the `where` parser both snake-case it, so the key the embed
//! joins on was missing from the projected row and the embed answered `null`). That is
//! fixed, and pinned end to end by
//! `an_embed_follows_a_join_column_published_under_a_camel_case_name` in
//! `rest_declared_name_stored_key_e2e_pg`.
//! The compiler's column→field resolution is pinned where it can be tested in isolation:
//! `a_join_column_resolves_against_a_camel_case_field_name` in `fraiseql-core` and
//! `a_join_column_resolves_against_the_camel_case_field_it_is_published_as` in the CLI.
//!
//! **Why a real database.** An embed that cannot follow its join key does not error — it
//! answers `[]` or `null` under a 200, indistinguishable from "there is genuinely nothing
//! related" (#1230). Only asserting the response *content* against known seeded rows can
//! tell the two apart.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `p1266_embed` schema → run `--test-threads=1`.
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

const SCHEMA: &str = "p1266_embed";

/// The authoring document, in the surface #1266 added.
///
/// `foreign_key` names the SQL column, and `Order` declares a field of the same name, so
/// the compiler's check resolves it and the executor reads it off the projected row. See
/// the module docs for why the two spellings are not deliberately split here.
fn fraiseql_toml() -> String {
    format!(
        r#"
[schema]
name = "embed-1266"
version = "1.0.0"
database_target = "postgresql"

[rest]
enabled = true

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

/// Two users, two orders each, with distinguishable totals so a mis-scoped embed shows up
/// as the wrong rows rather than as an empty one.
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
        // `fk_user` is both the declared field name and the stored JSONB key, which is
        // what makes this fixture readable by every consumer at once: the SQL projection
        // generator and the `where` parser snake-case the declared name, and
        // `ProjectionMapper` uses it verbatim.
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
    /// The loaded compiled schema, so a test can assert on the served document as well
    /// as on the served rows.
    schema:    CompiledSchema,
    // Held so the compiled artifact's temp directory outlives the test.
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
/// The compiled schema goes through `to_json`/`from_json` on the way, which is the load
/// path a served artifact takes — and the path carrying `finish_load`'s relationship
/// check, so this rig proves the compiler's output survives its own load-time refusal.
async fn rig() -> Option<Rig> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    seed(&adapter).await;

    let temp_dir = TempDir::new().expect("temp dir");
    let toml_path = temp_dir.path().join("fraiseql.toml");
    std::fs::write(&toml_path, fraiseql_toml()).expect("write fraiseql.toml");

    let (compiled, _) = compile_to_schema(CompileOptions {
        skip_hash: true,
        ..CompileOptions::new(toml_path.to_str().expect("utf-8 path"))
    })
    .await
    .expect("the authored relationships must compile");

    let mut schema = CompiledSchema::from_json(
        &compiled.to_json().expect("serialize the compiled artifact"),
        false,
    )
    .expect("the compiler's own output must survive the load-time relationship check");
    schema.build_indexes();

    let executor = Arc::new(Executor::new(schema.clone(), adapter));
    let state = AppState::new(executor);
    let router = rest_query_router(&state, &RestMountConfig::default()).expect("REST router");

    Some(Rig {
        router,
        schema,
        _temp_dir: temp_dir,
    })
}

/// The compiled artifact carries the relationships, rather than the empty vector every
/// converter used to write. Asserted through the rows the REST route serves rather than
/// on the struct, because the struct is what a hand-built fixture could fake.
#[tokio::test]
async fn a_compiled_schema_serves_an_embed_it_declared_in_toml() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/users?select=id,orders(id,total)").await;
    assert_eq!(status, StatusCode::OK, "read should succeed: {body}");

    assert_eq!(order_totals(&body, 1), vec![100, 101], "alice's orders: {body}");
    assert_eq!(order_totals(&body, 2), vec![200, 201], "bob's orders: {body}");
}

/// Before #1266 this request was a **400** against every compiled schema:
/// `validate_embedding_relationship_name` checked the name against an always-empty list
/// and answered `Available: none`. That is the exact symptom the issue named, so it is
/// asserted directly — a test of the success path alone would pass against a server that
/// had merely stopped validating.
#[tokio::test]
async fn an_embed_name_is_no_longer_refused_as_unavailable() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/users?select=id,orders(id)").await;
    assert_ne!(status, StatusCode::BAD_REQUEST, "the relationship must be known: {body}");

    // …and an undeclared one still is, so the validator is doing its job rather than
    // having been switched off.
    let (status, body) = rig.get("/rest/v1/users?select=id,invoices(id)").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "an unknown relationship stays a 400: {body}");
}

/// The `ManyToOne` direction, which reads `foreign_key` off the *declaring* type's row —
/// `fk_user` on an order. This is #1230's shape: the client never selects that column, so
/// the server projects it, follows the join, and strips it back out. A compiled schema
/// whose column→field resolution disagreed with the executor's would answer `null` here,
/// on a 200.
#[tokio::test]
async fn a_many_to_one_embed_resolves_the_join_column_the_client_did_not_select() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/orders?select=id,user(name)").await;
    assert_eq!(status, StatusCode::OK, "read should succeed: {body}");

    let rows = body
        .get("data")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("rows: {body}"));
    let order_10 = rows
        .iter()
        .find(|r| r.get("id").and_then(Value::as_i64) == Some(10))
        .unwrap_or_else(|| panic!("order 10 present: {body}"));

    assert_eq!(
        order_10.get("user").and_then(|u| u.get("name")).and_then(Value::as_str),
        Some("alice"),
        "the ManyToOne embed resolved: {body}"
    );
    assert!(
        order_10.get("fk_user").is_none(),
        "the server's own projection key is stripped back out (#1230): {body}"
    );
}

/// A count is the other consumer of `required_join_keys`, and it takes the same column.
#[tokio::test]
async fn a_count_embed_counts_the_right_parent_s_rows() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/users?select=id,orders.count").await;
    assert_eq!(status, StatusCode::OK, "read should succeed: {body}");

    let rows = body
        .get("data")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("rows: {body}"));
    for row in rows {
        assert_eq!(
            row.get("orders_count").and_then(Value::as_u64),
            Some(2),
            "each user has exactly two orders: {body}"
        );
    }
}

/// The `orders` array of the user row with `id == user_id`, as totals.
fn order_totals(body: &Value, user_id: i64) -> Vec<i64> {
    body.get("data")
        .and_then(Value::as_array)
        .and_then(|rows| rows.iter().find(|r| r.get("id").and_then(Value::as_i64) == Some(user_id)))
        .and_then(|row| row.get("orders"))
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("user {user_id} has an orders array: {body}"))
        .iter()
        .map(|order| order.get("total").and_then(Value::as_i64).expect("total"))
        .collect()
}

/// The third surface #1266 named as unreachable: the served `OpenAPI` document advertises
/// `?select=<rel>(fields)` per relationship, and emitted none for any compiled schema
/// because `type_def.relationships` was always empty.
///
/// Asserted against the document the generator produces from *this* compiled schema,
/// through the same `read_surface` mount `rest_query_router` builds — so what is checked
/// is what a client fetching `/openapi.json` from this rig would read.
#[tokio::test]
async fn the_served_openapi_document_advertises_the_embed() {
    use fraiseql_server::routes::rest::{
        openapi::generate_openapi,
        resource::{MountedRoutes, RestRouteTable},
    };

    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let route_table = RestRouteTable::from_compiled_schema(&rig.schema).expect("route table");
    let mounted = MountedRoutes::read_surface(&route_table);
    let doc = generate_openapi(&rig.schema, &route_table, false, &mounted).expect("openapi");

    let user = &doc["components"]["schemas"]["User"]["properties"]["orders"];
    assert_eq!(user["type"], "array", "a OneToMany embed is advertised as an array: {doc}");
    assert_eq!(
        user["items"]["$ref"], "#/components/schemas/Order",
        "…of the target type: {doc}"
    );
    assert!(
        user["description"]
            .as_str()
            .is_some_and(|d| d.contains("?select=orders(fields)")),
        "…and names the syntax that reaches it: {doc}"
    );

    let order = &doc["components"]["schemas"]["Order"]["properties"]["user"];
    assert_eq!(
        order["$ref"], "#/components/schemas/User",
        "a ManyToOne embed is advertised as the object itself: {doc}"
    );
}
