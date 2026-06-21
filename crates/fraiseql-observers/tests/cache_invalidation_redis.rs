//! Real-Redis integration tests for the cache-invalidation action (#428).
//!
//! Proves the `cache`/`invalidate` transport actually deletes keys — not a
//! fabricated success. The Redis-touching tests seed keys in a bound Redis,
//! fire the action, and assert the keys are gone; the failure-path tests need no
//! Redis and run unconditionally.
//!
//! # Requirements
//!
//! A Redis reachable via `REDIS_URL` (provided by the Dagger `integration(redis)`
//! leg, which binds a `redis` service). Without it, the Redis-touching tests
//! self-skip via [`fraiseql_test_support::redis`].
//!
//! Run locally against a Redis container:
//! ```bash
//! docker run --rm -p 6379:6379 redis:7-alpine &
//! REDIS_URL=redis://127.0.0.1:6379 \
//!   cargo test -p fraiseql-observers --features 'caching,testing' \
//!   --test cache_invalidation_redis -- --test-threads=1 --nocapture
//! ```
#![cfg(feature = "caching")]
// Reason: integration test — panics on failure, prints skip diagnostics, and uses
// small by-value test helpers.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    clippy::single_match_else,
    clippy::needless_pass_by_value
)]

use std::time::Duration;

use fraiseql_observers::{EntityEvent, EventKind, RedisCacheInvalidator, RedisConfig};
use redis::aio::ConnectionManager;
use uuid::Uuid;

/// `{{ id }}` placeholder, kept as a const so `format!` does not have to escape
/// the doubled braces.
const ID_PH: &str = "{{ id }}";

/// Resolve `REDIS_URL` (empty/unset → `None`, so the test self-skips).
async fn redis_url() -> Option<String> {
    fraiseql_test_support::redis().await.map(|s| s.url().to_string())
}

async fn raw_conn(url: &str) -> ConnectionManager {
    let client = redis::Client::open(url).expect("valid redis url");
    ConnectionManager::new(client).await.expect("connect to redis")
}

async fn set_key(conn: &ConnectionManager, key: &str) {
    let _: () = redis::cmd("SET")
        .arg(key)
        .arg("v")
        .query_async(&mut conn.clone())
        .await
        .expect("SET succeeds");
}

async fn key_exists(conn: &ConnectionManager, key: &str) -> bool {
    let n: i64 = redis::cmd("EXISTS")
        .arg(key)
        .query_async(&mut conn.clone())
        .await
        .expect("EXISTS succeeds");
    n == 1
}

fn invalidator_config(url: &str) -> RedisConfig {
    RedisConfig {
        url: url.to_string(),
        ..RedisConfig::default()
    }
}

fn event_with_id(id: serde_json::Value) -> EntityEvent {
    EntityEvent::new(
        EventKind::Updated,
        "Order".to_string(),
        Uuid::new_v4(),
        serde_json::json!({ "id": id }),
    )
}

// ── happy paths against real Redis ──────────────────────────────────────────

#[tokio::test]
async fn invalidate_direct_key_unlinks_exact_key() {
    let Some(url) = redis_url().await else {
        eprintln!("skip: REDIS_URL unset");
        return;
    };
    let conn = raw_conn(&url).await;
    let p = Uuid::new_v4();
    let key = format!("it428:{p}:app:order:123");
    set_key(&conn, &key).await;

    let inv = RedisCacheInvalidator::connect(&invalidator_config(&url)).await.unwrap();
    let event = event_with_id(serde_json::json!(123));
    let removed = inv
        .invalidate(&format!("it428:{p}:app:order:{ID_PH}"), &event)
        .await
        .expect("invalidate succeeds");

    assert_eq!(removed, 1, "exactly the one literal key is removed");
    assert!(!key_exists(&conn, &key).await, "the targeted key must be gone");
}

#[tokio::test]
async fn invalidate_glob_pattern_unlinks_only_matching_family() {
    let Some(url) = redis_url().await else {
        eprintln!("skip: REDIS_URL unset");
        return;
    };
    let conn = raw_conn(&url).await;
    let p = Uuid::new_v4();
    let k1 = format!("it428:{p}:app:user:7:page:1");
    let k2 = format!("it428:{p}:app:user:7:page:2");
    let other = format!("it428:{p}:app:user:8:page:1");
    set_key(&conn, &k1).await;
    set_key(&conn, &k2).await;
    set_key(&conn, &other).await;

    let inv = RedisCacheInvalidator::connect(&invalidator_config(&url)).await.unwrap();
    let event = event_with_id(serde_json::json!(7));
    // Trailing `*` is an author-intended glob → SCAN MATCH path.
    let removed = inv
        .invalidate(&format!("it428:{p}:app:user:{ID_PH}:*"), &event)
        .await
        .expect("invalidate succeeds");

    assert_eq!(removed, 2, "both pages for user 7 are removed");
    assert!(!key_exists(&conn, &k1).await);
    assert!(!key_exists(&conn, &k2).await);
    assert!(key_exists(&conn, &other).await, "user 8 must be untouched");
}

