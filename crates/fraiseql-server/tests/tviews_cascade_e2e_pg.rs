//! #1391: a cascade mutation opted into `cascade_source = "pg_tviews"` also serves the TVIEW
//! rows `pg_tviews` reports its transaction changed.
//!
//! `updatePost` renames a post. Its function lists the post in its own cascade, by hand;
//! the author's `tv_user` row carries its posts' titles, so `pg_tviews` refreshes it too, and
//! `tviews.pg_tviews_flush_and_report()` says so. The response lists both.
//!
//! Needs `pg_tviews`: the `postgres-tviews-test` service (`docker/pg-tviews`, pinned to
//! `v0.1.0-beta.26`), through `TVIEWS_DATABASE_URL`. Without it the suite skips.
//!
//! **Execution engine:** `PostgreSQL` + `pg_tviews` · **Infrastructure:** `TVIEWS_DATABASE_URL`
//! · **Parallelism:** drops and recreates its own `p1391` schema; installs the helper library
//! → run `--test-threads=1`.
#![cfg(feature = "metrics")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::sync::Arc;

use fraiseql_cli::schema::{ConvertOptions, SchemaConverter, intermediate::IntermediateSchema};
use fraiseql_core::{db::postgres::PostgresAdapter, schema::CompiledSchema};
use fraiseql_server::{Server, server_config::ServerConfig};
use serde_json::{Value, json};

const SCHEMA: &str = "p1391";
const POST: &str = "00000000-0000-0000-0000-000000001391";
const AUTHOR: &str = "00000000-0000-0000-0000-0000000013a1";
/// A second author's post: deleted by `deletePost`, renamed with the first by `renamePosts`.
const OTHER_POST: &str = "00000000-0000-0000-0000-000000001392";

fn tviews_database_url() -> Option<String> {
    std::env::var("TVIEWS_DATABASE_URL").ok().filter(|u| !u.is_empty())
}

