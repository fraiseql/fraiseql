//! #1576: a cached by-id read of a TVIEW is evicted by the rows a write changed, as
//! `pg_tviews` reports them, not by a sweep of its view.
//!
//! `user(id:)`, `post(id:)` and `tag(id:)` read one row of a TVIEW (`tv_u1576`, `tv_p1576`,
//! `tv_t1576`) by its `id` column. A write to user B must leave the cached read of user A
//! alone, and every write that changes A's row must evict it: A's own write, a cascade that
//! rewrites it (`tv_p1576` embeds its author's bio), a report that truncates its type, its
//! deletion, a write giving a row to an id whose miss was cached. Lists keep the sweep, and so does
//! a TVIEW with row security, whose read may depend on more than its row.
//!
//! Every test first proves its read is served from the cache: it changes the row behind the
//! server's back and reads the old value again.
//!
//! Needs `pg_tviews`: the `postgres-tviews-test` service (`docker/pg-tviews`), through
//! `TVIEWS_DATABASE_URL`. Without it the suite skips.
//!
//! **Execution engine:** `PostgreSQL` + `pg_tviews` · **Infrastructure:** `TVIEWS_DATABASE_URL`
//! · **Parallelism:** drops and recreates its own `p1576` schema → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::sync::Arc;

use fraiseql_cli::schema::{ConvertOptions, SchemaConverter, intermediate::IntermediateSchema};
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    schema::{CompiledSchema, NativeColumn},
};
use fraiseql_server::{Server, server_config::ServerConfig};
use serde_json::{Value, json};

const SCHEMA: &str = "p1576";
const USER_A: &str = "00000000-0000-0000-0000-00000000157a";
const USER_B: &str = "00000000-0000-0000-0000-00000000157b";
/// The user whose posts fill a report before any tag (`labelTagsLate`).
const USER_C: &str = "00000000-0000-0000-0000-00000000157e";
/// The default `max_updated_entities`: a write of this many posts, then of tags, fills the
/// report before `Tag`, which `pg_tviews` then names only as truncated, with none of its rows
/// listed (it reports in entity order, and `p1576` sorts before `t1576`).
const POSTS_OF_C: usize = 500;
/// A user with no row until a test writes one.
const USER_NEW: &str = "00000000-0000-0000-0000-00000000157c";
const POST_A: &str = "00000000-0000-0000-0000-0000000015a0";
const TAG: &str = "00000000-0000-0000-0000-0000000015d0";
/// One more than the default `max_updated_entities` (500): a write of every tag makes
/// `pg_tviews` truncate `Tag` out of its report.
const TAGS: usize = 501;

fn tviews_database_url() -> Option<String> {
    std::env::var("TVIEWS_DATABASE_URL").ok().filter(|u| !u.is_empty())
}

async fn client(url: &str) -> tokio_postgres::Client {
    let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client
}

