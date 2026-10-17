//! #1589: a server hears `pg_tviews`' change feed, so a write it did not run evicts what it
//! made stale, and a composed list is evicted by the collections its rows belong to.
//!
//! `tv_p1589` (posts) and `tv_c1589` (comments, `fk_p1589 REFERENCES tb_p1589`) are TVIEWs
//! of a database publishing rows (`pg_tviews.publish_changes = 'rows'`). Every write here
//! goes to a base table behind the server's back, as another application would: only the
//! feed can tell the server about it.
//!
//! Every test first proves its read is served from the cache: it edits the TVIEW row
//! directly (which `pg_tviews` does not publish) and reads the old value again. Only then
//! does it write a base table and wait for the new value, so a test cannot pass because
//! nothing was cached.
//!
//! Publishing is a per-database setting, and it makes every server over that database hear
//! writes made behind its back: the suite runs in a database of its own ([`DATABASE`]),
//! created on the `TVIEWS_DATABASE_URL` cluster, so no other suite sharing that database
//! sees it. Needs a `pg_tviews` whose registry names types (`tviews.registry.typename`) and a role
//! that can create a database and set the superuser setting. Without `TVIEWS_DATABASE_URL` the
//! suite skips; on a `pg_tviews` without `typename` it fails.
//!
//! **Execution engine:** `PostgreSQL` + `pg_tviews` · **Infrastructure:** `TVIEWS_DATABASE_URL`
//! · **Parallelism:** drops and recreates its own `p1589` schema in its own database → run
//! `--test-threads=1`.
#![cfg(feature = "rest")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::{sync::Arc, time::Duration};

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::{db::postgres::PostgresAdapter, schema::CompiledSchema};
use fraiseql_server::{Server, server_config::ServerConfig};
use fraiseql_test_support::wire::CountingProxy;
use serde_json::Value;
use tempfile::TempDir;

const SCHEMA: &str = "p1589";
/// The suite's own database, on the `TVIEWS_DATABASE_URL` cluster.
const DATABASE: &str = "fraiseql_p1589";
/// Post A and post B.
const POST_A: &str = "00000000-0000-0000-0000-00000000158a";
const POST_B: &str = "00000000-0000-0000-0000-00000000158b";
/// A's one comment.
const COMMENT_A: &str = "00000000-0000-0000-0000-0000000015ca";

fn tviews_database_url() -> Option<String> {
    std::env::var("TVIEWS_DATABASE_URL").ok().filter(|u| !u.is_empty())
}

async fn client(url: &str) -> tokio_postgres::Client {
    let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client
}

/// The URL of [`DATABASE`], created (publishing rows) on `url`'s cluster if it is not there.
async fn own_database(url: &str) -> String {
    let admin = client(url).await;
    let exists: bool = admin
        .query_one("SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = $1)", &[&DATABASE])
        .await
        .unwrap()
        .get(0);
    if !exists {
        admin.batch_execute(&format!("CREATE DATABASE {DATABASE}")).await.unwrap();
    }
    admin
        .batch_execute(&format!("ALTER DATABASE {DATABASE} SET pg_tviews.publish_changes = 'rows'"))
        .await
        .unwrap();
    let mut own = url::Url::parse(url).unwrap();
    own.set_path(&format!("/{DATABASE}"));
    own.to_string()
}

/// The database's `pg_tviews` names collections (`tviews.registry.typename`). One older than
/// the pinned image (`docker/pg-tviews`) would pass every test of the suite without running
/// one, so the suite fails on it instead.
async fn require_typename(url: &str) {
    let client = client(url).await;
    client.batch_execute("CREATE EXTENSION IF NOT EXISTS pg_tviews;").await.unwrap();
    let has: bool = client
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM pg_attribute WHERE attrelid = to_regclass('tviews.registry') \
             AND attname = 'typename' AND NOT attisdropped)",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        has,
        "pg_tviews in TVIEWS_DATABASE_URL has no tviews.registry.typename: run the pinned image \
         (docker/pg-tviews)"
    );
}

