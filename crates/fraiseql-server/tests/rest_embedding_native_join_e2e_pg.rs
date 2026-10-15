//! #1587: a composed read correlates on the columns `pg_tviews` proves, end to end.
//!
//! `fraiseql compile --database` reads `tviews.registry.references`: `tv_c1587.fk_p1587` holds
//! `tv_p1587`'s identity `pk_p1587`, proven by a `FOREIGN KEY`. The compiled relationship then
//! carries that native join, the composed read correlates `"fk_p1587" = _l0."_j0"` (served by
//! the managed `(fk_p1587, pk_c1587)` index), and each post still gets exactly its comments.
//!
//! Needs `pg_tviews` with `tviews.registry.references`, through `TVIEWS_DATABASE_URL`.
//! Without it the suite skips.
//!
//! **Execution engine:** `PostgreSQL` + `pg_tviews` · **Infrastructure:** `TVIEWS_DATABASE_URL`
//! · **Parallelism:** drops and recreates its own `p1587` schema → run `--test-threads=1`.
#![cfg(feature = "rest")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::sync::Arc;

use axum::body::Body;
use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::{db::postgres::PostgresAdapter, runtime::Executor, schema::CompiledSchema};
use fraiseql_server::routes::{
    graphql::AppState,
    rest::{RestMountConfig, rest_query_router},
};
use http::{Request, StatusCode};
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt;

const SCHEMA: &str = "p1587";

fn tviews_database_url() -> Option<String> {
    std::env::var("TVIEWS_DATABASE_URL").ok().filter(|u| !u.is_empty())
}

async fn client(url: &str) -> tokio_postgres::Client {
    let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client
}

/// Two posts, with two comments and one; `false` when the registry has no references.
async fn provision(url: &str) -> bool {
    let client = client(url).await;
    client.batch_execute("CREATE EXTENSION IF NOT EXISTS pg_tviews;").await.unwrap();
    let has_references: bool = client
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM pg_attribute WHERE attrelid = \
             to_regclass('tviews.registry') AND attname = 'references' AND NOT attisdropped)",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    if !has_references {
        return false;
    }
    client
        .batch_execute(&format!(
            "DO $$ DECLARE t text; BEGIN
               FOR t IN SELECT schema || '.' || name FROM tviews.registry
                        WHERE schema = '{SCHEMA}' ORDER BY entity LOOP
                 PERFORM tviews.pg_tviews_drop(t);
               END LOOP; END $$;
             DROP SCHEMA IF EXISTS {SCHEMA} CASCADE;
             CREATE SCHEMA {SCHEMA};
             CREATE TABLE {SCHEMA}.tb_p1587 (pk_p1587 int PRIMARY KEY, id uuid NOT NULL UNIQUE
                                             DEFAULT gen_random_uuid(), title text NOT NULL);
             CREATE TABLE {SCHEMA}.tb_c1587 (pk_c1587 int PRIMARY KEY, id uuid NOT NULL UNIQUE
                                             DEFAULT gen_random_uuid(),
                                             fk_p1587 int NOT NULL REFERENCES {SCHEMA}.tb_p1587,
                                             body text NOT NULL);
             INSERT INTO {SCHEMA}.tb_p1587 (pk_p1587, title) VALUES (1, 'A'), (2, 'B');
             INSERT INTO {SCHEMA}.tb_c1587 (pk_c1587, fk_p1587, body)
               VALUES (1, 1, 'a1'), (2, 1, 'a2'), (3, 2, 'b1');
             SELECT tviews.pg_tviews_create('{SCHEMA}.tv_p1587', $q$
               SELECT p.pk_p1587, p.id, jsonb_build_object('id', p.id, 'pk_p1587', p.pk_p1587, 'title', p.title) AS data
               FROM {SCHEMA}.tb_p1587 p $q$);
             SELECT tviews.pg_tviews_create('{SCHEMA}.tv_c1587', $q$
               SELECT c.pk_c1587, c.id, c.fk_p1587,
                      jsonb_build_object('id', c.id, 'fk_p1587', c.fk_p1587, 'body', c.body) AS data
               FROM {SCHEMA}.tb_c1587 c JOIN {SCHEMA}.tb_p1587 p ON p.pk_p1587 = c.fk_p1587 $q$);"
        ))
        .await
        .unwrap();
    true
}

