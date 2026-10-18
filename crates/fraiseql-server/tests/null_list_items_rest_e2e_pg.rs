//! #1614: a `null` item at a `[T!]` position is refused over REST as it is over GraphQL.
//!
//! REST builds its read with `QueryMatch::from_operation` and never passes the GraphQL
//! matcher, so a rule that lived only there would not reach it. A filter operand list is
//! `[T!]` (`in`, `nin`): `NOT IN (NULL, 'first')` is never true in SQL, so a null item answered
//! every `nin` with no rows at all, under a `200`.
//!
//! The document is compiled by the real compiler, so the derived `LabelWhereInput` the
//! refusal reads is the one a served artifact carries.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** drops and recreates its own `p1614_items` schema → run
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

const SCHEMA: &str = "p1614_items";

fn fraiseql_toml() -> String {
    format!(
        r#"
[schema]
name = "items-1614"
version = "1.0.0"
database_target = "postgresql"

[rest]
enabled = true

[types.Label]
sql_source = "{SCHEMA}.v_label"
fields.id = {{ type = "Int" }}
fields.name = {{ type = "String" }}

[queries.labels]
return_type = "Label"
return_array = true
sql_source = "{SCHEMA}.v_label"
"#
    )
}

async fn seed(adapter: &PostgresAdapter) {
    for stmt in [
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!("CREATE TABLE {SCHEMA}.tb_label (id bigint PRIMARY KEY, name text NOT NULL)"),
        format!("INSERT INTO {SCHEMA}.tb_label VALUES (1, 'first'), (2, 'second'), (3, 'third')"),
        format!(
            "CREATE VIEW {SCHEMA}.v_label AS SELECT id, jsonb_build_object('id', id, 'name', \
             name) AS data FROM {SCHEMA}.tb_label"
        ),
    ] {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup");
    }
}

async fn compile_document() -> (CompiledSchema, TempDir) {
    let temp_dir = TempDir::new().expect("temp dir");
    let toml_path = temp_dir.path().join("fraiseql.toml");
    std::fs::write(&toml_path, fraiseql_toml()).expect("write fraiseql.toml");
    let (compiled, _) = compile_to_schema(CompileOptions {
        skip_hash: true,
        ..CompileOptions::new(toml_path.to_str().expect("utf-8 path"))
    })
    .await
    .expect("the authored document must compile");
    let mut schema =
        CompiledSchema::from_json(&compiled.schema.to_json().expect("serialize"), false)
            .expect("the compiler's own output must survive load");
    schema.build_indexes();
    (schema, temp_dir)
}

async fn router() -> Option<(axum::Router, TempDir)> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    seed(&adapter).await;
    let (schema, temp_dir) = compile_document().await;
    let runtime_config = fraiseql_core::runtime::RuntimeConfig::from_compiled_schema(&schema)
        .expect("runtime config");
    let executor = Arc::new(Executor::with_config(schema, adapter, runtime_config));
    let state = AppState::new(executor);
    let router = rest_query_router(&state, &RestMountConfig::default()).expect("REST router");
    Some((router, temp_dir))
}

async fn get(router: &axum::Router, filter: &Value) -> (StatusCode, Value) {
    let encoded: String =
        url::form_urlencoded::byte_serialize(filter.to_string().as_bytes()).collect();
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/rest/v1/labels?filter={encoded}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({"raw": String::from_utf8_lossy(&bytes)}));
    (status, json)
}

#[tokio::test]
async fn the_document_loads_without_a_database() {
    let (schema, _dir) = compile_document().await;
    assert!(schema.find_query("labels").is_some(), "the `labels` query is served");
    assert!(
        schema.input_types.iter().any(|t| t.name == "LabelWhereInput"),
        "the derived `LabelWhereInput` the refusal reads is carried"
    );
}

#[tokio::test]
async fn a_null_item_in_a_rest_filter_operand_list_is_refused() {
    let Some((router, _dir)) = router().await else {
        eprintln!("skipping #1614: DATABASE_URL not set");
        return;
    };

    let (status, body) = get(&router, &json!({ "name": { "nin": [null, "first"] } })).await;
    let (served_status, served) = get(&router, &json!({ "name": { "nin": ["first"] } })).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.to_string().contains("nin[0]"), "the refusal names the null item: {body}");
    assert_eq!(served_status, StatusCode::OK, "{served}");
    let mut names: Vec<&str> = served["data"]
        .as_array()
        .unwrap_or_else(|| panic!("{served}"))
        .iter()
        .map(|l| l["name"].as_str().unwrap())
        .collect();
    names.sort_unstable();
    assert_eq!(names, ["second", "third"], "{served}");
}