/// `tv_u1576` gets row security when `rls`.
async fn provision(url: &str, rls: bool) {
    let client = client(url).await;
    client.batch_execute("CREATE EXTENSION IF NOT EXISTS pg_tviews;").await.unwrap();
    client
        .batch_execute(include_str!("../../fraiseql-cli/sql/helpers/mutation_response.sql"))
        .await
        .unwrap();
    for stmt in fraiseql_test_support::changelog::entity_change_log_provision_statements() {
        client.batch_execute(&stmt).await.unwrap_or_else(|e| panic!("{stmt}: {e}"));
    }
    client
        .batch_execute(&format!(
            "DO $$ DECLARE t text; BEGIN
               FOR t IN SELECT schema || '.' || name FROM tviews.registry
                        WHERE schema = '{SCHEMA}' LOOP
                 PERFORM tviews.pg_tviews_drop(t);
               END LOOP; END $$;
             DROP SCHEMA IF EXISTS {SCHEMA} CASCADE;"
        ))
        .await
        .unwrap();
    let row_security = if rls {
        "ALTER TABLE tv_u1576 ENABLE ROW LEVEL SECURITY;
         CREATE POLICY every_user ON tv_u1576 USING (true);"
    } else {
        ""
    };
    client
        .batch_execute(&format!(
            "CREATE SCHEMA {SCHEMA};
             SET search_path = {SCHEMA}, public;
             CREATE TYPE mutation_response AS (succeeded boolean, state_changed boolean,
               error_class text, status_detail text, http_status smallint, message text,
               entity_id uuid, entity_type text, entity jsonb, updated_fields text[],
               cascade jsonb, error_detail jsonb, metadata jsonb);
             CREATE TABLE tb_u1576 (pk_u1576 bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
               id uuid NOT NULL UNIQUE, bio text NOT NULL);
             CREATE TABLE tb_p1576 (pk_p1576 bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
               id uuid NOT NULL UNIQUE, fk_user bigint NOT NULL REFERENCES tb_u1576,
               title text NOT NULL);
             CREATE TABLE tb_t1576 (pk_t1576 bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
               id uuid NOT NULL UNIQUE, label text NOT NULL);
             INSERT INTO tb_u1576 (id, bio)
               VALUES ('{USER_A}', 'a0'), ('{USER_B}', 'b0'), ('{USER_C}', 'c0');
             INSERT INTO tb_p1576 (id, fk_user, title)
               SELECT '{POST_A}', pk_u1576, 'first' FROM tb_u1576 WHERE id = '{USER_A}';
             INSERT INTO tb_p1576 (id, fk_user, title)
               SELECT gen_random_uuid(), pk_u1576, 'c' FROM tb_u1576,
                 generate_series(1, {POSTS_OF_C}) WHERE id = '{USER_C}';
             INSERT INTO tb_t1576 (id, label) VALUES ('{TAG}', 't0');
             INSERT INTO tb_t1576 (id, label)
               SELECT gen_random_uuid(), 't0' FROM generate_series(2, {TAGS});
             SELECT tviews.pg_tviews_create('{SCHEMA}.tv_u1576', $T$
               SELECT u.pk_u1576, u.id, jsonb_build_object('id', u.id, 'bio', u.bio) AS data
               FROM {SCHEMA}.tb_u1576 u $T$, '{{\"typename\": \"User\"}}');
             SELECT tviews.pg_tviews_create('{SCHEMA}.tv_p1576', $T$
               SELECT p.pk_p1576, p.id, p.fk_user, jsonb_build_object('id', p.id,
                 'title', p.title, 'author', jsonb_build_object('id', u.id, 'bio', u.bio)) AS data
               FROM {SCHEMA}.tb_p1576 p JOIN {SCHEMA}.tb_u1576 u ON u.pk_u1576 = p.fk_user $T$,
               '{{\"typename\": \"Post\"}}');
             SELECT tviews.pg_tviews_create('{SCHEMA}.tv_t1576', $T$
               SELECT t.pk_t1576, t.id, jsonb_build_object('id', t.id, 'label', t.label) AS data
               FROM {SCHEMA}.tb_t1576 t $T$, '{{\"typename\": \"Tag\"}}');
             {row_security}
             CREATE FUNCTION fn_update_user(p_id uuid, p_bio text)
               RETURNS SETOF mutation_response LANGUAGE plpgsql AS $$
             BEGIN
               UPDATE {SCHEMA}.tb_u1576 SET bio = p_bio WHERE id = p_id;
               RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
                 (SELECT data FROM {SCHEMA}.tv_u1576 WHERE id = p_id), p_id, 'User');
             END $$;
             -- Writes every tag: more rows than the report holds.
             CREATE FUNCTION fn_label_tags(p_id uuid, p_label text)
               RETURNS SETOF mutation_response LANGUAGE plpgsql AS $$
             BEGIN
               UPDATE {SCHEMA}.tb_t1576 SET label = p_label;
               RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
                 (SELECT data FROM {SCHEMA}.tv_u1576 WHERE id = p_id), p_id, 'User');
             END $$;
             -- Rewrites C's posts, then every tag: the report is full before `Tag`.
             CREATE FUNCTION fn_label_tags_late(p_id uuid, p_label text)
               RETURNS SETOF mutation_response LANGUAGE plpgsql AS $$
             BEGIN
               UPDATE {SCHEMA}.tb_p1576 SET title = title || '.'
                 WHERE fk_user = (SELECT pk_u1576 FROM {SCHEMA}.tb_u1576 WHERE id = '{USER_C}');
               RETURN QUERY SELECT * FROM {SCHEMA}.fn_label_tags(p_id, p_label);
             END $$;
             -- Writes one tag: a type the mutation does not name, listed by row.
             CREATE FUNCTION fn_label_tag(p_id uuid, p_tag uuid, p_label text)
               RETURNS SETOF mutation_response LANGUAGE plpgsql AS $$
             BEGIN
               UPDATE {SCHEMA}.tb_t1576 SET label = p_label WHERE id = p_tag;
               RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
                 (SELECT data FROM {SCHEMA}.tv_u1576 WHERE id = p_id), p_id, 'User');
             END $$;
             CREATE FUNCTION fn_delete_tag(p_id uuid, p_tag uuid)
               RETURNS SETOF mutation_response LANGUAGE plpgsql AS $$
             BEGIN
               DELETE FROM {SCHEMA}.tb_t1576 WHERE id = p_tag;
               RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
                 (SELECT data FROM {SCHEMA}.tv_u1576 WHERE id = p_id), p_id, 'User');
             END $$;
             RESET search_path;"
        ))
        .await
        .unwrap();
}