async fn provision(url: &str) {
    let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client
        .batch_execute(
            "CREATE EXTENSION IF NOT EXISTS pg_tviews;
             CREATE EXTENSION IF NOT EXISTS pg_stat_statements;",
        )
        .await
        .unwrap();
    client
        .batch_execute(include_str!("../../fraiseql-cli/sql/helpers/mutation_response.sql"))
        .await
        .unwrap();
    for stmt in fraiseql_test_support::changelog::entity_change_log_provision_statements() {
        client.batch_execute(&stmt).await.unwrap_or_else(|e| panic!("{stmt}: {e}"));
    }
    client
        .batch_execute(&format!(
            "SET search_path = {SCHEMA}, public;
             DROP VIEW IF EXISTS v_post, v_user;
             RESET search_path;
             DROP SCHEMA IF EXISTS {SCHEMA} CASCADE;
             CREATE SCHEMA {SCHEMA};
             SET search_path = {SCHEMA}, public;
             CREATE TYPE mutation_response AS (succeeded boolean, state_changed boolean,
               error_class text, status_detail text, http_status smallint, message text,
               entity_id uuid, entity_type text, entity jsonb, updated_fields text[],
               cascade jsonb, error_detail jsonb, metadata jsonb);
             CREATE TABLE tb_user (pk_user bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
               id uuid NOT NULL UNIQUE, name text NOT NULL);
             CREATE TABLE tb_post (pk_post bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
               id uuid NOT NULL UNIQUE, fk_user bigint NOT NULL REFERENCES tb_user,
               title text NOT NULL);
             INSERT INTO tb_user (id, name) VALUES ('{AUTHOR}', 'ada');
             INSERT INTO tb_post (id, fk_user, title) SELECT '{POST}', pk_user, 'first'
               FROM tb_user;
             INSERT INTO tb_user (id, name) VALUES ('00000000-0000-0000-0000-0000000013a2', 'bob');
             INSERT INTO tb_post (id, fk_user, title) SELECT '{OTHER_POST}', pk_user, 'other'
               FROM tb_user WHERE name = 'bob';
             CREATE TABLE tv_post AS SELECT p.pk_post, p.id, jsonb_build_object('id', p.id,
               'title', p.title) AS data FROM tb_post p;
             CREATE TABLE tv_user AS SELECT u.pk_user, u.id, jsonb_build_object('id', u.id,
               'name', u.name, 'titles', coalesce((SELECT jsonb_agg(p.title ORDER BY p.pk_post)
               FROM tb_post p WHERE p.fk_user = u.pk_user), '[]'::jsonb)) AS data
               FROM tb_user u;
             CREATE VIEW v_post WITH (security_invoker = true) AS SELECT id, data FROM tv_post;
             -- `p1391_reader` is the caller who may not read users: its view returns none.
             CREATE VIEW v_user WITH (security_invoker = true) AS SELECT id, data FROM tv_user
               WHERE current_user <> 'p1391_reader';
             CREATE FUNCTION fn_update_post(p_id uuid, p_title text)
               RETURNS SETOF {SCHEMA}.mutation_response LANGUAGE plpgsql AS $$
             DECLARE v_post jsonb;
             BEGIN
               UPDATE {SCHEMA}.tb_post SET title = p_title WHERE id = p_id;
               SELECT data INTO v_post FROM {SCHEMA}.tv_post WHERE id = p_id;
               RETURN QUERY SELECT * FROM fraiseql.mutation_ok(v_post, p_id, 'Post',
                 p_cascade => jsonb_build_object('updated', jsonb_build_array(
                   jsonb_build_object('__typename', 'Post', 'id', p_id,
                                      'operation', 'UPDATED', 'entity', v_post))));
             END $$;
             -- Lists the post with data of its own: the function's entry wins.
             CREATE FUNCTION fn_update_post_own(p_id uuid, p_title text)
               RETURNS SETOF {SCHEMA}.mutation_response LANGUAGE plpgsql AS $$
             BEGIN
               UPDATE {SCHEMA}.tb_post SET title = p_title WHERE id = p_id;
               RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
                 jsonb_build_object('id', p_id, 'title', p_title), p_id, 'Post',
                 p_cascade => jsonb_build_object('updated', jsonb_build_array(
                   jsonb_build_object('__typename', 'Post', 'id', p_id, 'operation', 'UPDATED',
                     'entity', jsonb_build_object('id', p_id, 'title', 'from-function')))));
             END $$;
             -- Deletes a post and lists nothing: pg_tviews reports it deleted, its author
             -- updated.
             CREATE FUNCTION fn_delete_post(p_id uuid, p_title text)
               RETURNS SETOF {SCHEMA}.mutation_response LANGUAGE plpgsql AS $$
             BEGIN
               DELETE FROM {SCHEMA}.tb_post WHERE id = p_id;
               RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
                 jsonb_build_object('id', p_id, 'title', p_title), p_id, 'Post');
             END $$;
             -- Deletes a post and lists the deletion itself, as pg_tviews will.
             CREATE FUNCTION fn_delete_post_own(p_id uuid, p_title text)
               RETURNS SETOF {SCHEMA}.mutation_response LANGUAGE plpgsql AS $$
             BEGIN
               DELETE FROM {SCHEMA}.tb_post WHERE id = p_id;
               RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
                 jsonb_build_object('id', p_id, 'title', p_title), p_id, 'Post',
                 p_cascade => jsonb_build_object('deleted', jsonb_build_array(
                   jsonb_build_object('__typename', 'Post', 'id', p_id,
                                      'deletedAt', '2026-01-01T00:00:00Z'))));
             END $$;
             -- Renames every post and lists nothing: two posts, two authors.
             CREATE FUNCTION fn_rename_posts(p_id uuid, p_title text)
               RETURNS SETOF {SCHEMA}.mutation_response LANGUAGE plpgsql AS $$
             BEGIN
               UPDATE {SCHEMA}.tb_post SET title = p_title;
               RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
                 jsonb_build_object('id', p_id, 'title', p_title), p_id, 'Post');
             END $$;
             -- Calls the flush itself (reset), and returns its report as its own cascade.
             CREATE FUNCTION fn_update_post_self(p_id uuid, p_title text)
               RETURNS SETOF {SCHEMA}.mutation_response LANGUAGE plpgsql AS $$
             DECLARE v_report jsonb; v_updated jsonb;
             BEGIN
               UPDATE {SCHEMA}.tb_post SET title = p_title WHERE id = p_id;
               v_report := tviews.pg_tviews_flush_and_report();
               SELECT coalesce(jsonb_agg(e - 'data' || jsonb_build_object('entity', e -> 'data')),
                               '[]'::jsonb)
                 INTO v_updated FROM jsonb_array_elements(v_report -> 'updated') e;
               RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
                 (SELECT data FROM {SCHEMA}.tv_post WHERE id = p_id), p_id, 'Post',
                 p_cascade => jsonb_build_object('updated', v_updated));
             END $$;
             RESET search_path;
             DO $$ BEGIN CREATE ROLE p1391_reader LOGIN PASSWORD 'p1391_reader';
               EXCEPTION WHEN duplicate_object THEN NULL; END $$;
             GRANT USAGE ON SCHEMA {SCHEMA}, fraiseql, core, tviews TO p1391_reader;
             GRANT SELECT, UPDATE ON {SCHEMA}.tb_post TO p1391_reader;
             GRANT SELECT ON ALL TABLES IN SCHEMA {SCHEMA} TO p1391_reader;
             GRANT INSERT ON core.tb_entity_change_log TO p1391_reader;
             GRANT USAGE ON ALL SEQUENCES IN SCHEMA core TO p1391_reader;"
        ))
        .await
        .unwrap();
}

