//! #1575: a successful write evicts every cached read of a row it changed, including a read
//! of another view that embeds that row.
//!
//! `tv_p1575` (a TVIEW) embeds its author's `bio`; `v_item` (a plain view) reads `v_owner`, and
//! `v_item_direct` reads `tb_owner` directly. Writing the user, or the owner, must evict the
//! cached reads of the views that embed them, although the mutation names only the user's
//! (owner's) own view. And a cascade derived from `pg_tviews` must evict every type it reports
//! the transaction changed, including the types it truncated or did not list.
//!
//! Every test first proves its read is served from the cache: it changes the row behind the
//! server's back and reads the old value again. Only then does it run the mutation and expect
//! the new value, so a test cannot pass because nothing was cached.
//!
//! Needs `pg_tviews`: the `postgres-tviews-test` service (`docker/pg-tviews`), through
//! `TVIEWS_DATABASE_URL`. Without it the suite skips.
//!
//! **Execution engine:** `PostgreSQL` + `pg_tviews` · **Infrastructure:** `TVIEWS_DATABASE_URL`
//! · **Parallelism:** drops and recreates its own `p1575` schema → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::sync::Arc;

use fraiseql_cli::schema::{ConvertOptions, SchemaConverter, intermediate::IntermediateSchema};
use fraiseql_core::{db::postgres::PostgresAdapter, schema::CompiledSchema};
use fraiseql_server::{Server, server_config::ServerConfig};
use serde_json::{Value, json};

const SCHEMA: &str = "p1575";
const USER: &str = "00000000-0000-0000-0000-000000001575";
const OWNER: &str = "00000000-0000-0000-0000-0000000015a1";
/// The tag `deleteTag` deletes.
const TAG: &str = "00000000-0000-0000-0000-0000000015b1";
/// One more than the derived cascade's default `max_updated_entities` (500): a write of
/// this many tags makes `pg_tviews` truncate `Tag` out of its report.
const TAGS: usize = 501;
/// The user's posts: exactly `max_updated_entities`. `pg_tviews` reports in entity order and
/// `p1575` sorts before `t1575`, so a write of every post fills the report and leaves out
/// every tag: `Tag` is then named only as truncated, with none of its rows listed.
const POST_COUNT: usize = 500;

fn tviews_database_url() -> Option<String> {
    std::env::var("TVIEWS_DATABASE_URL").ok().filter(|u| !u.is_empty())
}

async fn client(url: &str) -> tokio_postgres::Client {
    let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client
}

