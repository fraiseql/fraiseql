//! #985: the Redis `cache`/`invalidate` observer transport must be reachable
//! from the shipped server, not only by embedding the library.
//!
//! The transport itself shipped in #428 and was tested against real Redis — but
//! `ObserverExecutor::with_cache_invalidator` was an *alternative constructor*,
//! and the server builds its executor with `new_with_email`. Nothing in
//! `fraiseql-server` ever constructed a `RedisCacheInvalidator`, the server's
//! `[observers.runtime]` table is `deny_unknown_fields` and had no `redis` key
//! (so the block was rejected at config load), and the server crate did not even
//! enable `fraiseql-observers/caching`. Three independent walls between a
//! `fraiseql.toml` and a working cache action.
//!
//! So these tests deliberately go through the server's own composition path —
//! `ObserverRuntime::start`, which performs the mount — rather than building an
//! executor by hand. A library-level test cannot fail for the defect this issue
//! is about.
//!
//! ## Running
//!
//! ```bash
//! DATABASE_URL=postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql \
//! REDIS_URL=redis://localhost:6379 \
//!   cargo test -p fraiseql-server --features observers-cache \
//!   --test observer_cache_mount_redis -- --ignored --test-threads=1
//! ```

#![cfg(feature = "observers-cache")]
#![allow(clippy::unwrap_used)] // Reason: integration test file

mod observer_test_helpers;

use std::time::Duration;

use fraiseql_observers::config::RedisConfig;
use fraiseql_server::observers::runtime::{ObserverRuntime, ObserverRuntimeConfig};
use observer_test_helpers::{
    cleanup_test_data, create_test_pool, insert_change_log_entry, setup_observer_schema,
};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

fn redis_url() -> String {
    std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string())
}

fn redis_config() -> RedisConfig {
    RedisConfig {
        url: redis_url(),
        ..RedisConfig::default()
    }
}

/// Drop any `cache` observer this suite left behind (P03 fixture ownership).
///
/// `tb_observer` is shared and `load_observers` loads every enabled row, so a
/// leftover from an earlier aborted run would make the "no cache action" case
/// see one. This suite owns the `cache-obs-`/`cache-noredis-` name prefixes and
/// clears only those — never another suite's rows.
async fn clear_suite_observers(pool: &PgPool) {
    const OWNED: &str = "name LIKE 'cache-obs-%' OR name LIKE 'cache-noredis-%'";
    // `tb_observer_log.fk_observer` references `tb_observer`, so the audit rows
    // go first — the delete is otherwise refused by the constraint.
    sqlx::query(&format!(
        "DELETE FROM tb_observer_log WHERE fk_observer IN \
         (SELECT pk_observer FROM tb_observer WHERE {OWNED})"
    ))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(&format!("DELETE FROM tb_observer WHERE {OWNED}"))
        .execute(pool)
        .await
        .unwrap();
}

/// Register an observer whose only action invalidates `key_pattern`.
async fn create_cache_observer(
    pool: &PgPool,
    name: &str,
    entity_type: &str,
    key_pattern: &str,
) -> i64 {
    let actions = json!([{ "type": "cache", "key_pattern": key_pattern, "action": "invalidate" }]);
    let row: (i64,) = sqlx::query_as(
        r"INSERT INTO tb_observer (name, entity_type, event_type, actions, enabled)
          VALUES ($1, $2, 'INSERT', $3, true)
          RETURNING pk_observer",
    )
    .bind(name)
    .bind(entity_type)
    .bind(actions)
    .fetch_one(pool)
    .await
    .unwrap();
    row.0
}