fn fraiseql_toml() -> String {
    format!(
        r#"
[schema]
name = "native-join-1587"
version = "1.0.0"
database_target = "postgresql"

[rest]
enabled = true

[types.Post]
sql_source = "{SCHEMA}.tv_p1587"
fields.id = {{ type = "ID" }}
fields.pk_p1587 = {{ type = "Int" }}
fields.title = {{ type = "String" }}

[types.Post.relationships.comments]
target_type = "Comment"
cardinality = "OneToMany"
foreign_key = "fk_p1587"
referenced_key = "pk_p1587"

[types.Comment]
sql_source = "{SCHEMA}.tv_c1587"
fields.id = {{ type = "ID" }}
fields.fk_p1587 = {{ type = "Int" }}
fields.body = {{ type = "String" }}

[queries.posts]
return_type = "Post"
return_array = true
sql_source = "{SCHEMA}.tv_p1587"

[queries.comments]
return_type = "Comment"
return_array = true
sql_source = "{SCHEMA}.tv_c1587"
"#
    )
}

/// The document above, compiled against the database.
async fn compile(url: &str) -> (CompiledSchema, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let toml_path = temp_dir.path().join("fraiseql.toml");
    std::fs::write(&toml_path, fraiseql_toml()).unwrap();
    let (compiled, _) = compile_to_schema(CompileOptions {
        skip_hash: true,
        database: Some(url),
        ..CompileOptions::new(toml_path.to_str().unwrap())
    })
    .await
    .unwrap();
    let mut schema = CompiledSchema::from_json(&compiled.schema.to_json().unwrap(), false).unwrap();
    schema.build_indexes();
    (schema, temp_dir)
}

#[tokio::test]
async fn a_composed_read_correlates_on_the_columns_pg_tviews_proves() {
    let Some(url) = tviews_database_url() else {
        eprintln!("skipping: TVIEWS_DATABASE_URL not set");
        return;
    };
    if !provision(&url).await {
        eprintln!("skipping: pg_tviews in TVIEWS_DATABASE_URL has no tviews.registry.references");
        return;
    }
    let (schema, _dir) = compile(&url).await;

    let comments = schema
        .find_type("Post")
        .unwrap()
        .relationships
        .iter()
        .find(|r| r.name == "comments")
        .unwrap();
    let join = comments.native_join.as_ref().expect("compile --database proves the join");
    assert_eq!(
        (join.foreign_key.as_str(), join.referenced_key.as_str()),
        ("fk_p1587", "pk_p1587")
    );

    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    let state = AppState::new(Arc::new(Executor::new(schema, adapter)));
    let router = rest_query_router(&state, &RestMountConfig::default()).unwrap();
    let response = router
        .oneshot(
            Request::builder()
                .uri("/rest/v1/posts?select=title,comments(body)")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();

    let mut served: Vec<(String, Vec<String>)> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|post| {
            let mut bodies: Vec<String> = post["comments"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c["body"].as_str().unwrap().to_string())
                .collect();
            bodies.sort();
            (post["title"].as_str().unwrap().to_string(), bodies)
        })
        .collect();
    served.sort();
    assert_eq!(
        served,
        vec![
            ("A".to_string(), vec!["a1".to_string(), "a2".to_string()]),
            ("B".to_string(), vec!["b1".to_string()]),
        ],
        "{body}"
    );
}

#[tokio::test]
async fn a_server_refuses_a_proof_its_database_does_not_hold() {
    let Some(url) = tviews_database_url() else {
        eprintln!("skipping: TVIEWS_DATABASE_URL not set");
        return;
    };
    if !provision(&url).await {
        eprintln!("skipping: pg_tviews in TVIEWS_DATABASE_URL has no tviews.registry.references");
        return;
    }
    let (mut schema, _dir) = compile(&url).await;
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    fraiseql_core::schema::refuse_unproven_native_joins(&adapter, &schema)
        .await
        .unwrap();

    // Compiled against another database: the column is not here.
    let post = schema.types.iter_mut().find(|t| t.name == "Post").unwrap();
    let comments = post.relationships.iter_mut().find(|r| r.name == "comments").unwrap();
    comments.native_join.as_mut().unwrap().foreign_key = "fk_gone".to_string();
    let error = fraiseql_core::schema::refuse_unproven_native_joins(&adapter, &schema)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("Post.comments: p1587.tv_c1587.fk_gone"), "{error}");
    assert!(error.contains("Recompile with `fraiseql compile --database`"), "{error}");
}