/// `url` with its credentials replaced by the reader's.
fn as_reader(url: &str) -> String {
    let (scheme, rest) = url.split_once("://").unwrap();
    let host = rest.split_once('@').map_or(rest, |(_, host)| host);
    format!("{scheme}://p1391_reader:p1391_reader@{host}")
}

/// The schema an SDK authors, compiled: `Post` and `User` read from their views, and
/// `updatePost`, a cascade mutation whose cascade also comes from `pg_tviews`.
fn schema() -> CompiledSchema {
    let mutations: Vec<Value> = [
        "updatePost",
        "updatePostOwn",
        "updatePostSelf",
        "updatePostPlain",
        "deletePost",
        "deletePostOwn",
        "renamePosts",
    ]
    .into_iter()
    .map(|name| {
        let function = match name {
            "updatePostOwn" => "fn_update_post_own",
            "updatePostSelf" => "fn_update_post_self",
            "deletePost" => "fn_delete_post",
            "deletePostOwn" => "fn_delete_post_own",
            "renamePosts" => "fn_rename_posts",
            _ => "fn_update_post",
        };
        // `updatePostPlain` is a cascade mutation that did not opt in.
        let source = if name == "updatePostPlain" {
            "function"
        } else {
            "pg_tviews"
        };
        json!({
            "name": name, "return_type": "Post", "operation": "update",
            "cascade": true, "cascade_source": source,
            "sql_source": format!("{SCHEMA}.{function}"),
            "arguments": [{ "name": "id", "type": "ID", "nullable": false },
                          { "name": "title", "type": "String", "nullable": false }]
        })
    })
    .collect();
    let intermediate: IntermediateSchema = serde_json::from_value(json!({
        "types": [
            {
                "name": "Post", "sql_source": format!("{SCHEMA}.v_post"), "is_input": false,
                "fields": [{ "name": "id", "type": "ID", "nullable": false },
                           { "name": "title", "type": "String", "nullable": false }]
            },
            {
                "name": "User", "sql_source": format!("{SCHEMA}.v_user"), "is_input": false,
                "fields": [{ "name": "id", "type": "ID", "nullable": false },
                           { "name": "name", "type": "String", "nullable": false },
                           { "name": "titles", "type": "[String!]", "nullable": false }]
            }
        ],
        "queries": [
            { "name": "posts", "return_type": "Post", "returns_list": true, "nullable": false,
              "sql_source": format!("{SCHEMA}.v_post"), "arguments": [] },
            { "name": "users", "return_type": "User", "returns_list": true, "nullable": false,
              "sql_source": format!("{SCHEMA}.v_user"), "arguments": [] }
        ],
        "mutations": mutations
    }))
    .unwrap();
    let mut schema = SchemaConverter::convert_artifact(intermediate, &ConvertOptions::default())
        .unwrap()
        .schema;
    schema.build_indexes();
    schema
}