#[tokio::test]
async fn value_wildcard_is_escaped_not_expanded() {
    // The security boundary: a `*` in the (untrusted) event value must NOT widen
    // the match. With key_pattern `app:order:{{ id }}` and id="*", the value's
    // star is escaped, so the action deletes the *literal* key `app:order:*` and
    // leaves the sibling `app:order:legit` intact — no broad wipe.
    let Some(url) = redis_url().await else {
        eprintln!("skip: REDIS_URL unset");
        return;
    };
    let conn = raw_conn(&url).await;
    let p = Uuid::new_v4();
    let star_key = format!("it428:{p}:app:order:*");
    let legit = format!("it428:{p}:app:order:legit");
    set_key(&conn, &star_key).await;
    set_key(&conn, &legit).await;

    let inv = RedisCacheInvalidator::connect(&invalidator_config(&url)).await.unwrap();
    let event = event_with_id(serde_json::json!("*"));
    let removed = inv
        .invalidate(&format!("it428:{p}:app:order:{ID_PH}"), &event)
        .await
        .expect("invalidate succeeds");

    assert_eq!(removed, 1, "only the literal '*' key is removed, not via glob");
    assert!(!key_exists(&conn, &star_key).await, "the literal '*' key is gone");
    assert!(
        key_exists(&conn, &legit).await,
        "an unrelated sibling key must survive — value globs are escaped"
    );
}

#[tokio::test]
async fn missing_key_invalidation_is_a_no_op_zero_count() {
    let Some(url) = redis_url().await else {
        eprintln!("skip: REDIS_URL unset");
        return;
    };
    let p = Uuid::new_v4();
    let inv = RedisCacheInvalidator::connect(&invalidator_config(&url)).await.unwrap();
    let event = event_with_id(serde_json::json!(999));
    let removed = inv
        .invalidate(&format!("it428:{p}:app:order:{ID_PH}"), &event)
        .await
        .expect("invalidate of an absent key succeeds with zero removed");
    assert_eq!(removed, 0, "deleting a non-existent key removes nothing, loudly succeeds");
}

// ── failure paths (no Redis required) ───────────────────────────────────────

#[tokio::test]
async fn connect_to_unreachable_redis_fails_loud() {
    // Port 1 is reserved and never serving Redis: connect must surface an error
    // (or at worst time out) — it must NOT silently report a usable backend.
    let cfg = invalidator_config("redis://127.0.0.1:1");
    let result =
        tokio::time::timeout(Duration::from_secs(10), RedisCacheInvalidator::connect(&cfg)).await;
    match result {
        Ok(Ok(_)) => panic!("connect to a closed port must not succeed"),
        Ok(Err(_)) | Err(_) => { /* loud error, or timed out: both are non-silent */ },
    }
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn cache_action_through_executor_invalidates_for_real() {
    use std::{collections::HashMap, sync::Arc};

    use fraiseql_observers::{
        ActionConfig, EventMatcher, FailurePolicy, ObserverDefinition, ObserverExecutor,
        RetryConfig, testing::mocks::MockDeadLetterQueue,
    };

    let Some(url) = redis_url().await else {
        eprintln!("skip: REDIS_URL unset");
        return;
    };
    let conn = raw_conn(&url).await;
    let p = Uuid::new_v4();
    let key = format!("it428:{p}:app:order:55");
    set_key(&conn, &key).await;

    let inv = RedisCacheInvalidator::connect(&invalidator_config(&url)).await.unwrap();
    let observer = ObserverDefinition {
        event_type: "UPDATE".to_string(),
        entity:     "Order".to_string(),
        condition:  None,
        actions:    vec![ActionConfig::Cache {
            key_pattern: format!("it428:{p}:app:order:{ID_PH}"),
            action:      "invalidate".to_string(),
        }],
        retry:      RetryConfig {
            max_attempts: 1,
            initial_delay_ms: 0,
            ..RetryConfig::default()
        },
        on_failure: FailurePolicy::Log,
    };
    let mut observers = HashMap::new();
    observers.insert("obs".to_string(), observer);
    let matcher = EventMatcher::build(observers).unwrap();

    let dlq = Arc::new(MockDeadLetterQueue::new());
    let executor = ObserverExecutor::with_cache_invalidator(matcher, dlq, Arc::new(inv));

    let event = event_with_id(serde_json::json!(55));
    let summary = executor.process_event(&event).await.unwrap();

    assert_eq!(summary.successful_actions, 1, "the cache action must succeed end-to-end");
    assert_eq!(summary.failed_actions, 0);
    assert!(!key_exists(&conn, &key).await, "the wired action must actually delete the key");
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn cache_action_without_backend_reports_failure_not_success() {
    // The honest-failure invariant (#349 class): with `caching` compiled but NO
    // invalidator wired, a cache action must report failure — never the pre-H24
    // fabricated success. Needs no Redis.
    use std::{collections::HashMap, sync::Arc};

    use fraiseql_observers::{
        ActionConfig, EventMatcher, FailurePolicy, ObserverDefinition, ObserverExecutor,
        RetryConfig, testing::mocks::MockDeadLetterQueue,
    };

    let observer = ObserverDefinition {
        event_type: "UPDATE".to_string(),
        entity:     "Order".to_string(),
        condition:  None,
        actions:    vec![ActionConfig::Cache {
            key_pattern: "app:order:{{ id }}".to_string(),
            action:      "invalidate".to_string(),
        }],
        retry:      RetryConfig {
            max_attempts: 1,
            initial_delay_ms: 0,
            ..RetryConfig::default()
        },
        on_failure: FailurePolicy::Log,
    };
    let mut observers = HashMap::new();
    observers.insert("obs".to_string(), observer);
    let matcher = EventMatcher::build(observers).unwrap();

    let dlq = Arc::new(MockDeadLetterQueue::new());
    // `new` wires NO cache invalidator.
    let executor = ObserverExecutor::new(matcher, dlq);

    let event = event_with_id(serde_json::json!(1));
    let summary = executor.process_event(&event).await.unwrap();

    assert_eq!(summary.successful_actions, 0, "no fabricated success without a backend");
    assert!(summary.failed_actions >= 1, "the missing backend must surface as a failure");
}
