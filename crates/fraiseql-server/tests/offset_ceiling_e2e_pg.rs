//! #1306: `[validation] max_offset` refuses an offset page deeper than the operator allows,
//! before any statement, and names the cursor path that pages at any depth.
//!
//! `OFFSET n` reads `n` rows to discard them, whatever the ordering, so a deep offset page is
//! slow at any depth an index cannot skip. The ceiling is opt-in: unset, nothing is refused.
//! Set, every offset a client chooses is held to it, on every surface that takes one: a
//! GraphQL list's `offset:` and REST `?offset=`. (An embedded level takes no offset: REST
//! refuses `?rel.offset=` whatever the ceiling.)
//!
//! **Zero statements, proven on the database.** `booms` reads `v_boom`, a view whose every
//! row raises. A refused request against it answers with the ceiling's refusal; had any
//! statement reached the view, PostgreSQL's exception would be the answer instead.
//!
//! The document is compiled by the real compiler and its runtime config derived from the
//! compiled artifact, so the setting is driven `fraiseql.toml` → compile → load → request:
//! a key that parsed and did nothing would fail here.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** each rig drops and recreates its own `p1306_offset` schema → run
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

const SCHEMA: &str = "p1306_offset";

/// Five users with an order each; `v_boom` raises on every row it would return.
fn fraiseql_toml(max_offset: Option<u32>) -> String {
    let validation = match max_offset {
        Some(max) => format!("\n[validation]\nmax_offset = {max}\n"),
        None => String::new(),
    };
    format!(
        r#"
[schema]
name = "offset-1306"
version = "1.0.0"
database_target = "postgresql"
{validation}
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

[types.Boom]
sql_source = "{SCHEMA}.v_boom"
fields.id = {{ type = "Int" }}

[queries.users]
return_type = "User"
return_array = true
sql_source = "{SCHEMA}.v_user"

[queries.orders]
return_type = "Order"
return_array = true
sql_source = "{SCHEMA}.v_order"

[queries.booms]
return_type = "Boom"
return_array = true
sql_source = "{SCHEMA}.v_boom"
"#
    )
}

async fn seed(adapter: &PostgresAdapter) {
    for stmt in [
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!("CREATE TABLE {SCHEMA}.tb_user (id bigint PRIMARY KEY, name text NOT NULL)"),
        format!("CREATE TABLE {SCHEMA}.tb_order (id bigint PRIMARY KEY, fk_user bigint NOT NULL)"),
        format!("INSERT INTO {SCHEMA}.tb_user SELECT k, 'user ' || k FROM generate_series(1, 5) k"),
        format!("INSERT INTO {SCHEMA}.tb_order SELECT 10 + k, k FROM generate_series(1, 5) k"),
        format!(
            "CREATE VIEW {SCHEMA}.v_user AS SELECT id, jsonb_build_object('id', id, 'name', name) \
             AS data FROM {SCHEMA}.tb_user"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_order AS SELECT id, jsonb_build_object('id', id, 'fk_user', \
             fk_user) AS data FROM {SCHEMA}.tb_order"
        ),
        format!(
            "CREATE FUNCTION {SCHEMA}.boom(bigint) RETURNS jsonb LANGUAGE plpgsql AS $$ BEGIN \
             RAISE EXCEPTION 'v_boom was read'; END $$"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_boom AS SELECT id, {SCHEMA}.boom(id) AS data FROM \
             {SCHEMA}.tb_user"
        ),
    ] {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup");
    }
}

struct Rig {
    router:    axum::Router,
    executor:  Arc<Executor>,
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

    async fn graphql(&self, query: &str) -> fraiseql_core::error::Result<Value> {
        self.executor.execute(query, None).await
    }

    /// The same document from an authenticated caller: a separate runner arm.
    async fn graphql_as_alice(&self, query: &str) -> fraiseql_core::error::Result<Value> {
        let alice = fraiseql_core::security::SecurityContext {
            user_id:          "alice".into(),
            roles:            vec![],
            tenant_id:        None,
            scopes:           vec![],
            attributes:       std::collections::HashMap::new(),
            request_id:       "req-1306".to_string(),
            ip_address:       None,
            authenticated_at: chrono::Utc::now(),
            expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        };
        self.executor.execute_with_security(query, None, &alice).await
    }
}

async fn rig(max_offset: Option<u32>) -> Option<Rig> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    seed(&adapter).await;
    let (schema, temp_dir) = compile_document(max_offset).await;
    let runtime_config = fraiseql_core::runtime::RuntimeConfig::from_compiled_schema(&schema)
        .expect("the compiled document must yield a runtime config");
    let executor = Arc::new(Executor::with_config(schema, adapter, runtime_config));
    let state = AppState::new(Arc::clone(&executor));
    let router = rest_query_router(&state, &RestMountConfig::default()).expect("REST router");
    Some(Rig {
        router,
        executor,
        _temp_dir: temp_dir,
    })
}

