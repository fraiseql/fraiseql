//! #1577: with `cache_cross_instance = "notify"`, a write through one server evicts the
//! entries it made stale on every other server over the same database; and a server that
//! cannot hear the other servers' writes caches nothing.
//!
//! Two servers (`a`, `b`) share one database. `users` is cached with no expiry
//! (`cache_ttl_seconds = 0`), the case where nothing but an eviction ever refreshes an entry.
//! A test first proves `b` serves `users` from its cache (it changes the row behind both
//! servers' backs and reads the old value again); only then does it write through `a` and
//! read through `b`.
//!
//! `b`'s feed connection goes through a proxy (`CountingProxy`) the tests sever, or make
//! swallow notifications as a pooler in transaction mode does; its pool does not.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` · **Parallelism:**
//! drops and recreates its own `p1577` schema → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use fraiseql_cli::schema::{ConvertOptions, SchemaConverter, intermediate::IntermediateSchema};
use fraiseql_core::{db::postgres::PostgresAdapter, schema::CompiledSchema};
use fraiseql_server::{
    Server,
    server_config::{CacheCrossInstance, ServerConfig},
};
use fraiseql_test_support::wire::CountingProxy;
use serde_json::{Value, json};

const SCHEMA: &str = "p1577";
const USER: &str = "00000000-0000-0000-0000-000000001577";
/// How long a server may take to apply another server's write.
const DELIVERY: Duration = Duration::from_secs(2);

async fn database_url() -> String {
    fraiseql_test_support::postgres()
        .await
        .expect("DATABASE_URL must be set (or enable fraiseql-test-support/local-testcontainers)")
        .url()
        .to_string()
}

async fn client(url: &str) -> tokio_postgres::Client {
    let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client
}

async fn provision(url: &str) {
    let client = client(url).await;
    client
        .batch_execute(include_str!("../../fraiseql-cli/sql/helpers/mutation_response.sql"))
        .await
        .unwrap();
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {SCHEMA} CASCADE;
             CREATE SCHEMA {SCHEMA};
             CREATE TYPE {SCHEMA}.mutation_response AS (succeeded boolean,
               state_changed boolean, error_class text, status_detail text,
               http_status smallint, message text, entity_id uuid, entity_type text,
               entity jsonb, updated_fields text[], cascade jsonb, error_detail jsonb,
               metadata jsonb);
             CREATE TABLE {SCHEMA}.tb_user (pk_user bigint GENERATED ALWAYS AS IDENTITY
               PRIMARY KEY, id uuid NOT NULL UNIQUE, bio text NOT NULL);
             INSERT INTO {SCHEMA}.tb_user (id, bio) VALUES ('{USER}', 'v0');
             CREATE VIEW {SCHEMA}.v_user AS SELECT u.id,
               jsonb_build_object('id', u.id, 'bio', u.bio) AS data FROM {SCHEMA}.tb_user u;
             CREATE FUNCTION {SCHEMA}.fn_update_user(p_id uuid, p_bio text)
               RETURNS SETOF {SCHEMA}.mutation_response LANGUAGE plpgsql AS $$
             BEGIN
               UPDATE {SCHEMA}.tb_user SET bio = p_bio WHERE id = p_id;
               RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
                 (SELECT data FROM {SCHEMA}.v_user WHERE id = p_id), p_id, 'User');
             END $$;"
        ))
        .await
        .unwrap();
}

