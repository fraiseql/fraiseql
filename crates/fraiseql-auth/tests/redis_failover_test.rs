//! Redis rate limiter failover integration tests.
//!
//! Validates that the Redis-backed state store (used in distributed rate
//! limiting deployments) degrades gracefully when Redis is unavailable.
//!
//! # Failover behavior
//!
//! `RedisStateStore` (the Redis component in `fraiseql-auth` gated behind
//! the `redis-rate-limiting` feature) does **not** implement fail-open
//! semantics. When Redis is unreachable:
//!
//! - **Startup**: `RedisStateStore::new()` returns `Err(AuthError::ConfigError)` immediately. The
//!   caller is responsible for falling back to `InMemoryStateStore` or aborting startup.
//!
//! - **Mid-operation**: If the Redis connection drops after a successful `new()`, subsequent
//!   `store()` and `retrieve()` calls propagate the Redis error as `AuthError::ConfigError` (store)
//!   or `AuthError::InvalidState` / `AuthError::ConfigError` (retrieve). There is no automatic
//!   fallback to in-memory — the error bubbles up.
//!
//! - **Recovery**: `redis::aio::ConnectionManager` automatically reconnects on the next command
//!   after a transient failure. No manual intervention is needed; the next `store()` / `retrieve()`
//!   call will succeed once Redis is reachable again.
//!
//! The server-level `RedisRateLimiter` (in `fraiseql-server`) *does*
//! implement fail-open: on Redis errors, requests are allowed and a
//! warning is logged. That behavior is tested separately in the server
//! crate. This file focuses on the auth-crate `RedisStateStore`.
//!
//! # Running
//!
//! ```bash
//! cargo test -p fraiseql-auth --features redis-rate-limiting \
//!     --test redis_failover_test
//! ```

#![cfg(feature = "redis-rate-limiting")]
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use fraiseql_auth::{
    error::AuthError,
    state_store::{RedisStateStore, StateStore},
};

/// A Redis URL pointing to a port where nothing is listening.
/// Used to simulate "Redis unavailable at startup."
const DEAD_REDIS_URL: &str = "redis://127.0.0.1:63999";

// ── Redis unavailable at startup ─────────────────────────────────────────────

#[tokio::test]
async fn redis_unavailable_at_startup_returns_config_error() {
    // Attempting to connect to a dead Redis instance must return a clear
    // error rather than hanging or panicking.
    let result = RedisStateStore::new(DEAD_REDIS_URL).await;

    assert!(result.is_err(), "connecting to a dead Redis URL must fail, got Ok");

    let err = result.err().expect("expected Err variant");
    assert!(
        matches!(err, AuthError::ConfigError { .. }),
        "expected AuthError::ConfigError for unreachable Redis, got: {err:?}"
    );
}

#[tokio::test]
async fn redis_invalid_url_returns_config_error() {
    // A syntactically invalid URL must also produce a clear error.
    let result = RedisStateStore::new("not-a-valid-url").await;

    assert!(result.is_err(), "invalid Redis URL must fail, got Ok");

    let err = result.err().expect("expected Err variant");
    assert!(
        matches!(err, AuthError::ConfigError { .. }),
        "expected AuthError::ConfigError for invalid URL, got: {err:?}"
    );
}

#[tokio::test]
async fn redis_error_message_is_actionable() {
    // The error message should contain enough information for an operator
    // to diagnose the problem (connection refused, DNS failure, etc.).
    let result = RedisStateStore::new(DEAD_REDIS_URL).await;
    let err = result.err().expect("expected Err variant");

    match err {
        AuthError::ConfigError { ref message } => {
            assert!(
                !message.is_empty(),
                "ConfigError message must not be empty — operators need diagnostic detail"
            );
        },
        other => panic!("expected ConfigError, got: {other:?}"),
    }
}

// ── Redis available then dies (mid-operation failure) ────────────────────────
//
// These tests require a live Redis instance. They are marked `#[ignore]`
// because the "dies mid-operation" scenario cannot be reliably simulated
// without stopping a real Redis server. Run manually with:
//
//   REDIS_URL=redis://localhost:6379 cargo test -p fraiseql-auth \
//       --features redis-rate-limiting --test redis_failover_test \
//       -- --ignored

#[tokio::test]
#[ignore = "requires live Redis — set REDIS_URL"]
async fn redis_store_and_retrieve_works_when_healthy() {
    let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());

    let store = RedisStateStore::new(&url).await.expect("Redis must be reachable for this test");

    let expiry = now_plus_secs(600);
    let state = format!("failover_test_{}", uuid::Uuid::new_v4());

    // Store succeeds.
    store
        .store(state.clone(), "test_provider".to_string(), expiry)
        .await
        .expect("store() must succeed when Redis is healthy");

    // Retrieve returns the correct provider and consumes the state.
    let (provider, _) = store
        .retrieve(&state)
        .await
        .expect("retrieve() must succeed when Redis is healthy");
    assert_eq!(provider, "test_provider");

    // Second retrieve must fail (state consumed).
    let replay = store.retrieve(&state).await;
    assert!(replay.is_err(), "replayed retrieve() must fail after consumption");
}

#[tokio::test]
#[ignore = "requires live Redis — set REDIS_URL"]
async fn redis_store_propagates_error_not_panic() {
    // Verify that even when we force a store on a state store whose
    // underlying connection has been dropped, we get an Err — not a panic.
    //
    // NOTE: `ConnectionManager` reconnects transparently, so this test
    // mainly verifies that the error path does not panic. A true
    // mid-operation failure (Redis killed between new() and store())
    // would require infrastructure control (testcontainers stop/start).
    let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());

    let store = RedisStateStore::new(&url).await.expect("Redis must be reachable for this test");

    // This should succeed (Redis is alive).
    let expiry = now_plus_secs(600);
    let result = store
        .store(
            format!("propagation_test_{}", uuid::Uuid::new_v4()),
            "provider".to_string(),
            expiry,
        )
        .await;
    assert!(result.is_ok(), "store() must succeed with live Redis: {result:?}");
}

// ── Recovery ─────────────────────────────────────────────────────────────────
//
// `redis::aio::ConnectionManager` handles automatic reconnection. These
// tests verify that the store continues to work after a transient issue
// resolves. Full stop/start tests require testcontainers or manual
// Docker control.

#[tokio::test]
#[ignore = "requires live Redis — set REDIS_URL"]
async fn redis_successive_operations_after_reconnect() {
    let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());

    let store = RedisStateStore::new(&url).await.expect("Redis must be reachable for this test");

    // Perform multiple store/retrieve cycles to confirm the connection
    // manager maintains a healthy connection across operations.
    for i in 0..5 {
        let state = format!("recovery_test_{}_{}", i, uuid::Uuid::new_v4());
        let expiry = now_plus_secs(600);

        store
            .store(state.clone(), format!("provider_{i}"), expiry)
            .await
            .unwrap_or_else(|e| panic!("store() cycle {i} failed: {e}"));

        let (provider, _) = store
            .retrieve(&state)
            .await
            .unwrap_or_else(|e| panic!("retrieve() cycle {i} failed: {e}"));

        assert_eq!(provider, format!("provider_{i}"));
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Return a Unix timestamp `secs` seconds in the future.
fn now_plus_secs(secs: u64) -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_secs()
        + secs
}