async fn provision(url: &str) {
    let client = client(url).await;
    client
        .batch_execute(include_str!("../../fraiseql-cli/sql/helpers/mutation_response.sql"))
        .await
        .unwrap();
    for stmt in fraiseql_test_support::changelog::entity_change_log_provision_statements() {
        client.batch_execute(&stmt).await.unwrap_or_else(|e| panic!("{stmt}: {e}"));
    }
    client
        .batch_execute(&format!(
            "CREATE EXTENSION IF NOT EXISTS pg_tviews;
             DO $$ DECLARE t text; BEGIN
               FOR t IN SELECT schema || '.' || name FROM tviews.registry
                        WHERE schema = '{SCHEMA}' ORDER BY entity LOOP
                 PERFORM tviews.pg_tviews_drop(t);
               END LOOP; END $$;
             DROP SCHEMA IF EXISTS {SCHEMA} CASCADE;
             CREATE SCHEMA {SCHEMA};
             CREATE TABLE {SCHEMA}.tb_p1589 (pk_p1589 int PRIMARY KEY, id uuid NOT NULL UNIQUE,
                                             title text NOT NULL);
             CREATE TABLE {SCHEMA}.tb_c1589 (pk_c1589 int PRIMARY KEY, id uuid NOT NULL UNIQUE,
                                             fk_p1589 int NOT NULL REFERENCES {SCHEMA}.tb_p1589,
                                             body text NOT NULL);
             INSERT INTO {SCHEMA}.tb_p1589 VALUES (1, '{POST_A}', 'A'), (2, '{POST_B}', 'B');
             INSERT INTO {SCHEMA}.tb_c1589 VALUES (1, '{COMMENT_A}', 1, 'first');
             SELECT tviews.pg_tviews_create('{SCHEMA}.tv_p1589', $q$
               SELECT p.pk_p1589, p.id,
                      jsonb_build_object('id', p.id, 'pk_p1589', p.pk_p1589, 'title', p.title) AS data
               FROM {SCHEMA}.tb_p1589 p $q$);
             SELECT tviews.pg_tviews_create('{SCHEMA}.tv_c1589', $q$
               SELECT c.pk_c1589, c.id, c.fk_p1589,
                      jsonb_build_object('id', c.id, 'fk_p1589', c.fk_p1589, 'body', c.body) AS data
               FROM {SCHEMA}.tb_c1589 c JOIN {SCHEMA}.tb_p1589 p ON p.pk_p1589 = c.fk_p1589 $q$);
             CREATE TYPE {SCHEMA}.mutation_response AS (succeeded boolean,
               state_changed boolean, error_class text, status_detail text, http_status smallint,
               message text, entity_id uuid, entity_type text, entity jsonb,
               updated_fields text[], cascade jsonb, error_detail jsonb, metadata jsonb);
             -- `p_count` comments on the post `p_id`, through FraiseQL (#1603). With
             -- `p_publish` off the transaction publishes nothing, so pg_tviews captures no
             -- collections for its report.
             CREATE FUNCTION {SCHEMA}.fn_add_comments(p_id uuid, p_body text, p_count int,
                                                       p_publish boolean)
               RETURNS SETOF {SCHEMA}.mutation_response LANGUAGE plpgsql AS $$
             DECLARE v_id uuid;
             BEGIN
               IF NOT p_publish THEN
                 PERFORM set_config('pg_tviews.publish_changes', 'off', true);
               END IF;
               INSERT INTO {SCHEMA}.tb_c1589
                 SELECT m.top + i, gen_random_uuid(), p.pk_p1589, p_body
                 FROM {SCHEMA}.tb_p1589 p,
                      (SELECT coalesce(max(pk_c1589), 0) AS top FROM {SCHEMA}.tb_c1589) m,
                      generate_series(1, p_count) i
                 WHERE p.id = p_id;
               SELECT id INTO v_id FROM {SCHEMA}.tb_c1589 ORDER BY pk_c1589 DESC LIMIT 1;
               RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
                 (SELECT data FROM {SCHEMA}.tv_c1589 WHERE id = v_id), v_id, 'Comment');
             END $$;"
        ))
        .await
        .unwrap();
}