/// `users`, cached with no expiry, and `updateUser`.
fn schema() -> CompiledSchema {
    let field = |name: &str, ty: &str| json!({ "name": name, "type": ty, "nullable": false });
    let intermediate: IntermediateSchema = serde_json::from_value(json!({
        "types": [{ "name": "User", "sql_source": format!("{SCHEMA}.v_user"), "is_input": false,
                    "fields": [field("id", "ID"), field("bio", "String")] }],
        "queries": [{ "name": "users", "return_type": "User", "returns_list": true,
                      "nullable": false, "sql_source": format!("{SCHEMA}.v_user"),
                      "arguments": [], "cache_ttl_seconds": 0 }],
        "mutations": [{ "name": "updateUser", "return_type": "User", "operation": "update",
                        "sql_source": format!("{SCHEMA}.fn_update_user"),
                        "arguments": [field("id", "ID"), field("bio", "String")] }],
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

/// A server whose pool reads `url`, and whose feed (if any) connects to `feed_url`.
async fn serve(url: &str, feed_url: &str, cross_instance: CacheCrossInstance) -> Running {
    let adapter = Arc::new(PostgresAdapter::new(url).await.unwrap());
    let config = ServerConfig {
        database_url: feed_url.to_string(),
        cache_cross_instance: cross_instance,
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
    Running {
        base:      format!("http://127.0.0.1:{port}"),
        _shutdown: tx,
    }
}

/// `url` naming its connections `name`.
fn named(url: &str, name: &str) -> String {
    format!("{url}{}application_name={name}", if url.contains('?') { "&" } else { "?" })
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

async fn bio(server: &Running) -> String {
    let body = graphql(server, "{ users { bio } }").await;
    body["data"]["users"][0]["bio"]
        .as_str()
        .unwrap_or_else(|| panic!("{body}"))
        .to_string()
}

async fn update_user(server: &Running, bio: &str) {
    graphql(
        server,
        &format!(r#"mutation {{ updateUser(id: "{USER}", bio: "{bio}") {{ id }} }}"#),
    )
    .await;
}

/// Change the row behind every server's back: no server evicts anything.
async fn behind_back(url: &str, bio: &str) {
    client(url)
        .await
        .execute(&format!("UPDATE {SCHEMA}.tb_user SET bio = $1"), &[&bio])
        .await
        .unwrap();
}

/// Whether `server` serves `users` from its cache now: read, change the row behind its back,
/// read again.
async fn serves_from_cache(server: &Running, url: &str, probe: &str) -> bool {
    let before = bio(server).await;
    behind_back(url, probe).await;
    bio(server).await == before
}

/// Wait until `server` caches `users` (its feed is listening); panic after 10 s.
async fn until_caching(server: &Running, url: &str) {
    let start = Instant::now();
    for i in 0.. {
        if serves_from_cache(server, url, &format!("probe {i}")).await {
            return;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "the server never cached `users`");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Wait until `server` serves `expected`; how long that took. Panics after [`DELIVERY`].
async fn until_served(server: &Running, expected: &str) -> Duration {
    let start = Instant::now();
    loop {
        let served = bio(server).await;
        if served == expected {
            return start.elapsed();
        }
        assert!(
            start.elapsed() < DELIVERY,
            "still serving {served:?} {DELIVERY:?} after another server wrote {expected:?}"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// The process id of the backend named `name`, once one exists; panics after 10 s.
async fn backend(url: &str, name: &str, other_than: Option<i32>) -> i32 {
    let client = client(url).await;
    let start = Instant::now();
    loop {
        let row = client
            .query_opt(
                "SELECT pid FROM pg_stat_activity WHERE application_name = $1 AND pid <> $2",
                &[&name, &other_than.unwrap_or(0)],
            )
            .await
            .unwrap();
        if let Some(row) = row {
            return row.get(0);
        }
        assert!(start.elapsed() < Duration::from_secs(10), "no backend named {name}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn a_write_through_one_server_evicts_the_entry_on_another() {
    let url = database_url().await;
    provision(&url).await;
    let a = serve(&url, &named(&url, "p1577a"), CacheCrossInstance::Notify).await;
    let b = serve(&url, &named(&url, "p1577b"), CacheCrossInstance::Notify).await;
    until_caching(&b, &url).await;

    update_user(&a, "written through a").await;

    let took = until_served(&b, "written through a").await;
    eprintln!("b served a's write {took:?} after it committed");
}

/// What the docs state for `"off"`: the cache is per process, and `cache_ttl_seconds = 0`
/// never expires on another server.
#[tokio::test]
async fn without_the_feed_a_write_through_one_server_leaves_another_stale() {
    let url = database_url().await;
    provision(&url).await;
    let a = serve(&url, &url, CacheCrossInstance::Off).await;
    let b = serve(&url, &url, CacheCrossInstance::Off).await;
    until_caching(&b, &url).await;
    let cached = bio(&b).await;

    update_user(&a, "written through a").await;

    tokio::time::sleep(DELIVERY).await;
    assert_eq!(bio(&b).await, cached, "b has no way to hear a's write");
    assert_eq!(bio(&a).await, "written through a", "a evicted its own entry");
}

#[tokio::test]
async fn a_server_that_cannot_reach_its_feed_caches_nothing() {
    let url = database_url().await;
    provision(&url).await;
    let proxy = CountingProxy::start(&url);
    proxy.sever();
    let b = serve(&url, proxy.url(), CacheCrossInstance::Notify).await;

    for i in 0..10 {
        assert!(
            !serves_from_cache(&b, &url, &format!("probe {i}")).await,
            "probe {i}: b cached without hearing the other servers"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// A `LISTEN` behind `PgBouncer` in transaction mode receives nothing, though every statement
/// succeeds: the server must notice it does not hear its own messages.
#[tokio::test]
async fn a_server_whose_feed_receives_nothing_caches_nothing() {
    let url = database_url().await;
    provision(&url).await;
    let proxy = CountingProxy::start(&url);
    proxy.drop_notifications(true);
    let b = serve(&url, proxy.url(), CacheCrossInstance::Notify).await;

    for i in 0..10 {
        assert!(
            !serves_from_cache(&b, &url, &format!("probe {i}")).await,
            "probe {i}: b cached though its feed receives nothing"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// While its feed is down a server cannot hear writes; it serves none of its entries, and
/// caches again once the feed is back.
#[tokio::test]
async fn a_server_whose_feed_is_down_serves_no_stale_entry_and_recovers() {
    let url = database_url().await;
    provision(&url).await;
    let proxy = CountingProxy::start(&url);
    let a = serve(&url, &url, CacheCrossInstance::Notify).await;
    let b = serve(&url, proxy.url(), CacheCrossInstance::Notify).await;
    until_caching(&b, &url).await;

    proxy.sever();
    // b notices the closed connection at once; give it a moment to suspend.
    tokio::time::sleep(Duration::from_millis(300)).await;
    update_user(&a, "written while b is deaf").await;
    assert_eq!(
        bio(&b).await,
        "written while b is deaf",
        "b served an entry it could not know was stale"
    );

    proxy.restore();
    until_caching(&b, &url).await;
}

/// The phase's criterion in its own terms: terminate the listener's backend, write, let it
/// reconnect, read fresh.
#[tokio::test]
async fn a_server_reads_fresh_after_its_feed_backend_is_terminated() {
    let url = database_url().await;
    provision(&url).await;
    let a = serve(&url, &url, CacheCrossInstance::Notify).await;
    let b = serve(&url, &named(&url, "p1577b"), CacheCrossInstance::Notify).await;
    until_caching(&b, &url).await;

    let listener = backend(&url, "p1577b", None).await;
    client(&url)
        .await
        .execute("SELECT pg_terminate_backend($1)", &[&listener])
        .await
        .unwrap();
    update_user(&a, "written while b reconnects").await;
    backend(&url, "p1577b", Some(listener)).await;

    until_served(&b, "written while b reconnects").await;
}