/// The schema an SDK authors, compiled; each single-object query reads its TVIEW's native
/// `id` column, as `compile --database` maps it. Every query is cached for 300 s.
fn schema() -> CompiledSchema {
    let field = |name: &str, ty: &str| json!({ "name": name, "type": ty, "nullable": false });
    let id = || field("id", "ID");
    let ty = |name: &str, view: &str, fields: Vec<Value>| {
        json!({ "name": name, "sql_source": format!("{SCHEMA}.{view}"), "is_input": false,
                "fields": fields })
    };
    let one = |name: &str, ret: &str, view: &str| {
        json!({ "name": name, "return_type": ret, "returns_list": false, "nullable": true,
                "sql_source": format!("{SCHEMA}.{view}"), "arguments": [id()],
                "cache_ttl_seconds": 300 })
    };
    let list = |name: &str, ret: &str, view: &str| {
        json!({ "name": name, "return_type": ret, "returns_list": true, "nullable": false,
                "sql_source": format!("{SCHEMA}.{view}"), "arguments": [],
                "cache_ttl_seconds": 300 })
    };
    let mutation = |name: &str, function: &str, extra: &[(&str, &str)]| {
        let mut arguments = vec![id()];
        arguments.extend(extra.iter().map(|(n, t)| field(n, t)));
        json!({ "name": name, "return_type": "User", "operation": "update",
                "sql_source": format!("{SCHEMA}.{function}"), "arguments": arguments })
    };
    let mut derived = mutation("updateUserC", "fn_update_user", &[("bio", "String")]);
    derived["cascade"] = json!(true);
    derived["cascade_source"] = json!("pg_tviews");
    let intermediate: IntermediateSchema = serde_json::from_value(json!({
        "types": [
            ty("User", "tv_u1576", vec![id(), field("bio", "String")]),
            ty("Post", "tv_p1576", vec![id(), field("title", "String"),
                                       json!({ "name": "author", "type": "User",
                                               "nullable": false })]),
            ty("Tag", "tv_t1576", vec![id(), field("label", "String")]),
        ],
        "queries": [
            one("user", "User", "tv_u1576"),
            one("post", "Post", "tv_p1576"),
            one("tag", "Tag", "tv_t1576"),
            list("users", "User", "tv_u1576"),
            list("tags", "Tag", "tv_t1576"),
        ],
        "mutations": [
            mutation("updateUser", "fn_update_user", &[("bio", "String")]),
            derived,
            mutation("labelTags", "fn_label_tags", &[("label", "String")]),
            mutation("labelTagsLate", "fn_label_tags_late", &[("label", "String")]),
            mutation("labelTag", "fn_label_tag", &[("tag", "ID"), ("label", "String")]),
            mutation("deleteTag", "fn_delete_tag", &[("tag", "ID")]),
        ]
    }))
    .unwrap();
    let mut schema = SchemaConverter::convert_artifact(intermediate, &ConvertOptions::default())
        .unwrap()
        .schema;
    for query in schema.queries.iter_mut().filter(|q| !q.returns_list) {
        query.native_columns.insert(
            "id".to_string(),
            NativeColumn {
                pg_type:  "uuid".to_string(),
                not_null: true,
            },
        );
    }
    schema.build_indexes();
    schema
}