struct Running {
    base:      String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

async fn serve() -> Option<Running> {
    let url = tviews_database_url()?;
    provision(&url).await;
    Some(serve_as(url).await)
}

async fn serve_as(url: String) -> Running {
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    let config = ServerConfig {
        database_url: url,
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = Box::pin(Server::new(config, schema(), adapter, None)).await.unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    Running {
        base:      format!("http://127.0.0.1:{port}"),
        _shutdown: tx,
    }
}

/// `mutation`'s document: the cascade's updated entries and invalidations.
fn document(mutation: &str, title: &str) -> String {
    let payload = format!("{}{}Payload", mutation[..1].to_uppercase(), &mutation[1..]);
    format!(
        "mutation {{ {mutation}(id: \"{POST}\", title: \"{title}\") {{ ... on {payload} {{ \
         cascade {{ updated {{ id operation entity {{ __typename ... on Post {{ title }} \
         ... on User {{ name titles }} }} }} invalidations {{ queryName scope }} }} }} }} }}"
    )
}

async fn update_post(server: &Running, title: &str) -> Value {
    run(server, "updatePost", title).await
}

async fn run(server: &Running, mutation: &str, title: &str) -> Value {
    let query = document(mutation, title);
    reqwest::Client::new()
        .post(format!("{}/graphql", server.base))
        .json(&json!({ "query": query }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

fn updated(body: &Value) -> Vec<(String, Value)> {
    let (_, payload) = body["data"].as_object().unwrap().iter().next().unwrap();
    payload["cascade"]["updated"]
        .as_array()
        .unwrap_or_else(|| panic!("a cascade: {body}"))
        .iter()
        .map(|e| {
            let mut entity = e["entity"].clone();
            let typename = entity
                .as_object_mut()
                .and_then(|o| o.remove("__typename"))
                .and_then(|t| t.as_str().map(str::to_string))
                .unwrap_or_default();
            (typename, entity)
        })
        .collect()
}

/// The post the function listed, and the author `pg_tviews` refreshed with it.
#[tokio::test]
async fn the_cascade_lists_the_rows_pg_tviews_refreshed() {
    let Some(server) = serve().await else {
        eprintln!("skipping #1391: TVIEWS_DATABASE_URL not set");
        return;
    };
    let body = update_post(&server, "renamed").await;
    assert!(body.get("errors").is_none(), "{body}");
    let entries = updated(&body);
    assert!(
        entries.contains(&("Post".to_string(), json!({ "title": "renamed" }))),
        "the function's own entry: {body}"
    );
    assert!(
        entries.contains(&("User".to_string(), json!({ "name": "ada", "titles": ["renamed"] }))),
        "the author pg_tviews refreshed: {body}"
    );
    assert_eq!(entries.len(), 2, "each row once: {body}");
}

/// A derived row the caller's own read does not return is not served: the reader's view
/// returns no user. Its type is invalidated instead, one hint per root query returning it.
#[tokio::test]
async fn a_derived_row_the_caller_cannot_read_is_invalidated_not_served() {
    let Some(url) = tviews_database_url() else {
        return;
    };
    provision(&url).await;
    let reader = serve_as(as_reader(&url)).await;
    let body = update_post(&reader, "as-reader").await;
    assert!(body.get("errors").is_none(), "{body}");
    let entries = updated(&body);
    assert_eq!(
        entries,
        [("Post".to_string(), json!({ "title": "as-reader" }))],
        "no derived User for a caller who cannot read one: {body}"
    );
    assert_eq!(
        body["data"]["updatePost"]["cascade"]["invalidations"],
        json!([{ "queryName": "users", "scope": "PREFIX" }]),
        "{body}"
    );
}

/// The function's entry for a row wins over the report's: the post keeps the function's
/// data, and the author is still added.
#[tokio::test]
async fn the_functions_own_entry_wins() {
    let Some(server) = serve().await else {
        return;
    };
    let body = run(&server, "updatePostOwn", "renamed").await;
    let entries = updated(&body);
    assert!(
        entries.contains(&("Post".to_string(), json!({ "title": "from-function" }))),
        "{body}"
    );
    assert!(entries.iter().any(|(t, _)| t == "User"), "{body}");
    assert_eq!(entries.len(), 2, "the post once: {body}");
}

/// A function that calls the flush itself owns its report: the executor's call then reports
/// nothing new, and no row is listed twice.
#[tokio::test]
async fn a_function_that_flushes_itself_is_not_listed_twice() {
    let Some(server) = serve().await else {
        return;
    };
    let body = run(&server, "updatePostSelf", "self-flushed").await;
    assert!(body.get("errors").is_none(), "{body}");
    let mut types: Vec<String> = updated(&body).into_iter().map(|(t, _)| t).collect();
    types.sort();
    assert_eq!(types, ["Post", "User"], "{body}");
}

/// Truncated past `max_entities`: `pg_tviews` names the types it left out, and each is
/// invalidated rather than listed in part.
#[tokio::test]
async fn a_truncated_report_invalidates_the_types_it_left_out() {
    use fraiseql_core::runtime::{CascadeLimits, Executor, RuntimeConfig};

    let Some(url) = tviews_database_url() else {
        return;
    };
    provision(&url).await;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    let config = RuntimeConfig {
        cascade_limits: CascadeLimits {
            max_updated_entities: 1,
            ..CascadeLimits::default()
        },
        ..RuntimeConfig::default()
    };
    let executor = Executor::with_config(schema(), adapter, config);
    let body = executor.execute(&document("updatePost", "truncated"), None).await.unwrap();
    assert_eq!(
        updated(&body),
        [("Post".to_string(), json!({ "title": "truncated" }))],
        "{body}"
    );
    assert_eq!(
        body["data"]["updatePost"]["cascade"]["invalidations"],
        json!([{ "queryName": "users", "scope": "PREFIX" }]),
        "{body}"
    );
}

/// The flush runs for a mutation that opted in, and only for one: `pg_stat_statements`
/// counts the statement on the real function.
#[tokio::test]
async fn pg_tviews_is_called_only_for_a_mutation_that_opted_in() {
    let Some(server) = serve().await else {
        return;
    };
    let url = tviews_database_url().unwrap();
    let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    let flushes = || async {
        client
            .query_one(
                "SELECT coalesce(sum(calls), 0)::bigint FROM pg_stat_statements \
                 WHERE query LIKE '%pg_tviews_flush_and_report%' \
                   AND query NOT LIKE '%pg_stat_statements%'",
                &[],
            )
            .await
            .unwrap()
            .get::<_, i64>(0)
    };
    client.batch_execute("SELECT pg_stat_statements_reset()").await.unwrap();
    let body = run(&server, "updatePostPlain", "plain").await;
    assert!(body.get("errors").is_none(), "{body}");
    assert_eq!(flushes().await, 0, "a mutation that did not opt in calls no flush");
    run(&server, "updatePost", "opted").await;
    assert!(flushes().await >= 1, "an opted-in mutation calls it");
}

/// Opted in against a database without `pg_tviews` (an artifact compiled without
/// `--database`), the write fails loudly, naming `pg_tviews`, and commits nothing: never an
/// empty cascade.
#[tokio::test]
async fn without_pg_tviews_an_opted_in_write_fails_loudly() {
    use fraiseql_core::runtime::Executor;

    let Some(url) = fraiseql_test_support::try_database_url() else {
        return;
    };
    let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    for stmt in fraiseql_test_support::changelog::entity_change_log_provision_statements() {
        client.batch_execute(&stmt).await.unwrap_or_else(|e| panic!("{stmt}: {e}"));
    }
    client
        .batch_execute(
            "DROP SCHEMA IF EXISTS p1391_absent CASCADE; CREATE SCHEMA p1391_absent;
             CREATE TYPE p1391_absent.mutation_response AS (succeeded boolean, state_changed
               boolean, error_class text, status_detail text, http_status smallint, message
               text, entity_id uuid, entity_type text, entity jsonb, updated_fields text[],
               cascade jsonb, error_detail jsonb, metadata jsonb, result jsonb);
             CREATE TABLE p1391_absent.tb_post (id uuid PRIMARY KEY, title text);
             INSERT INTO p1391_absent.tb_post VALUES ('00000000-0000-0000-0000-000000001391', 'a');
             CREATE VIEW p1391_absent.v_post AS SELECT id, jsonb_build_object('id', id,
               'title', title) AS data FROM p1391_absent.tb_post;
             CREATE FUNCTION p1391_absent.fn_update_post(p_id uuid, p_title text)
               RETURNS SETOF p1391_absent.mutation_response LANGUAGE plpgsql AS $$
             DECLARE v p1391_absent.mutation_response;
             BEGIN
               UPDATE p1391_absent.tb_post SET title = p_title WHERE id = p_id;
               v.succeeded := true; v.state_changed := true; v.entity_type := 'Post';
               v.entity := (SELECT data FROM p1391_absent.v_post WHERE id = p_id);
               RETURN NEXT v;
             END $$;",
        )
        .await
        .unwrap();
    let has_pg_tviews: bool = client
        .query_one("SELECT to_regnamespace('tviews') IS NOT NULL", &[])
        .await
        .unwrap()
        .get(0);
    assert!(!has_pg_tviews, "DATABASE_URL must be a database without pg_tviews");
    let mut compiled = schema();
    for t in &mut compiled.types {
        if t.name == "Post" {
            t.sql_source = "p1391_absent.v_post".into();
        }
    }
    for m in &mut compiled.mutations {
        m.sql_source = Some("p1391_absent.fn_update_post".to_string());
    }
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    let executor = Executor::new(compiled, adapter);
    let result = executor.execute(&document("updatePost", "absent"), None).await;
    let rendered = match result {
        Ok(body) => body.to_string(),
        Err(e) => e.to_string(),
    };
    assert!(rendered.contains("pg_tviews"), "names pg_tviews: {rendered}");
    let title: String = client
        .query_one("SELECT title FROM p1391_absent.tb_post", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(title, "a", "nothing committed");
}

/// The number of entries a derived cascade served is observed on `/metrics`.
#[tokio::test]
async fn the_derived_fan_out_is_observed() {
    let Some(server) = serve().await else {
        return;
    };
    fraiseql_server::metrics_recorder::install();
    run(&server, "updatePost", "observed").await;
    let rendered = fraiseql_server::metrics_recorder::render();
    assert!(rendered.contains("fraiseql_cascade_derived_entries"), "{rendered}");
}

/// A row `pg_tviews` reports deleted is served as a deleted entry, stamped with the time; the
/// author it changed with is updated.
#[tokio::test]
async fn a_deleted_row_is_served_as_deleted() {
    let Some(server) = serve().await else {
        return;
    };
    let query = format!(
        "mutation {{ deletePost(id: \"{OTHER_POST}\", title: \"other\") {{ ... on \
         DeletePostPayload {{ cascade {{ deleted {{ id deletedAt }} updated {{ entity {{ \
         __typename ... on User {{ name titles }} }} }} }} }} }} }}"
    );
    let body: Value = reqwest::Client::new()
        .post(format!("{}/graphql", server.base))
        .json(&json!({ "query": query }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let cascade = &body["data"]["deletePost"]["cascade"];
    let deleted = cascade["deleted"].as_array().unwrap_or_else(|| panic!("{body}"));
    assert_eq!(deleted.len(), 1, "{body}");
    assert_eq!(deleted[0]["id"], OTHER_POST, "{body}");
    assert!(deleted[0]["deletedAt"].as_str().is_some_and(|t| t.ends_with('Z')), "{body}");
    assert_eq!(
        cascade["updated"],
        json!([{ "entity": { "__typename": "User", "name": "bob", "titles": [] } }]),
        "{body}"
    );
}

/// A deletion the function lists itself is served once, as the function listed it.
#[tokio::test]
async fn a_deletion_the_function_lists_is_served_once() {
    let Some(server) = serve().await else {
        return;
    };
    let query = format!(
        "mutation {{ deletePostOwn(id: \"{OTHER_POST}\", title: \"other\") {{ ... on \
         DeletePostOwnPayload {{ cascade {{ deleted {{ id deletedAt }} }} }} }} }}"
    );
    let body: Value = reqwest::Client::new()
        .post(format!("{}/graphql", server.base))
        .json(&json!({ "query": query }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        body["data"]["deletePostOwn"]["cascade"]["deleted"],
        json!([{ "id": OTHER_POST, "deletedAt": "2026-01-01T00:00:00Z" }]),
        "{body}"
    );
}

/// A type `pg_tviews` truncated within (two posts changed, one reported) lists none of its
/// rows: a partial set would read as the whole one.
#[tokio::test]
async fn a_type_truncated_in_part_lists_none_of_its_rows() {
    use fraiseql_core::runtime::{CascadeLimits, Executor, RuntimeConfig};

    let Some(url) = tviews_database_url() else {
        return;
    };
    provision(&url).await;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    let config = RuntimeConfig {
        cascade_limits: CascadeLimits {
            max_updated_entities: 1,
            ..CascadeLimits::default()
        },
        ..RuntimeConfig::default()
    };
    let executor = Executor::with_config(schema(), adapter, config);
    let body = executor.execute(&document("renamePosts", "all"), None).await.unwrap();
    assert_eq!(updated(&body), [], "{body}");
    assert_eq!(
        body["data"]["renamePosts"]["cascade"]["invalidations"],
        json!([{ "queryName": "posts", "scope": "PREFIX" }, { "queryName": "users", "scope": "PREFIX" }]),
        "{body}"
    );
}

/// The suite's document compiles with no database.
#[test]
fn the_document_loads_without_a_database() {
    let schema = schema();
    assert!(schema.find_type("UpdatePostPayload").is_some());
}