fn fraiseql_toml() -> String {
    format!(
        r#"
[schema]
name = "feed-1589"
version = "1.0.0"
database_target = "postgresql"

[rest]
enabled = true

[types.Post]
sql_source = "{SCHEMA}.tv_p1589"
fields.id = {{ type = "ID" }}
fields.pk_p1589 = {{ type = "Int" }}
fields.title = {{ type = "String" }}

[types.Post.relationships.comments]
target_type = "Comment"
cardinality = "OneToMany"
foreign_key = "fk_p1589"
referenced_key = "pk_p1589"

[types.Comment]
sql_source = "{SCHEMA}.tv_c1589"
fields.id = {{ type = "ID" }}
fields.fk_p1589 = {{ type = "Int" }}
fields.body = {{ type = "String" }}

[queries.posts]
return_type = "Post"
return_array = true
sql_source = "{SCHEMA}.tv_p1589"

[queries.comments]
return_type = "Comment"
return_array = true
sql_source = "{SCHEMA}.tv_c1589"

[queries.post]
return_type = "Post"
return_array = false
sql_source = "{SCHEMA}.tv_p1589"

[[queries.post.args]]
name = "id"
type = "ID"
required = true

[mutations.addComments]
return_type = "Comment"
operation = "INSERT"
sql_source = "{SCHEMA}.fn_add_comments"
args = [
  {{ name = "id", type = "ID", required = true }},
  {{ name = "body", type = "String", required = true }},
  {{ name = "count", type = "Int", required = true }},
  {{ name = "publish", type = "Boolean", required = true }},
]
"#
    )
}

/// The document above, compiled against the database at `url` (which proves `Post.comments`
/// joins on `fk_p1589 = pk_p1589`, #1587), every query cached for 300 s.
async fn schema(url: &str) -> (CompiledSchema, TempDir) {
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
    for query in &mut schema.queries {
        query.cache_ttl_seconds = Some(300);
    }
    schema.build_indexes();
    (schema, temp_dir)
}