/// The operator's path, end to end: a `cache` observer plus a configured Redis
/// backend, booted through `ObserverRuntime::start`, must actually delete the
/// key from a real Redis when the entity changes.
#[tokio::test]
#[ignore = "requires PostgreSQL + Redis — run with --ignored --test-threads=1"]
async fn a_cache_observer_booted_by_the_server_invalidates_a_real_redis_key() {
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.unwrap();
    clear_suite_observers(&pool).await;

    let test_id = Uuid::new_v4().to_string();
    let entity_type = format!("Product_{test_id}");
    let key = format!("fraiseql:test:{test_id}:one");

    // Seed the key the observer is supposed to remove.
    let client = redis::Client::open(redis_url()).unwrap();
    let mut conn = client.get_multiplexed_async_connection().await.unwrap();
    redis::cmd("SET")
        .arg(&key)
        .arg("cached")
        .query_async::<()>(&mut conn)
        .await
        .unwrap();

    create_cache_observer(
        &pool,
        &format!("cache-obs-{test_id}"),
        &entity_type,
        &format!("fraiseql:test:{test_id}:*"),
    )
    .await;

    // The shipped composition path: config in, runtime out. Nothing here builds
    // an executor or an invalidator by hand.
    let mut runtime = ObserverRuntime::new(
        ObserverRuntimeConfig::new(pool.clone())
            .with_poll_interval(50)
            .with_listener_id(format!("cache-mount-{test_id}"))
            .with_redis(Some(redis_config())),
    );
    runtime
        .start()
        .await
        .expect("runtime must boot with a cache observer and a Redis backend");

    insert_change_log_entry(
        &pool,
        "INSERT",
        &entity_type,
        &Uuid::new_v4().to_string(),
        json!({ "name": "widget" }),
        None,
    )
    .await
    .unwrap();

    // Poll for the deletion rather than sleeping a fixed span.
    let mut exists: i64 = 1;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        exists = redis::cmd("EXISTS").arg(&key).query_async(&mut conn).await.unwrap();
        if exists == 0 {
            break;
        }
    }

    runtime.stop().await.ok();
    let _ = redis::cmd("DEL").arg(&key).query_async::<i64>(&mut conn).await;
    cleanup_test_data(&pool, &test_id).await.ok();

    assert_eq!(
        exists, 0,
        "the server booted a `cache` observer with a Redis backend, but the key {key} \
         survived — the transport was never mounted on the executor the runtime runs"
    );
}

/// A `cache` action with no `[observers.runtime.redis]` must fail at **boot**.
///
/// The transport is genuinely absent, so every dispatch would fail identically
/// and forever; saying so once, at startup, is what lets an operator fix it.
#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn a_cache_observer_without_a_redis_backend_refuses_to_boot() {
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.unwrap();
    clear_suite_observers(&pool).await;

    let test_id = Uuid::new_v4().to_string();
    let entity_type = format!("Product_{test_id}");
    create_cache_observer(
        &pool,
        &format!("cache-obs-{test_id}"),
        &entity_type,
        &format!("fraiseql:test:{test_id}:*"),
    )
    .await;

    let mut runtime = ObserverRuntime::new(
        ObserverRuntimeConfig::new(pool.clone())
            .with_poll_interval(50)
            .with_listener_id(format!("cache-noredis-{test_id}")),
        // deliberately no .with_redis(..)
    );
    let err = runtime
        .start()
        .await
        .expect_err("a cache action with no Redis backend must fail at boot, not once per event");
    let message = err.to_string();

    runtime.stop().await.ok();
    cleanup_test_data(&pool, &test_id).await.ok();

    assert!(
        message.contains("cache") && message.contains("Redis"),
        "the boot error must name the missing backend so the operator can act on it, got: \
         {message}"
    );
}

/// An observer set with no `cache` action must not require Redis at all — the
/// mount is conditional on the declared actions, so the common deployment keeps
/// booting with no Redis anywhere.
#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn a_runtime_without_cache_actions_needs_no_redis() {
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.unwrap();
    clear_suite_observers(&pool).await;

    let test_id = Uuid::new_v4().to_string();
    let mut runtime = ObserverRuntime::new(
        ObserverRuntimeConfig::new(pool.clone())
            .with_poll_interval(50)
            .with_listener_id(format!("cache-absent-{test_id}")),
    );
    let started = runtime.start().await;
    runtime.stop().await.ok();
    cleanup_test_data(&pool, &test_id).await.ok();

    assert!(
        started.is_ok(),
        "no cache action declared, so Redis must not be required: {started:?}"
    );
}
