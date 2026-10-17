//! #1391: a cascade mutation opted into `cascade_source = "pg_tviews"` also serves the TVIEW
//! rows `pg_tviews` reports its transaction changed.
//!
//! `updatePost` renames a post. Its function lists the post in its own cascade, by hand;
//! the author's `tv_user` row carries its posts' titles, so `pg_tviews` refreshes it too, and
//! `tviews.pg_tviews_flush_and_report()` says so. The response lists both.
//!
//! Needs `pg_tviews`: the `postgres-tviews-test` service (`docker/pg-tviews`, pinned to
//! `v0.1.0-beta.31`), through `TVIEWS_DATABASE_URL`. Without it the suite skips.
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
             -- Ordered against the report (which lists users by key): rows matched back to
             -- their entries by position would swap the two authors.
             CREATE VIEW v_user WITH (security_invoker = true) AS SELECT id, data FROM tv_user
               WHERE current_user <> 'p1391_reader' ORDER BY data ->> 'name' DESC;
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
    schema_with_inject("", &json!({}))
}

/// [`schema`], its `query` (`posts` or `users`) reading through an engine row filter
/// (`inject`, e.g. `{"owner": "jwt:sub"}`): a read of its type is then filtered by the
/// engine, not only by the database.
fn schema_with_inject(query: &str, inject: &Value) -> CompiledSchema {
    let inject_for = |name: &str| {
        if name == query {
            inject.clone()
        } else {
            json!({})
        }
    };
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
              "sql_source": format!("{SCHEMA}.v_post"), "arguments": [],
              "inject_params": inject_for("posts") },
            { "name": "users", "return_type": "User", "returns_list": true, "nullable": false,
              "sql_source": format!("{SCHEMA}.v_user"), "arguments": [],
              "inject_params": inject_for("users") }
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
    serve_schema(url, schema()).await
}

/// The `/metrics` bearer token every server here is started with.
const METRICS_TOKEN: &str = "p1391-metrics-token-0123456789-0123456789";

async fn serve_schema(url: String, schema: CompiledSchema) -> Running {
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    serve_adapter(url, schema, adapter).await
}