struct Running {
    base:      String,
    url:       String,
    http:      reqwest::Client,
    _schema:   TempDir,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

async fn serve() -> Option<Running> {
    serve_with_feed_through(None).await
}

/// [`serve`], its feed's connection (`database_url`) through `proxy` when given; its pool
/// connects directly.
async fn serve_with_feed_through(proxy: Option<&CountingProxy>) -> Option<Running> {
    let shared = tviews_database_url()?;
    require_typename(&shared).await;
    let url = own_database(&shared).await;
    provision(&url).await;
    let (schema, schema_dir) = schema(&url).await;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    let config = ServerConfig {
        database_url: proxy.map_or_else(|| url.clone(), |p| p.url().to_string()),
        cors_enabled: false,
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
    // The server's listener connects in the background: give it time to listen.
    tokio::time::sleep(Duration::from_millis(500)).await;
    Some(Running {
        base: format!("http://127.0.0.1:{port}"),
        url,
        http: reqwest::Client::new(),
        _schema: schema_dir,
        _shutdown: tx,
    })
}

impl Running {
    async fn get(&self, path: &str) -> Value {
        let response = self.http.get(format!("{}{path}", self.base)).send().await.unwrap();
        assert!(response.status().is_success(), "{path}: {}", response.status());
        response.json().await.unwrap()
    }

    async fn sql(&self, statement: &str) {
        client(&self.url).await.batch_execute(statement).await.unwrap();
    }

    /// Read `path` until `seen` holds, for up to five seconds: the feed is delivered
    /// after the write commits.
    async fn eventually(&self, path: &str, seen: impl Fn(&Value) -> bool) -> Value {
        let mut body = self.get(path).await;
        for _ in 0..50 {
            if seen(&body) {
                return body;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
            body = self.get(path).await;
        }
        panic!("{path} never changed: {body}");
    }
}

/// Every string in `body`, wherever it sits.
fn texts(body: &Value) -> Vec<String> {
    match body {
        Value::String(s) => vec![s.clone()],
        Value::Array(items) => items.iter().flat_map(texts).collect(),
        Value::Object(map) => map.values().flat_map(texts).collect(),
        _ => Vec::new(),
    }
}

fn shows(body: &Value, text: &str) -> bool {
    texts(body).iter().any(|t| t == text)
}

const POSTS: &str = "/rest/v1/posts?select=id,title";
const POST_A_WITH_COMMENTS: &str = "/rest/v1/posts?select=id,title,comments(id,body)&title[eq]=A";
const COMMENTS: &str = "/rest/v1/comments?select=id,body";

/// Posts A and B with their comments, by id: a title filter would no longer match once
/// [`Running::cache_both_composed_reads`] renames them.
fn with_comments(post: &str) -> String {
    format!("/rest/v1/posts?select=id,title,comments(id,body)&id[eq]={post}")
}

impl Running {
    /// Run `addComments` through this server: `count` comments on `post`, the transaction
    /// publishing rows, or not.
    async fn add_comments(&self, post: &str, body: &str, count: u32, publish: bool) {
        let query = format!(
            "mutation {{ addComments(id: \"{post}\", body: \"{body}\", count: {count}, \
             publish: {publish}) {{ id }} }}"
        );
        let response: Value = self
            .http
            .post(format!("{}/graphql", self.base))
            .json(&serde_json::json!({ "query": query }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(response.get("errors").is_none(), "{response}");
    }

    /// Cache the composed reads of posts A and B, then rename both posts behind the server's
    /// back, which `pg_tviews` does not publish: a read served from the cache still shows the
    /// old titles, one the cache missed shows `behind`.
    async fn cache_both_composed_reads(&self) {
        assert!(shows(&self.get(&with_comments(POST_A)).await, "A"), "post A is read");
        assert!(shows(&self.get(&with_comments(POST_B)).await, "B"), "post B is read");
        self.sql(&format!(
            "UPDATE {SCHEMA}.tv_p1589 SET data = data || '{{\"title\": \"behind\"}}'"
        ))
        .await;
        assert!(shows(&self.get(&with_comments(POST_A)).await, "A"), "A's read is cached");
        assert!(shows(&self.get(&with_comments(POST_B)).await, "B"), "B's read is cached");
    }
}

/// #1603: a comment this server writes on post A evicts the composed reads listing A's
/// comments, by the collections `pg_tviews`' report names, and not post B's. Before, an own
/// write evicted every composed read of comments. The feed's messages are dropped first: A's
/// eviction is the write's own, at once, not the feed's.
#[tokio::test]
async fn a_comment_this_server_writes_evicts_the_composed_reads_of_its_post_only() {
    let Some(shared) = tviews_database_url() else {
        eprintln!("skipping: TVIEWS_DATABASE_URL not set");
        return;
    };
    require_typename(&shared).await;
    let proxy = CountingProxy::start(&own_database(&shared).await);
    let Some(server) = serve_with_feed_through(Some(&proxy)).await else {
        return;
    };
    server.cache_both_composed_reads().await;
    proxy.drop_notifications(true);

    server.add_comments(POST_A, "own", 1, true).await;
    let post_a = server.get(&with_comments(POST_A)).await;
    let post_b = server.get(&with_comments(POST_B)).await;
    assert!(shows(&post_a, "own"), "post A's composed read was not evicted: {post_a}");
    assert!(
        shows(&post_b, "B"),
        "a comment on post A evicted post B's composed read: {post_b}"
    );
}

/// #1603: a transaction that does not publish rows captures no collections, so its report
/// cannot say which lists changed: the write evicts every composed read of comments, as
/// before.
#[tokio::test]
async fn a_write_that_captured_no_collections_evicts_every_composed_read_of_its_type() {
    let Some(shared) = tviews_database_url() else {
        eprintln!("skipping: TVIEWS_DATABASE_URL not set");
        return;
    };
    require_typename(&shared).await;
    let proxy = CountingProxy::start(&own_database(&shared).await);
    let Some(server) = serve_with_feed_through(Some(&proxy)).await else {
        return;
    };
    server.cache_both_composed_reads().await;
    proxy.drop_notifications(true);

    server.add_comments(POST_A, "unpublished", 1, false).await;
    let post_a = server.get(&with_comments(POST_A)).await;
    let post_b = server.get(&with_comments(POST_B)).await;
    assert!(
        shows(&post_a, "unpublished"),
        "post A's composed read was not evicted: {post_a}"
    );
    assert!(shows(&post_b, "behind"), "post B's composed read was not evicted: {post_b}");
}

/// #1603: more comments than the report lists (`max_updated_entities`, 500) put `Comment`
/// in `invalidated_types`, which stands for every collection of comments: post B's composed
/// read is evicted too.
#[tokio::test]
async fn a_report_that_cannot_list_a_types_rows_evicts_every_collection_of_it() {
    let Some(shared) = tviews_database_url() else {
        eprintln!("skipping: TVIEWS_DATABASE_URL not set");
        return;
    };
    require_typename(&shared).await;
    let proxy = CountingProxy::start(&own_database(&shared).await);
    let Some(server) = serve_with_feed_through(Some(&proxy)).await else {
        return;
    };
    server.cache_both_composed_reads().await;
    proxy.drop_notifications(true);

    server.add_comments(POST_A, "many", 501, true).await;
    let post_a = server.get(&with_comments(POST_A)).await;
    let post_b = server.get(&with_comments(POST_B)).await;
    assert!(shows(&post_a, "many"), "post A's composed read was not evicted: {post_a}");
    assert!(shows(&post_b, "behind"), "post B's composed read was not evicted: {post_b}");
}

#[tokio::test]
async fn a_write_the_server_did_not_run_evicts_the_reads_of_its_type() {
    let Some(server) = serve().await else {
        eprintln!("skipping: TVIEWS_DATABASE_URL not set");
        return;
    };
    assert!(shows(&server.get(POSTS).await, "A"));
    server
        .sql(&format!(
            "UPDATE {SCHEMA}.tv_p1589 SET data = data || '{{\"title\": \"behind\"}}' WHERE pk_p1589 = 1"
        ))
        .await;
    assert!(shows(&server.get(POSTS).await, "A"), "the read is cached");

    server
        .sql(&format!("UPDATE {SCHEMA}.tb_p1589 SET title = 'renamed' WHERE pk_p1589 = 1"))
        .await;
    server.eventually(POSTS, |body| shows(body, "renamed")).await;
}

#[tokio::test]
async fn a_comment_added_to_a_post_evicts_the_composed_read_of_that_post() {
    let Some(server) = serve().await else {
        eprintln!("skipping: TVIEWS_DATABASE_URL not set");
        return;
    };
    assert!(shows(&server.get(POST_A_WITH_COMMENTS).await, "first"));
    server
        .sql(&format!(
            "UPDATE {SCHEMA}.tv_c1589 SET data = data || '{{\"body\": \"behind\"}}' WHERE pk_c1589 = 1"
        ))
        .await;
    assert!(shows(&server.get(POST_A_WITH_COMMENTS).await, "first"), "the read is cached");

    server
        .sql(&format!(
            "INSERT INTO {SCHEMA}.tb_c1589 VALUES (2, gen_random_uuid(), 1, 'second')"
        ))
        .await;
    server.eventually(POST_A_WITH_COMMENTS, |body| shows(body, "second")).await;
}

#[tokio::test]
async fn a_comment_added_to_another_post_keeps_the_composed_read() {
    let Some(server) = serve().await else {
        eprintln!("skipping: TVIEWS_DATABASE_URL not set");
        return;
    };
    assert!(shows(&server.get(POST_A_WITH_COMMENTS).await, "first"));
    assert!(shows(&server.get(COMMENTS).await, "first"));
    server
        .sql(&format!(
            "UPDATE {SCHEMA}.tv_c1589 SET data = data || '{{\"body\": \"behind\"}}' WHERE pk_c1589 = 1"
        ))
        .await;

    server
        .sql(&format!(
            "INSERT INTO {SCHEMA}.tb_c1589 VALUES (3, gen_random_uuid(), 2, 'on B')"
        ))
        .await;
    // The list of every comment is evicted, so the message has been applied…
    server.eventually(COMMENTS, |body| shows(body, "on B")).await;
    // …and post A's comments, which it did not change, are still served from the cache.
    assert!(
        shows(&server.get(POST_A_WITH_COMMENTS).await, "first"),
        "a comment of post B evicted post A's composed read"
    );
}

#[tokio::test]
async fn a_role_that_does_not_publish_keeps_the_feed_off() {
    let Some(shared) = tviews_database_url() else {
        eprintln!("skipping: TVIEWS_DATABASE_URL not set");
        return;
    };
    require_typename(&shared).await;
    let url = own_database(&shared).await;
    provision(&url).await;
    let (schema, _dir) = schema(&url).await;
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    let catalog = || fraiseql_core::cache::tviews_feed::read_feed_catalog(&adapter, &schema);
    assert!(catalog().await.unwrap().is_some(), "the database publishes rows");

    // Another writer's role turns publishing off: its writes would never be heard.
    let admin = client(&url).await;
    admin
        .batch_execute(&format!(
            "DROP ROLE IF EXISTS p1589_writer; CREATE ROLE p1589_writer;
             ALTER ROLE p1589_writer IN DATABASE {DATABASE} SET pg_tviews.publish_changes = 'off';"
        ))
        .await
        .unwrap();
    let refused = catalog().await.unwrap();
    admin
        .batch_execute(&format!(
            "ALTER ROLE p1589_writer IN DATABASE {DATABASE} RESET pg_tviews.publish_changes;
             DROP ROLE p1589_writer;"
        ))
        .await
        .unwrap();
    assert!(refused.is_none(), "a role that does not publish must keep the feed off");
}

/// A feed whose connection stops delivering (a half-open TCP connection, a stalled listener)
/// would pass for a live one: writes made behind the server's back stay cached. The
/// listener's heartbeat notices its probe is not heard back, and clears the cache.
#[tokio::test]
async fn a_feed_that_stops_delivering_is_noticed() {
    let Some(shared) = tviews_database_url() else {
        eprintln!("skipping: TVIEWS_DATABASE_URL not set");
        return;
    };
    require_typename(&shared).await;
    let proxy = CountingProxy::start(&own_database(&shared).await);
    let Some(server) = serve_with_feed_through(Some(&proxy)).await else {
        return;
    };
    assert!(shows(&server.get(POSTS).await, "A"));
    server
        .sql(&format!("UPDATE {SCHEMA}.tb_p1589 SET title = 'heard' WHERE pk_p1589 = 1"))
        .await;
    server.eventually(POSTS, |body| shows(body, "heard")).await;

    proxy.drop_notifications(true);
    server
        .sql(&format!("UPDATE {SCHEMA}.tb_p1589 SET title = 'unheard' WHERE pk_p1589 = 1"))
        .await;
    let start = std::time::Instant::now();
    loop {
        if shows(&server.get(POSTS).await, "unheard") {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "the feed went silent and the server kept serving its cache"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// `post(id:)` reads one row of `tv_p1589` by its id (#1576): the feed evicts it by the rows
/// it names, so a write to post B, made behind the server's back, leaves post A cached.
#[tokio::test]
async fn the_feed_evicts_the_reads_by_id_of_the_rows_it_names_only() {
    let Some(server) = serve().await else {
        eprintln!("skipping: TVIEWS_DATABASE_URL not set");
        return;
    };
    let title = |id: &str| format!("{{ post(id: \"{id}\") {{ title }} }}");
    let read = |id: &str| {
        let query = title(id);
        let server = &server;
        async move {
            let body: Value = server
                .http
                .post(format!("{}/graphql", server.base))
                .json(&serde_json::json!({ "query": query }))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            assert!(body.get("errors").is_none(), "{body}");
            body["data"]["post"]["title"].as_str().unwrap_or_default().to_string()
        }
    };
    assert_eq!(read(POST_A).await, "A");
    assert_eq!(read(POST_B).await, "B");
    // Unpublished edits of the TVIEW rows: a read served from the cache does not show them.
    server
        .sql(&format!(
            "UPDATE {SCHEMA}.tv_p1589 SET data = data || '{{\"title\": \"behind\"}}'"
        ))
        .await;
    assert_eq!(read(POST_A).await, "A", "post A is cached");
    assert_eq!(read(POST_B).await, "B", "post B is cached");

    server
        .sql(&format!("UPDATE {SCHEMA}.tb_p1589 SET title = 'renamed' WHERE pk_p1589 = 2"))
        .await;
    let mut seen = String::new();
    for _ in 0..50 {
        seen = read(POST_B).await;
        if seen == "renamed" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(seen, "renamed", "post B is evicted by the feed");
    assert_eq!(read(POST_A).await, "A", "post A, which the feed did not name, is still cached");
}