async fn provision(url: &str) {
    let client = client(url).await;
    client.batch_execute("CREATE EXTENSION IF NOT EXISTS pg_tviews;").await.unwrap();
    client
        .batch_execute(include_str!("../../fraiseql-cli/sql/helpers/mutation_response.sql"))
        .await
        .unwrap();
    for stmt in fraiseql_test_support::changelog::entity_change_log_provision_statements() {
        client.batch_execute(&stmt).await.unwrap_or_else(|e| panic!("{stmt}: {e}"));
    }
    // A TVIEW is dropped through pg_tviews (after the views reading it), so its
    // registration goes with it.
    client
        .batch_execute(&format!(
            "DROP VIEW IF EXISTS {SCHEMA}.v_tag_summary, {SCHEMA}.v_tag_hidden;
             DO $$ DECLARE t text; BEGIN
               FOR t IN SELECT schema || '.' || name FROM tviews.registry
                        WHERE schema = '{SCHEMA}' LOOP
                 PERFORM tviews.pg_tviews_drop(t);
               END LOOP; END $$;
             DROP SCHEMA IF EXISTS {SCHEMA} CASCADE;"
        ))
        .await
        .unwrap();
    client
        .batch_execute(&format!(
            "CREATE SCHEMA {SCHEMA};
             SET search_path = {SCHEMA}, public;
             CREATE TYPE mutation_response AS (succeeded boolean, state_changed boolean,
               error_class text, status_detail text, http_status smallint, message text,
               entity_id uuid, entity_type text, entity jsonb, updated_fields text[],
               cascade jsonb, error_detail jsonb, metadata jsonb);
             CREATE TABLE tb_u1575 (pk_u1575 bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
               id uuid NOT NULL UNIQUE, name text NOT NULL, bio text NOT NULL);
             CREATE TABLE tb_p1575 (pk_p1575 bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
               id uuid NOT NULL UNIQUE, fk_user bigint NOT NULL REFERENCES tb_u1575,
               title text NOT NULL);
             CREATE TABLE tb_t1575 (pk_t1575 bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
               id uuid NOT NULL UNIQUE, label text NOT NULL);
             CREATE TABLE tb_owner (pk_owner bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
               id uuid NOT NULL UNIQUE, name text NOT NULL);
             CREATE TABLE tb_item (pk_item bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
               id uuid NOT NULL UNIQUE, fk_owner bigint NOT NULL REFERENCES tb_owner,
               title text NOT NULL);
             INSERT INTO tb_u1575 (id, name, bio) VALUES ('{USER}', 'ada', 'v0');
             INSERT INTO tb_p1575 (id, fk_user, title)
               SELECT gen_random_uuid(), pk_u1575, 'post ' || n
               FROM tb_u1575, generate_series(1, {POST_COUNT}) n;
             INSERT INTO tb_t1575 (id, label) VALUES ('{TAG}', 'v0');
             INSERT INTO tb_t1575 (id, label)
               SELECT gen_random_uuid(), 'v0' FROM generate_series(2, {TAGS});
             INSERT INTO tb_owner (id, name) VALUES ('{OWNER}', 'v0');
             INSERT INTO tb_item (id, fk_owner, title)
               SELECT gen_random_uuid(), pk_owner, 'item' FROM tb_owner;
             SELECT tviews.pg_tviews_create('{SCHEMA}.tv_u1575', $T$
               SELECT u.pk_u1575, u.id, jsonb_build_object('id', u.id, 'name', u.name,
                 'bio', u.bio) AS data FROM {SCHEMA}.tb_u1575 u $T$);
             SELECT tviews.pg_tviews_create('{SCHEMA}.tv_p1575', $T$
               SELECT p.pk_p1575, p.id, p.fk_user, jsonb_build_object('id', p.id,
                 'title', p.title, 'author', jsonb_build_object('id', u.id, 'name', u.name,
                 'bio', u.bio)) AS data
               FROM {SCHEMA}.tb_p1575 p JOIN {SCHEMA}.tb_u1575 u ON u.pk_u1575 = p.fk_user $T$);
             SELECT tviews.pg_tviews_create('{SCHEMA}.tv_t1575', $T$
               SELECT t.pk_t1575, t.id, jsonb_build_object('id', t.id, 'label', t.label) AS data
               FROM {SCHEMA}.tb_t1575 t $T$);
             -- pg_tviews entity names are database-wide: unique ones, typed back.
             SELECT tviews.pg_tviews_set_typename('u1575', 'User');
             SELECT tviews.pg_tviews_set_typename('p1575', 'Post');
             SELECT tviews.pg_tviews_set_typename('t1575', 'Tag');
             -- Reads the tags only through their TVIEW, grouped: not a `Tag` read.
             CREATE VIEW v_tag_summary AS SELECT md5(t.data->>'label')::uuid AS id,
               jsonb_build_object('id', md5(t.data->>'label')::uuid, 'label', t.data->>'label',
                 'n', count(*)) AS data
               FROM tv_t1575 t GROUP BY t.data->>'label';
             -- A view of the tags that returns none: a derived re-read finds no row.
             CREATE VIEW v_tag_hidden AS SELECT t.id, t.data FROM tv_t1575 t WHERE false;
             CREATE VIEW v_owner AS SELECT o.id, jsonb_build_object('id', o.id,
               'name', o.name) AS data FROM tb_owner o;
             -- Reads the owner through its view.
             CREATE VIEW v_item AS SELECT i.id, jsonb_build_object('id', i.id,
               'title', i.title, 'owner', o.data) AS data
               FROM tb_item i JOIN tb_owner b ON b.pk_owner = i.fk_owner
               JOIN v_owner o ON o.id = b.id;
             -- Reads the owner's table directly.
             CREATE VIEW v_item_direct AS SELECT i.id, jsonb_build_object('id', i.id,
               'title', i.title, 'owner', jsonb_build_object('id', o.id, 'name', o.name)) AS data
               FROM tb_item i JOIN tb_owner o ON o.pk_owner = i.fk_owner;
             CREATE FUNCTION fn_update_user(p_id uuid, p_bio text)
               RETURNS SETOF mutation_response LANGUAGE plpgsql AS $$
             BEGIN
               UPDATE {SCHEMA}.tb_u1575 SET bio = p_bio WHERE id = p_id;
               RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
                 (SELECT data FROM {SCHEMA}.tv_u1575 WHERE id = p_id), p_id, 'User');
             END $$;
             CREATE FUNCTION fn_rename_owner(p_id uuid, p_name text)
               RETURNS SETOF mutation_response LANGUAGE plpgsql AS $$
             BEGIN
               UPDATE {SCHEMA}.tb_owner SET name = p_name WHERE id = p_id;
               RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
                 (SELECT data FROM {SCHEMA}.v_owner WHERE id = p_id), p_id, 'Owner');
             END $$;
             -- Writes `p_n` tags besides the user: a type unrelated to the user's views.
             CREATE FUNCTION fn_label_tags(p_id uuid, p_label text, p_n int)
               RETURNS SETOF mutation_response LANGUAGE plpgsql AS $$
             BEGIN
               UPDATE {SCHEMA}.tb_u1575 SET name = name WHERE id = p_id;
               UPDATE {SCHEMA}.tb_t1575 SET label = p_label
                 WHERE pk_t1575 IN (SELECT pk_t1575 FROM {SCHEMA}.tb_t1575 ORDER BY pk_t1575 LIMIT p_n);
               RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
                 (SELECT data FROM {SCHEMA}.tv_u1575 WHERE id = p_id), p_id, 'User');
             END $$;
             -- `fn_label_tags`, after rewriting every post: the report is full before `Tag`.
             CREATE FUNCTION fn_label_tags_late(p_id uuid, p_label text, p_n int)
               RETURNS SETOF mutation_response LANGUAGE plpgsql AS $$
             BEGIN
               UPDATE {SCHEMA}.tb_p1575 SET title = title || '.';
               RETURN QUERY SELECT * FROM {SCHEMA}.fn_label_tags(p_id, p_label, p_n);
             END $$;
             CREATE FUNCTION fn_delete_tag(p_id uuid, p_tag uuid)
               RETURNS SETOF mutation_response LANGUAGE plpgsql AS $$
             BEGIN
               DELETE FROM {SCHEMA}.tb_t1575 WHERE id = p_tag;
               RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
                 (SELECT data FROM {SCHEMA}.tv_u1575 WHERE id = p_id), p_id, 'User');
             END $$;
             -- Writes every tag and says so only through its own cascade's invalidation hint.
             CREATE FUNCTION fn_label_tags_hinted(p_id uuid, p_label text, p_n int)
               RETURNS SETOF mutation_response LANGUAGE plpgsql AS $$
             BEGIN
               UPDATE {SCHEMA}.tb_t1575 SET label = p_label;
               RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
                 (SELECT data FROM {SCHEMA}.tv_u1575 WHERE id = p_id), p_id, 'User',
                 p_cascade => jsonb_build_object('invalidations', jsonb_build_array(
                   jsonb_build_object('queryName', 'tags', 'strategy', 'INVALIDATE',
                                      'scope', 'PREFIX'))));
             END $$;
             RESET search_path;"
        ))
        .await
        .unwrap();
}