struct Running {
    base:      String,
    url:       String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

/// A server over a freshly provisioned schema, booted through the production constructor.
async fn serve(rls: bool) -> Option<Running> {
    let url = tviews_database_url()?;
    provision(&url, rls).await;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    let config = ServerConfig {
        database_url: url.clone(),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    assert!(config.cache_enabled, "the result cache is on by default");
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

/// The value at `pointer` under `data` of what `query` serves; `null` when absent.
async fn served(server: &Running, query: &str, pointer: &str) -> Value {
    graphql(server, query).await["data"]
        .pointer(pointer)
        .cloned()
        .unwrap_or(Value::Null)
}

/// Run `sql` behind the server's back, on a connection of its own.
async fn behind_the_back(server: &Running, sql: &str) {
    client(&server.url)
        .await
        .batch_execute(&format!("SET search_path = {SCHEMA}, public; {sql}"))
        .await
        .unwrap();
}

/// Cache `query`, prove it is served from the cache (`behind` does not show), run
/// `mutation`, and return what `query` serves afterwards.
async fn after_mutation(
    server: &Running,
    (query, pointer): (&str, &str),
    behind: &str,
    cached: &Value,
    mutation: &str,
) -> Value {
    assert_eq!(&served(server, query, pointer).await, cached, "the cached value");
    behind_the_back(server, behind).await;
    assert_eq!(
        &served(server, query, pointer).await,
        cached,
        "the read is served from the cache (vacuity check)"
    );
    graphql(server, mutation).await;
    served(server, query, pointer).await
}

fn user_bio(id: &str) -> (String, &'static str) {
    (format!("{{ user(id: \"{id}\") {{ bio }} }}"), "/user/bio")
}

fn update_user(id: &str, bio: &str) -> String {
    format!("mutation {{ updateUser(id: \"{id}\", bio: \"{bio}\") {{ id }} }}")
}

/// The issue: a write to user B no longer throws away the cached read of user A.
#[tokio::test]
async fn a_by_id_read_survives_a_write_to_another_row() {
    let Some(server) = serve(false).await else {
        eprintln!("skipping #1576: TVIEWS_DATABASE_URL not set");
        return;
    };
    let (query, pointer) = user_bio(USER_A);
    let after = after_mutation(
        &server,
        (&query, pointer),
        &format!("UPDATE tb_u1576 SET bio = 'behind' WHERE id = '{USER_A}'"),
        &json!("a0"),
        &update_user(USER_B, "b1"),
    )
    .await;
    assert_eq!(after, json!("a0"), "user A's read is still served from the cache");
}

/// A's own write evicts it.
#[tokio::test]
async fn a_by_id_read_is_evicted_by_a_write_to_its_row() {
    let Some(server) = serve(false).await else {
        return;
    };
    let (query, pointer) = user_bio(USER_A);
    let after = after_mutation(
        &server,
        (&query, pointer),
        &format!("UPDATE tb_u1576 SET bio = 'behind' WHERE id = '{USER_A}'"),
        &json!("a0"),
        &update_user(USER_A, "a1"),
    )
    .await;
    assert_eq!(after, json!("a1"));
}

/// The id spelled as a client may send it: PostgreSQL reads `…157A` as A, and so must the
/// eviction.
#[tokio::test]
async fn an_id_spelled_another_way_is_evicted_by_its_row() {
    let Some(server) = serve(false).await else {
        return;
    };
    let (query, pointer) = user_bio(&USER_A.to_uppercase());
    let after = after_mutation(
        &server,
        (&query, pointer),
        &format!("UPDATE tb_u1576 SET bio = 'behind' WHERE id = '{USER_A}'"),
        &json!("a0"),
        &update_user(USER_A, "a1"),
    )
    .await;
    assert_eq!(after, json!("a1"));
}

/// The post embeds its author's bio: the author's write makes `pg_tviews` rewrite the post's
/// row, and its report lists it.
#[tokio::test]
async fn a_by_id_read_is_evicted_when_a_cascade_rewrites_its_row() {
    let Some(server) = serve(false).await else {
        return;
    };
    let after = after_mutation(
        &server,
        (
            &format!("{{ post(id: \"{POST_A}\") {{ author {{ bio }} }} }}"),
            "/post/author/bio",
        ),
        &format!("UPDATE tb_p1576 SET title = 'behind' WHERE id = '{POST_A}'"),
        &json!("a0"),
        &update_user(USER_A, "a1"),
    )
    .await;
    assert_eq!(after, json!("a1"));
}

/// The report is full of posts before any tag: `Tag` is named truncated, none of its rows is
/// listed, and every by-id read of a tag goes.
#[tokio::test]
async fn a_by_id_read_of_a_type_the_report_truncated_is_evicted() {
    let Some(server) = serve(false).await else {
        return;
    };
    let after = after_mutation(
        &server,
        (&format!("{{ tag(id: \"{TAG}\") {{ label }} }}"), "/tag/label"),
        &format!("UPDATE tb_t1576 SET label = 'behind' WHERE id = '{TAG}'"),
        &json!("t0"),
        &format!("mutation {{ labelTagsLate(id: \"{USER_A}\", label: \"t1\") {{ id }} }}"),
    )
    .await;
    assert_eq!(after, json!("t1"));
}

/// A deleted row is reported deleted.
#[tokio::test]
async fn a_by_id_read_of_a_deleted_row_is_evicted() {
    let Some(server) = serve(false).await else {
        return;
    };
    let after = after_mutation(
        &server,
        (&format!("{{ tag(id: \"{TAG}\") {{ label }} }}"), "/tag/label"),
        &format!("UPDATE tb_t1576 SET label = 'behind' WHERE id = '{TAG}'"),
        &json!("t0"),
        &format!("mutation {{ deleteTag(id: \"{USER_A}\", tag: \"{TAG}\") {{ id }} }}"),
    )
    .await;
    assert_eq!(after, Value::Null, "the tag is gone");
}

/// A cached miss holds no row to tag: it is tagged by the id it asked for, which the report
/// of the write that gives it a row names.
#[tokio::test]
async fn a_cached_miss_is_evicted_by_a_write_to_its_id() {
    let Some(server) = serve(false).await else {
        return;
    };
    let (query, pointer) = user_bio(USER_NEW);
    let after = after_mutation(
        &server,
        (&query, pointer),
        &format!("INSERT INTO tb_u1576 (id, bio) VALUES ('{USER_NEW}', 'behind')"),
        &Value::Null,
        &update_user(USER_NEW, "n1"),
    )
    .await;
    assert_eq!(after, json!("n1"));
}

/// Every user's bio `{ users }` serves, sorted.
async fn bios(server: &Running) -> Vec<String> {
    let mut bios: Vec<String> = graphql(server, "{ users { bio } }").await["data"]["users"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["bio"].as_str().unwrap().to_string())
        .collect();
    bios.sort();
    bios
}

/// A list may gain or lose any row: the write to B still sweeps it.
#[tokio::test]
async fn a_list_is_swept_by_a_write_to_any_row() {
    let Some(server) = serve(false).await else {
        return;
    };
    assert_eq!(bios(&server).await, ["a0", "b0", "c0"]);
    behind_the_back(&server, &format!("UPDATE tb_u1576 SET bio = 'behind' WHERE id = '{USER_A}'"))
        .await;
    assert_eq!(bios(&server).await, ["a0", "b0", "c0"], "the read is served from the cache");
    graphql(&server, &update_user(USER_B, "b1")).await;
    assert_eq!(bios(&server).await, ["b1", "behind", "c0"]);
}

/// Row security may make a read depend on more than its row: such a TVIEW keeps the sweep.
#[tokio::test]
async fn a_by_id_read_of_a_tview_with_row_security_is_swept() {
    let Some(server) = serve(true).await else {
        return;
    };
    let (query, pointer) = user_bio(USER_A);
    let after = after_mutation(
        &server,
        (&query, pointer),
        &format!("UPDATE tb_u1576 SET bio = 'behind' WHERE id = '{USER_A}'"),
        &json!("a0"),
        &update_user(USER_B, "b1"),
    )
    .await;
    assert_eq!(after, json!("behind"), "swept with its view");
}

/// A cascade derived from `pg_tviews` carries the report already: it serves as the rows the
/// write changed, so a write to B keeps user A cached there too, and the cascade still lists
/// B (one report per call, not a second that would find the journal empty).
#[tokio::test]
async fn a_derived_cascades_report_keeps_the_reads_of_other_rows() {
    let Some(server) = serve(false).await else {
        return;
    };
    let (query, pointer) = user_bio(USER_A);
    assert_eq!(served(&server, &query, pointer).await, json!("a0"));
    behind_the_back(&server, &format!("UPDATE tb_u1576 SET bio = 'behind' WHERE id = '{USER_A}'"))
        .await;
    assert_eq!(served(&server, &query, pointer).await, json!("a0"), "the read is cached");
    let response = graphql(
        &server,
        &format!(
            "mutation {{ updateUserC(id: \"{USER_B}\", bio: \"b1\") {{ ... on UpdateUserCPayload \
             {{ cascade {{ updated {{ id entity {{ __typename }} }} }} }} }} }}"
        ),
    )
    .await;
    let updated = response["data"]["updateUserC"]["cascade"]["updated"].clone();
    assert!(
        updated.as_array().is_some_and(|u| u.iter().any(|e| e["id"] == json!(USER_B))),
        "the derived cascade lists B: {response}"
    );
    assert_eq!(
        served(&server, &query, pointer).await,
        json!("a0"),
        "user A's read is still served from the cache"
    );
}

/// Every distinct label `{ tags }` serves, sorted.
async fn tag_labels(server: &Running) -> Vec<String> {
    let mut labels: Vec<String> = graphql(server, "{ tags { label } }").await["data"]["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["label"].as_str().unwrap().to_string())
        .collect();
    labels.sort();
    labels.dedup();
    labels
}

/// `labelTag` names only `User`, but the report says it changed a tag: every view of `Tag`
/// is swept, its lists included.
#[tokio::test]
async fn a_write_sweeps_the_lists_of_a_type_it_changed_but_does_not_name() {
    let Some(server) = serve(false).await else {
        return;
    };
    assert_eq!(tag_labels(&server).await, ["t0"]);
    behind_the_back(&server, &format!("UPDATE tb_t1576 SET label = 'behind' WHERE id = '{TAG}'"))
        .await;
    assert_eq!(tag_labels(&server).await, ["t0"], "the read is served from the cache");
    graphql(
        &server,
        &format!(
            "mutation {{ labelTag(id: \"{USER_A}\", tag: \"{TAG}\", label: \"t1\") {{ id }} }}"
        ),
    )
    .await;
    assert_eq!(tag_labels(&server).await, ["t0", "t1"]);
}