/// Compile the document and load it the way a served artifact is loaded.
async fn compile_document(max_offset: Option<u32>) -> (CompiledSchema, TempDir) {
    let temp_dir = TempDir::new().expect("temp dir");
    let toml_path = temp_dir.path().join("fraiseql.toml");
    std::fs::write(&toml_path, fraiseql_toml(max_offset)).expect("write fraiseql.toml");
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

/// What a refusal must say: the ceiling, and the path that pages at any depth.
fn assert_names_the_cursor_path(message: &str) {
    assert!(message.contains("max_offset"), "names the ceiling: {message}");
    assert!(message.contains("relay = true"), "names how to get a connection: {message}");
    assert!(message.contains("after"), "names the cursor argument: {message}");
}

fn ids(rows: &Value) -> Vec<i64> {
    rows.as_array()
        .unwrap_or_else(|| panic!("not a list: {rows}"))
        .iter()
        .map(|r| r["id"].as_i64().unwrap())
        .collect()
}

#[tokio::test]
async fn a_rest_offset_at_the_ceiling_is_served_and_one_past_it_is_refused() {
    let Some(rig) = rig(Some(2)).await else {
        eprintln!("skipping #1306: DATABASE_URL not set");
        return;
    };
    let (status, body) = rig.get("/rest/v1/users?select=id&sort=id&offset=2").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(ids(&body["data"]), vec![3, 4, 5], "{body}");

    let (status, body) = rig.get("/rest/v1/booms?select=id&offset=3").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let message = body.to_string();
    assert!(!message.contains("v_boom was read"), "refused before any statement: {message}");
    assert_names_the_cursor_path(&message);
}

#[tokio::test]
async fn a_graphql_offset_past_the_ceiling_is_refused_before_any_statement() {
    let Some(rig) = rig(Some(2)).await else {
        return;
    };
    let served = rig.graphql("{ users(offset: 2, orderBy: {id: ASC}) { id } }").await.unwrap();
    assert_eq!(ids(&served["data"]["users"]), vec![3, 4, 5], "{served}");

    let err = rig
        .graphql("{ booms(offset: 3) { id } }")
        .await
        .expect_err("refused")
        .to_string();
    assert!(!err.contains("v_boom was read"), "refused before any statement: {err}");
    assert_names_the_cursor_path(&err);
}

#[tokio::test]
async fn an_authenticated_graphql_offset_past_the_ceiling_is_refused_before_any_statement() {
    let Some(rig) = rig(Some(2)).await else {
        return;
    };
    let served = rig
        .graphql_as_alice("{ users(offset: 2, orderBy: {id: ASC}) { id } }")
        .await
        .unwrap();
    assert_eq!(ids(&served["data"]["users"]), vec![3, 4, 5], "{served}");

    let err = rig
        .graphql_as_alice("{ booms(offset: 3) { id } }")
        .await
        .expect_err("refused")
        .to_string();
    assert!(!err.contains("v_boom was read"), "refused before any statement: {err}");
    assert_names_the_cursor_path(&err);
}

/// Unset (the default), nothing is refused however deep.
#[tokio::test]
async fn with_no_ceiling_a_deep_offset_is_served() {
    let Some(rig) = rig(None).await else {
        return;
    };
    let (status, body) = rig.get("/rest/v1/users?select=id&offset=100000").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"], json!([]), "{body}");
    let served = rig.graphql("{ users(offset: 100000) { id } }").await.unwrap();
    assert_eq!(served["data"]["users"], json!([]), "{served}");
}

/// A ceiling of 0 would refuse every offset page, which is not a ceiling: the compiler
/// refuses it, and so does the loader, for an artifact written by hand. A negative one is
/// not a number of rows. No database involved, so these cannot hide behind a skip.
#[tokio::test]
async fn a_ceiling_of_zero_or_below_is_refused_at_compile_and_at_load() {
    for (value, says) in [("0", "max_offset = 0"), ("-1", "max_offset")] {
        let temp_dir = TempDir::new().expect("temp dir");
        let toml_path = temp_dir.path().join("fraiseql.toml");
        let document =
            fraiseql_toml(Some(2)).replace("max_offset = 2", &format!("max_offset = {value}"));
        std::fs::write(&toml_path, document).expect("write fraiseql.toml");
        let err = compile_to_schema(CompileOptions {
            skip_hash: true,
            ..CompileOptions::new(toml_path.to_str().expect("utf-8 path"))
        })
        .await
        .expect_err("refused at compile");
        assert!(format!("{err:#}").contains(says), "{value}: {err:#}");
    }

    let (schema, _dir) = compile_document(Some(2)).await;
    let mut artifact: Value = serde_json::from_str(&schema.to_json().unwrap()).unwrap();
    artifact["validation_config"]["max_offset"] = json!(0);
    let err = CompiledSchema::from_json(&artifact.to_string(), false)
        .expect_err("refused at load")
        .to_string();
    assert!(err.contains("max_offset"), "{err}");
}

/// The suite's documents compile and load with no database.
#[tokio::test]
async fn the_document_loads_without_a_database() {
    compile_document(None).await;
    compile_document(Some(2)).await;
}