/// How [`schema`] varies between tests.
#[derive(Clone, Copy, Default)]
struct Shape {
    /// `tagsMine` returns tags through an inject parameter, which makes `Tag` a type whose
    /// reads the engine filters: a derived cascade then lists none of its rows.
    filtered:      bool,
    /// No root query returns `Tag`, so a derived cascade cannot hint one for it.
    no_tags_query: bool,
    /// `Tag` reads from a view that returns no rows: a derived cascade cannot read the
    /// tags it reports, so it lists none of them.
    hidden_tags:   bool,
    /// `usersBare` reads `tv_u1575` unqualified, through the connection's `search_path`,
    /// while `User` names it `p1575.tv_u1575`: two names for one relation.
    bare_users:    bool,
}

/// The schema an SDK authors, compiled. Every query is cached for 300 s.
fn schema(shape: Shape) -> CompiledSchema {
    let field = |name: &str, ty: &str| json!({ "name": name, "type": ty, "nullable": false });
    let id = || field("id", "ID");
    let ty = |name: &str, view: &str, fields: Vec<Value>| {
        json!({ "name": name, "sql_source": format!("{SCHEMA}.{view}"), "is_input": false,
                "fields": fields })
    };
    let list = |name: &str, ret: &str, view: &str| {
        json!({ "name": name, "return_type": ret, "returns_list": true, "nullable": false,
                "sql_source": format!("{SCHEMA}.{view}"), "arguments": [],
                "cache_ttl_seconds": 300 })
    };
    let mut queries = vec![
        list("users", "User", "tv_u1575"),
        list("posts", "Post", "tv_p1575"),
        list("items", "Item", "v_item"),
        list("itemsDirect", "Item", "v_item_direct"),
        list("tagSummary", "TagSummary", "v_tag_summary"),
    ];
    if shape.bare_users {
        let mut bare = list("usersBare", "User", "tv_u1575");
        bare["sql_source"] = json!("tv_u1575");
        queries.push(bare);
    }
    if !shape.no_tags_query {
        queries.push(list("tags", "Tag", "tv_t1575"));
    }
    if shape.filtered {
        let mut mine = list("tagsMine", "Tag", "tv_t1575");
        mine["inject_params"] = json!({ "owner": "jwt:sub" });
        queries.push(mine);
    }
    let mutation =
        |name: &str, ret: &str, function: &str, extra: &[(&str, &str)], cascade: &str| {
            let mut arguments = vec![field("id", "ID")];
            arguments.extend(extra.iter().map(|(n, t)| field(n, t)));
            let mut m = json!({ "name": name, "return_type": ret, "operation": "update",
                            "sql_source": format!("{SCHEMA}.{function}"),
                            "arguments": arguments });
            if !cascade.is_empty() {
                m["cascade"] = json!(true);
                m["cascade_source"] = json!(cascade);
            }
            m
        };
    let intermediate: IntermediateSchema = serde_json::from_value(json!({
        "types": [
            ty("User", "tv_u1575", vec![id(), field("name", "String"), field("bio", "String")]),
            ty("Post", "tv_p1575", vec![id(), field("title", "String"),
                                       json!({ "name": "author", "type": "User",
                                               "nullable": false })]),
            ty("Tag", if shape.hidden_tags { "v_tag_hidden" } else { "tv_t1575" },
               vec![id(), field("label", "String")]),
            ty("TagSummary", "v_tag_summary", vec![id(), field("label", "String"),
                                                   field("n", "Int")]),
            ty("Owner", "v_owner", vec![id(), field("name", "String")]),
            ty("Item", "v_item", vec![id(), field("title", "String"),
                                      json!({ "name": "owner", "type": "Owner",
                                              "nullable": false })]),
        ],
        "queries": queries,
        "mutations": [
            mutation("updateUser", "User", "fn_update_user", &[("bio", "String")], ""),
            mutation("renameOwner", "Owner", "fn_rename_owner", &[("name", "String")], ""),
            mutation("labelTags", "User", "fn_label_tags",
                     &[("label", "String"), ("n", "Int")], "pg_tviews"),
            mutation("labelTagsLate", "User", "fn_label_tags_late",
                     &[("label", "String"), ("n", "Int")], "pg_tviews"),
            mutation("deleteTag", "User", "fn_delete_tag", &[("tag", "ID")], "pg_tviews"),
            mutation("labelTagsHinted", "User", "fn_label_tags_hinted",
                     &[("label", "String"), ("n", "Int")], "function"),
        ]
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
    url:       String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

/// A server over a freshly provisioned schema, booted through the production constructor
/// (so through the one cache seam every constructor shares).
async fn serve(shape: Shape) -> Option<Running> {
    let url = tviews_database_url()?;
    provision(&url).await;
    // The server resolves unqualified names in this suite's schema first.
    let url = format!(
        "{url}{}options=-csearch_path%3D{SCHEMA}%2Cpublic",
        if url.contains('?') { "&" } else { "?" }
    );
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    let config = ServerConfig {
        database_url: url.clone(),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    assert!(config.cache_enabled, "the result cache is on by default");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = Box::pin(Server::new(config, schema(shape), adapter, None)).await.unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    Some(Running {
        base: format!("http://127.0.0.1:{port}"),
        url,
        _shutdown: tx,
    })
}

async fn graphql(server: &Running, query: &str) -> Value {
    let body: Value = reqwest::Client::new()
        .post(format!("{}/graphql", server.base))
        .json(&json!({ "query": query }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(body.get("errors").is_none(), "{query}: {body}");
    body
}

/// Every distinct string at `pointer` in the rows `query` serves, sorted.
async fn served(server: &Running, query: &str, list: &str, pointer: &str) -> Vec<String> {
    let body = graphql(server, query).await;
    let mut values: Vec<String> = body["data"][list]
        .as_array()
        .unwrap_or_else(|| panic!("{body}"))
        .iter()
        .map(|row| row.pointer(pointer).and_then(Value::as_str).unwrap_or_default().to_string())
        .collect();
    values.sort();
    values.dedup();
    values
}

/// Run `sql` behind the server's back, on a connection of its own.
async fn behind_the_back(server: &Running, sql: &str) {
    client(&server.url)
        .await
        .batch_execute(&format!("SET search_path = {SCHEMA}, public; {sql}"))
        .await
        .unwrap();
}

/// Cache `query`, prove it is served from the cache (a change behind the server's back does
/// not show), run `mutation`, and return what `query` serves afterwards, with the mutation's
/// response.
async fn after_mutation(
    server: &Running,
    (query, list, pointer): (&str, &str, &str),
    behind: &str,
    expect_cached: &[&str],
    mutation: &str,
) -> (Vec<String>, Value) {
    let before = served(server, query, list, pointer).await;
    behind_the_back(server, behind).await;
    assert_eq!(
        served(server, query, list, pointer).await,
        before,
        "the read is served from the cache (vacuity check)"
    );
    assert_eq!(before, expect_cached, "the cached value");
    let response = graphql(server, mutation).await;
    (served(server, query, list, pointer).await, response)
}

/// The query names the mutation's cascade hints as invalidated.
fn hinted(response: &Value) -> Vec<String> {
    let (_, payload) = response["data"].as_object().unwrap().iter().next().unwrap();
    payload["cascade"]["invalidations"]
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter_map(|h| h["queryName"].as_str().map(str::to_string))
        .collect()
}

const POSTS: (&str, &str, &str) = ("{ posts { author { bio } } }", "posts", "/author/bio");
const TAG_LABELS: (&str, &str, &str) = ("{ tags { label } }", "tags", "/label");

// ---------------------------------------------------------------------------------------
// The embedding graph: a write reaches the views that embed what it wrote.
// ---------------------------------------------------------------------------------------

/// The issue as filed: `updateUser` returns `User`, so it sweeps `tv_u1575`; `tv_p1575` embeds
/// the author's bio, and `pg_tviews` refreshed it, but its cached read survived.
#[tokio::test]
async fn a_plain_write_evicts_the_tviews_that_embed_its_row() {
    let Some(server) = serve(Shape::default()).await else {
        eprintln!("skipping #1575: TVIEWS_DATABASE_URL not set");
        return;
    };
    let (bios, _) = after_mutation(
        &server,
        POSTS,
        "UPDATE tb_u1575 SET bio = 'behind'",
        &["v0"],
        &format!("mutation {{ updateUser(id: \"{USER}\", bio: \"v1\") {{ id }} }}"),
    )
    .await;
    assert_eq!(bios, ["v1"], "the posts embedding the author serve the new bio");
}

/// A plain view reading the owner through the owner's view.
#[tokio::test]
async fn a_plain_write_evicts_a_view_reading_its_view() {
    let Some(server) = serve(Shape::default()).await else {
        return;
    };
    let (names, _) = after_mutation(
        &server,
        ("{ items { owner { name } } }", "items", "/owner/name"),
        "UPDATE tb_owner SET name = 'behind'",
        &["v0"],
        &format!("mutation {{ renameOwner(id: \"{OWNER}\", name: \"v1\") {{ id }} }}"),
    )
    .await;
    assert_eq!(names, ["v1"]);
}

/// A plain view reading the owner's table directly, not through the owner's view.
#[tokio::test]
async fn a_plain_write_evicts_a_view_reading_its_table() {
    let Some(server) = serve(Shape::default()).await else {
        return;
    };
    let (names, _) = after_mutation(
        &server,
        ("{ itemsDirect { owner { name } } }", "itemsDirect", "/owner/name"),
        "UPDATE tb_owner SET name = 'behind'",
        &["v0"],
        &format!("mutation {{ renameOwner(id: \"{OWNER}\", name: \"v1\") {{ id }} }}"),
    )
    .await;
    assert_eq!(names, ["v1"]);
}

// ---------------------------------------------------------------------------------------
// The derived cascade: every type pg_tviews reports is swept, listed or not.
// ---------------------------------------------------------------------------------------

/// `pg_tviews` truncated `Tag` out of its report (501 rows > 500): the cascade lists none of
/// its rows and hints its queries instead. The tags' cached read must go.
#[tokio::test]
async fn a_type_the_report_truncated_is_evicted() {
    let Some(server) = serve(Shape::default()).await else {
        return;
    };
    let (labels, response) = after_mutation(
        &server,
        TAG_LABELS,
        "UPDATE tb_t1575 SET label = 'behind'",
        &["v0"],
        &format!(
            "mutation {{ labelTags(id: \"{USER}\", label: \"v1\", n: {TAGS}) {{ \
             ... on LabelTagsPayload {{ cascade {{ invalidations {{ queryName }} }} }} }} }}"
        ),
    )
    .await;
    assert!(
        hinted(&response).contains(&"tags".to_string()),
        "truncated, so hinted: {response}"
    );
    assert_eq!(labels, ["v1"]);
}

/// `Tag` is engine-filtered (`tagsMine` injects a parameter), so a derived cascade lists
/// none of its updated rows: the tags' cached read must go anyway.
#[tokio::test]
async fn an_engine_filtered_updated_type_is_evicted() {
    let Some(server) = serve(Shape {
        filtered: true,
        ..Shape::default()
    })
    .await
    else {
        return;
    };
    let (labels, response) = after_mutation(
        &server,
        TAG_LABELS,
        "UPDATE tb_t1575 SET label = 'behind'",
        &["v0"],
        &format!(
            "mutation {{ labelTags(id: \"{USER}\", label: \"v1\", n: 1) {{ \
             ... on LabelTagsPayload {{ cascade {{ invalidations {{ queryName }} }} }} }} }}"
        ),
    )
    .await;
    assert!(
        hinted(&response).contains(&"tags".to_string()),
        "filtered, so hinted: {response}"
    );
    // One tag relabelled, the rest as they were behind the server's back.
    assert_eq!(labels, ["behind", "v1"]);
}

/// The same for a deleted row of an engine-filtered type: not even its id is listed.
#[tokio::test]
async fn an_engine_filtered_deleted_type_is_evicted() {
    let Some(server) = serve(Shape {
        filtered: true,
        ..Shape::default()
    })
    .await
    else {
        return;
    };
    let tag_ids = ("{ tags { id } }", "tags", "/id");
    let before = served(&server, tag_ids.0, tag_ids.1, tag_ids.2).await;
    assert!(before.contains(&TAG.to_string()), "the tag is served");
    assert_eq!(before.len(), TAGS);
    behind_the_back(&server, "INSERT INTO tb_t1575 (id, label) VALUES (gen_random_uuid(), 'x')")
        .await;
    assert_eq!(
        served(&server, tag_ids.0, tag_ids.1, tag_ids.2).await,
        before,
        "the read is served from the cache (vacuity check)"
    );
    let response = graphql(
        &server,
        &format!(
            "mutation {{ deleteTag(id: \"{USER}\", tag: \"{TAG}\") {{ \
             ... on DeleteTagPayload {{ cascade {{ invalidations {{ queryName }} }} }} }} }}"
        ),
    )
    .await;
    assert!(
        hinted(&response).contains(&"tags".to_string()),
        "filtered, so hinted: {response}"
    );
    let ids = served(&server, tag_ids.0, tag_ids.1, tag_ids.2).await;
    assert!(!ids.contains(&TAG.to_string()), "the deleted tag is no longer served");
    assert_eq!(ids.len(), TAGS, "less the deleted tag, plus the one inserted behind");
}

/// A function's own cascade can name a query to invalidate; the cache honours it.
#[tokio::test]
async fn a_functions_own_invalidation_hint_is_evicted() {
    let Some(server) = serve(Shape::default()).await else {
        return;
    };
    let (labels, response) = after_mutation(
        &server,
        TAG_LABELS,
        "UPDATE tb_t1575 SET label = 'behind'",
        &["v0"],
        &format!(
            "mutation {{ labelTagsHinted(id: \"{USER}\", label: \"v1\", n: 0) {{ \
             ... on LabelTagsHintedPayload {{ cascade {{ invalidations {{ queryName }} }} }} }} }}"
        ),
    )
    .await;
    assert_eq!(hinted(&response), ["tags"], "{response}");
    assert_eq!(labels, ["v1"]);
}

/// `pg_tviews` filled its report with posts and left every tag out: `Tag` is named only as
/// truncated, no root query returns it, so the cascade does not even hint it; `tagSummary`
/// reads its TVIEW through a plain view. Its cached read must go.
#[tokio::test]
async fn a_truncated_type_no_query_returns_is_evicted_through_its_readers() {
    let Some(server) = serve(Shape {
        no_tags_query: true,
        ..Shape::default()
    })
    .await
    else {
        return;
    };
    let (labels, response) = after_mutation(
        &server,
        ("{ tagSummary { label } }", "tagSummary", "/label"),
        "UPDATE tb_t1575 SET label = 'behind'",
        &["v0"],
        &format!(
            "mutation {{ labelTagsLate(id: \"{USER}\", label: \"v1\", n: 1) {{ \
             ... on LabelTagsLatePayload {{ cascade {{ invalidations {{ queryName }} }} }} }} }}"
        ),
    )
    .await;
    assert!(!hinted(&response).contains(&"tags".to_string()), "nothing to hint: {response}");
    assert_eq!(labels, ["behind", "v1"]);
}

/// `pg_tviews` reports the tag it updated, but `Tag`'s view returns no row for it, so the
/// cascade lists none, and no root query returns `Tag` to hint. `tagSummary` reads the
/// TVIEW behind that view: its cached read must go.
#[tokio::test]
async fn an_unreadable_type_no_query_returns_is_evicted_through_its_readers() {
    let Some(server) = serve(Shape {
        no_tags_query: true,
        hidden_tags: true,
        ..Shape::default()
    })
    .await
    else {
        return;
    };
    let (labels, response) = after_mutation(
        &server,
        ("{ tagSummary { label } }", "tagSummary", "/label"),
        "UPDATE tb_t1575 SET label = 'behind'",
        &["v0"],
        &format!(
            "mutation {{ labelTags(id: \"{USER}\", label: \"v1\", n: 1) {{ \
             ... on LabelTagsPayload {{ cascade {{ invalidations {{ queryName }} }} }} }} }}"
        ),
    )
    .await;
    assert!(hinted(&response).is_empty(), "nothing to hint: {response}");
    assert_eq!(labels, ["behind", "v1"]);
}

/// Two names for one relation: `usersBare` is cached under `tv_u1575`, and `updateUser`
/// sweeps `User`'s `p1575.tv_u1575`. The cached read must go.
#[tokio::test]
async fn a_write_evicts_a_read_of_the_same_relation_under_another_name() {
    let Some(server) = serve(Shape {
        bare_users: true,
        ..Shape::default()
    })
    .await
    else {
        return;
    };
    let (bios, _) = after_mutation(
        &server,
        ("{ usersBare { bio } }", "usersBare", "/bio"),
        "UPDATE tb_u1575 SET bio = 'behind'",
        &["v0"],
        &format!("mutation {{ updateUser(id: \"{USER}\", bio: \"v1\") {{ id }} }}"),
    )
    .await;
    assert_eq!(bios, ["v1"]);
}
