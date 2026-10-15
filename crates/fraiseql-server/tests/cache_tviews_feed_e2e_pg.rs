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
//! that can create a database and set the superuser setting. Without them the suite skips.
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

/// Whether the database's `pg_tviews` publishes collections (`tviews.registry.typename`).
async fn publishes_collections(url: &str) -> bool {
    let client = client(url).await;
    client.batch_execute("CREATE EXTENSION IF NOT EXISTS pg_tviews;").await.unwrap();
    client
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM pg_attribute WHERE attrelid = to_regclass('tviews.registry') \
             AND attname = 'typename' AND NOT attisdropped)",
            &[],
        )
        .await
        .unwrap()
        .get(0)
}

async fn provision(url: &str) {
    let client = client(url).await;
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
               FROM {SCHEMA}.tb_c1589 c JOIN {SCHEMA}.tb_p1589 p ON p.pk_p1589 = c.fk_p1589 $q$);"
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
    if !publishes_collections(&shared).await {
        eprintln!("skipping: pg_tviews in TVIEWS_DATABASE_URL has no tviews.registry.typename");
        return None;
    }
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
    if !publishes_collections(&shared).await {
        eprintln!("skipping: pg_tviews in TVIEWS_DATABASE_URL has no tviews.registry.typename");
        return;
    }
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