async fn serve_adapter(
    url: String,
    schema: CompiledSchema,
    adapter: Arc<PostgresAdapter>,
) -> Running {
    let config = ServerConfig {
        database_url: url,
        cors_enabled: false,
        metrics_enabled: true,
        metrics_token: Some(METRICS_TOKEN.to_string()),
        ..ServerConfig::default()
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = Box::pin(Server::new(config, schema, adapter, None)).await.unwrap();
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

/// `p1601`: a schema the reader may not use, holding a TVIEW with a reference column
/// (`tv_r1601.fk_q1601 → tv_q1601`), as another application's schema in the same database
/// would. Returns the references `tviews.registry` records for `tv_r1601`.
async fn provision_unusable_schema(url: &str) -> Value {
    let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client
        .batch_execute(
            "DO $$ DECLARE t text; BEGIN
               FOR t IN SELECT schema || '.' || name FROM tviews.registry
                        WHERE schema = 'p1601' ORDER BY entity DESC LOOP
                 PERFORM tviews.pg_tviews_drop(t);
               END LOOP; END $$;
             DROP SCHEMA IF EXISTS p1601 CASCADE;
             CREATE SCHEMA p1601;
             CREATE TABLE p1601.tb_q1601 (pk_q1601 int PRIMARY KEY, id uuid NOT NULL UNIQUE);
             CREATE TABLE p1601.tb_r1601 (pk_r1601 int PRIMARY KEY, id uuid NOT NULL UNIQUE,
                                          fk_q1601 int NOT NULL REFERENCES p1601.tb_q1601);
             INSERT INTO p1601.tb_q1601 VALUES (1, gen_random_uuid());
             INSERT INTO p1601.tb_r1601 VALUES (1, gen_random_uuid(), 1);
             SELECT tviews.pg_tviews_create('p1601.tv_q1601', $q$
               SELECT q.pk_q1601, q.id, jsonb_build_object('id', q.id) AS data
               FROM p1601.tb_q1601 q $q$);
             SELECT tviews.pg_tviews_create('p1601.tv_r1601', $q$
               SELECT r.pk_r1601, r.id, r.fk_q1601, jsonb_build_object('id', r.id) AS data
               FROM p1601.tb_r1601 r $q$);
             REVOKE ALL ON SCHEMA p1601 FROM PUBLIC, p1391_reader;",
        )
        .await
        .unwrap();
    client
        .query_one(
            "SELECT r.\"references\" FROM tviews.registry r \
             WHERE r.schema = 'p1601' AND r.name = 'tv_r1601'",
            &[],
        )
        .await
        .unwrap()
        .get(0)
}

/// #1601: a write reaching a TVIEW needs no `USAGE` on the schemas of other TVIEWs. Since
/// `pg_tviews` beta.30, every write loads each TVIEW's references, resolving their tables
/// through the caller's privileges, so a writer without `USAGE` on another schema holding a
/// TVIEW with a reference column got `permission denied for schema` and its write rolled
/// back; beta.31 matches them through `pg_class`. A TVIEW without a reference column never
/// triggered it, so the schema here has one.
#[tokio::test]
async fn a_write_needs_no_usage_on_another_schema_holding_a_tview_with_a_reference() {
    let Some(url) = tviews_database_url() else {
        eprintln!("skipping #1601: TVIEWS_DATABASE_URL not set");
        return;
    };
    provision(&url).await;
    let references = provision_unusable_schema(&url).await;
    assert!(
        references.get("fk_q1601").is_some(),
        "tv_r1601 must have a reference column, or this test proves nothing: {references}"
    );
    let reader = serve_as(as_reader(&url)).await;
    let body = update_post(&reader, "no-usage-on-p1601").await;
    assert!(
        body.get("errors").is_none(),
        "a write by a role without USAGE on another TVIEW's schema failed: {body}"
    );
    assert!(
        updated(&body).contains(&("Post".to_string(), json!({ "title": "no-usage-on-p1601" }))),
        "{body}"
    );
}

/// A derived row of a type whose reads the engine filters (an inject parameter here, an
/// RLS policy alike) is not served: the database showed it to the write's connection, but
/// a read of it by this caller would pass a filter the derived re-read does not apply. Its
/// type is invalidated instead.
#[tokio::test]
async fn a_derived_row_of_an_engine_filtered_type_is_invalidated_not_served() {
    let Some(url) = tviews_database_url() else {
        return;
    };
    provision(&url).await;
    let server =
        serve_schema(url, schema_with_inject("users", &json!({ "owner": "jwt:sub" }))).await;
    let body = update_post(&server, "filtered").await;
    assert!(body.get("errors").is_none(), "{body}");
    assert_eq!(
        updated(&body),
        [("Post".to_string(), json!({ "title": "filtered" }))],
        "the author is not served: {body}"
    );
    let (_, payload) = body["data"].as_object().unwrap().iter().next().unwrap();
    assert!(
        payload["cascade"]["invalidations"].to_string().contains("users"),
        "its type is invalidated: {body}"
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
               cascade jsonb, error_detail jsonb, metadata jsonb);
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

/// A derived deletion of an engine-filtered type is not served, not even its id: the type
/// is invalidated instead.
#[tokio::test]
async fn a_derived_deletion_of_an_engine_filtered_type_is_invalidated_not_served() {
    let Some(url) = tviews_database_url() else {
        return;
    };
    provision(&url).await;
    let server =
        serve_schema(url, schema_with_inject("posts", &json!({ "owner": "jwt:sub" }))).await;
    let query = format!(
        "mutation {{ deletePost(id: \"{OTHER_POST}\", title: \"other\") {{ ... on \
         DeletePostPayload {{ cascade {{ deleted {{ id }} invalidations {{ queryName }} }} }} }} }}"
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
    assert!(!cascade.to_string().contains(OTHER_POST), "the id is not served: {body}");
    assert!(cascade["invalidations"].to_string().contains("posts"), "{body}");
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

async fn metrics(server: &Running) -> String {
    reqwest::Client::new()
        .get(format!("{}/metrics", server.base))
        .bearer_auth(METRICS_TOKEN)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap()
}

/// #1392: each TVIEW the schema reads reports its health on `/metrics`, labelled by TVIEW;
/// a schema that reads none reports no TVIEW metric at all.
#[tokio::test]
async fn metrics_report_the_health_of_each_tview_read() {
    let Some(server) = serve().await else {
        return;
    };
    run(&server, "updatePost", "measured").await;
    // A backend's update counts reach the cumulative statistics on its next flush, not at
    // commit; `hot_ratio` is NULL until they do.
    let mut rendered = metrics(&server).await;
    for _ in 0..50 {
        if rendered.contains("fraiseql_tview_hot_ratio{tview=\"p1391.tv_user\"}") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        rendered = metrics(&server).await;
    }
    for (metric, tview) in [
        ("fraiseql_tview_dead_tuples", "p1391.tv_post"),
        ("fraiseql_tview_dead_tuples", "p1391.tv_user"),
        ("fraiseql_tview_updates", "p1391.tv_user"),
        ("fraiseql_tview_hot_updates", "p1391.tv_user"),
        ("fraiseql_tview_hot_ratio", "p1391.tv_user"),
        ("fraiseql_tview_warnings", "p1391.tv_post"),
    ] {
        assert!(
            rendered.contains(&format!("{metric}{{tview=\"{tview}\"}}")),
            "{metric} {tview}: {rendered}"
        );
    }

    let url = tviews_database_url().unwrap();
    let mut unrelated = CompiledSchema::new();
    unrelated
        .types
        .push(fraiseql_core::schema::TypeDefinition::new("Other", "pg_catalog.pg_class"));
    let quiet = serve_schema(url, unrelated).await;
    assert!(
        !metrics(&quiet).await.contains("fraiseql_tview_"),
        "no TVIEW read, no TVIEW metric"
    );
}

/// The suite's document compiles with no database.
#[test]
fn the_document_loads_without_a_database() {
    let schema = schema();
    assert!(schema.find_type("UpdatePostPayload").is_some());
}

/// #1573: a derived cascade reads the rows `pg_tviews` reported with no round trip per row,
/// and plans nothing per call. `renamePosts` over 2 posts, then over 12 (one author gains
/// 10), through one counted connection: the cascade lists 10 more rows for the same cost.
#[tokio::test]
async fn a_derived_cascade_costs_the_same_whatever_its_fan_out() {
    use fraiseql_test_support::wire::CountingProxy;

    let Some(url) = tviews_database_url() else {
        return;
    };
    provision(&url).await;
    let proxy = CountingProxy::start(&url);
    let adapter = Arc::new(PostgresAdapter::with_pool_size(proxy.url(), 1).await.unwrap());
    let server = serve_adapter(proxy.url().to_string(), schema(), adapter).await;
    let cost = |title: &'static str| {
        let (server, wire) = (&server, proxy.wire());
        async move {
            run(server, "renamePosts", "warm-up").await;
            wire.reset();
            let body = run(server, "renamePosts", title).await;
            assert!(body.get("errors").is_none(), "{body}");
            (updated(&body).len(), wire.round_trips(), wire.parses())
        }
    };

    let (rows_before, round_trips_before, parses_before) = cost("two posts").await;
    let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client
        .batch_execute(&format!(
            "INSERT INTO {SCHEMA}.tb_post (id, fk_user, title) SELECT gen_random_uuid(), \
             pk_user, 'more' FROM {SCHEMA}.tb_user, generate_series(1, 10) WHERE name = 'ada'"
        ))
        .await
        .unwrap();
    let (rows_after, round_trips_after, parses_after) = cost("twelve posts").await;

    assert_eq!((rows_before, rows_after), (4, 14), "2 posts + 2 authors, then 12 + 2");
    assert_eq!(
        round_trips_before, 5,
        "BEGIN, the configuration and the call, the report, every type's read, COMMIT"
    );
    assert_eq!(
        round_trips_after, round_trips_before,
        "10 more reported rows, no more round trips"
    );
    assert_eq!((parses_before, parses_after), (0, 0), "nothing planned per call");
}

/// The statements `renamePosts` runs under `schema`, through one counted connection, and
/// its response.
async fn statements_run(url: &str, schema: CompiledSchema) -> (u64, Value) {
    use fraiseql_test_support::wire::CountingProxy;

    provision(url).await;
    let proxy = CountingProxy::start(url);
    let adapter = Arc::new(PostgresAdapter::with_pool_size(proxy.url(), 1).await.unwrap());
    let server = serve_adapter(proxy.url().to_string(), schema, adapter).await;
    run(&server, "renamePosts", "warm-up").await;
    proxy.wire().reset();
    let body = run(&server, "renamePosts", "counted").await;
    assert!(body.get("errors").is_none(), "{body}");
    (proxy.wire().executes(), body)
}

/// #1573: a reported type whose reads the engine filters is not read at all. The merge lists
/// none of its rows (they are invalidated instead), so reading them was pure cost.
#[tokio::test]
async fn a_type_the_engine_filters_is_not_read() {
    let Some(url) = tviews_database_url() else {
        return;
    };
    let (unfiltered, body) = Box::pin(statements_run(&url, schema())).await;
    assert!(
        updated(&body).iter().any(|(typename, _)| typename == "User"),
        "unfiltered, the authors are read and listed: {body}"
    );

    let filtered_schema = schema_with_inject("users", &json!({ "owner": "jwt:sub" }));
    let (filtered, body) = Box::pin(statements_run(&url, filtered_schema)).await;

    assert!(
        updated(&body).iter().all(|(typename, _)| typename == "Post"),
        "filtered, no author is listed: {body}"
    );
    assert_eq!(filtered, unfiltered - 1, "the authors' read is not run");
}

/// #1573: each derived row is attached to the entry of its own id, whatever order its view
/// returns rows in. `renamePosts` reports both authors; `v_user` returns them in the
/// opposite order.
#[tokio::test]
async fn each_derived_row_is_matched_to_its_entry_by_id() {
    let Some(server) = serve().await else {
        return;
    };
    let body = run(&server, "renamePosts", "matched").await;
    assert!(body.get("errors").is_none(), "{body}");

    let authors: Vec<(String, String)> = body["data"]["renamePosts"]["cascade"]["updated"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["entity"]["__typename"] == "User")
        .map(|e| {
            (
                e["id"].as_str().unwrap().to_string(),
                e["entity"]["name"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        authors,
        [
            (AUTHOR.to_string(), "ada".to_string()),
            ("00000000-0000-0000-0000-0000000013a2".to_string(), "bob".to_string()),
        ],
        "{body}"
    );
}

/// #1574: in a batch, each call's report says what that call changed. The report is read
/// after its own call and before the next one; read after every call, the first would list
/// both authors and the second none.
#[tokio::test]
async fn each_call_of_a_batch_reports_what_it_changed() {
    use fraiseql_core::db::traits::{DERIVED_CASCADE_KEY, DerivedCascade, WriteMode, WriteRequest};
    let Some(url) = tviews_database_url() else {
        eprintln!("skipping #1574: TVIEWS_DATABASE_URL not set");
        return;
    };
    provision(&url).await;
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    let (post_view, user_view) = (format!("{SCHEMA}.v_post"), format!("{SCHEMA}.v_user"));
    let views = [("Post", post_view.as_str()), ("User", user_view.as_str())];
    let derived = DerivedCascade::new(100, &views);
    let function = format!("{SCHEMA}.fn_update_post");
    let args = [
        [json!(POST), json!("ada's, renamed")],
        [json!(OTHER_POST), json!("bob's, renamed")],
    ];
    let requests: Vec<WriteRequest<'_>> = args
        .iter()
        .map(|a| {
            WriteRequest::new(&function, a)
                .with_derived_cascade(Some(&derived))
                .with_mode(WriteMode::DryRun)
        })
        .collect();

    let rows = fraiseql_core::db::Writer::execute_writes(&adapter, &requests, &|_, _| Ok(()))
        .await
        .unwrap();

    let authors: Vec<Vec<String>> = rows
        .iter()
        .map(|call| {
            call[0][DERIVED_CASCADE_KEY]["updated"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["__typename"] == "User")
                .map(|e| e["entity"]["name"].as_str().unwrap().to_string())
                .collect()
        })
        .collect();
    assert_eq!(authors, [["ada"], ["bob"]]);
}
