//! Tests for the `idempotency` module.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use serde_json::json;

use super::*;

fn make_store(ttl_secs: u64) -> InMemoryIdempotencyStore {
    InMemoryIdempotencyStore::new(Duration::from_secs(ttl_secs), 100)
}

/// A scope standing in for one tenant/method/resource, so these tests exercise the
/// store rather than the scoping rules (which have their own tests below).
fn scope() -> IdempotencyScope {
    IdempotencyScope {
        tenant: Some("tenant-a".to_string()),
        method: "POST".to_string(),
        path:   "/users".to_string(),
    }
}

/// A scoped key from the default scope.
fn k(client_key: &str) -> ScopedIdempotencyKey {
    scope().key(client_key)
}

fn make_response() -> StoredResponse {
    StoredResponse {
        status:  201,
        headers: vec![("x-request-id".to_string(), "abc".to_string())],
        body:    Some(json!({"id": 1, "name": "Alice"})),
    }
}

#[tokio::test]
async fn new_key_returns_new() {
    let store = make_store(3600);
    let body_hash = hash_body(&json!({"name": "Alice"}));
    assert!(matches!(store.check(&k("key1"), body_hash).await, IdempotencyCheck::New));
}

#[tokio::test]
async fn stored_key_replays_response() {
    let store = make_store(3600);
    let body = json!({"name": "Alice"});
    let body_hash = hash_body(&body);
    let response = make_response();

    store.store(k("key1"), body_hash, response).await;

    match store.check(&k("key1"), body_hash).await {
        IdempotencyCheck::Replay(stored) => {
            assert_eq!(stored.status, 201);
            assert_eq!(stored.body.as_ref().unwrap()["name"], "Alice");
        },
        other => panic!("Expected Replay, got {other:?}"),
    }
}

#[tokio::test]
async fn same_key_different_body_returns_conflict() {
    let store = make_store(3600);
    let body1 = json!({"name": "Alice"});
    let body2 = json!({"name": "Bob"});
    let hash1 = hash_body(&body1);
    let hash2 = hash_body(&body2);

    store.store(k("key1"), hash1, make_response()).await;

    assert!(matches!(store.check(&k("key1"), hash2).await, IdempotencyCheck::Conflict));
}

#[tokio::test(start_paused = true)]
async fn expired_key_treated_as_new() {
    let store = InMemoryIdempotencyStore::new(Duration::from_millis(1), 100);
    let body = json!({"name": "Alice"});
    let body_hash = hash_body(&body);

    store.store(k("key1"), body_hash, make_response()).await;

    // Advance frozen time past the TTL.
    tokio::time::advance(Duration::from_millis(5)).await;

    assert!(matches!(store.check(&k("key1"), body_hash).await, IdempotencyCheck::New));
}

#[tokio::test(start_paused = true)]
async fn max_entries_evicts_oldest() {
    let store = InMemoryIdempotencyStore::new(Duration::from_hours(1), 3);
    let hash = hash_body(&json!({}));

    store.store(k("key1"), hash, make_response()).await;
    tokio::time::advance(Duration::from_millis(1)).await;
    store.store(k("key2"), hash, make_response()).await;
    tokio::time::advance(Duration::from_millis(1)).await;
    store.store(k("key3"), hash, make_response()).await;
    tokio::time::advance(Duration::from_millis(1)).await;

    // This should evict key1 (oldest)
    store.store(k("key4"), hash, make_response()).await;

    assert!(matches!(store.check(&k("key1"), hash).await, IdempotencyCheck::New));
    // key2 should still be there
    assert!(matches!(store.check(&k("key2"), hash).await, IdempotencyCheck::Replay(_)));
}

#[test]
fn body_hash_deterministic() {
    let body = json!({"name": "Alice", "age": 30});
    let hash1 = hash_body(&body);
    let hash2 = hash_body(&body);
    assert_eq!(hash1, hash2);
}

#[test]
fn body_hash_different_for_different_bodies() {
    let hash1 = hash_body(&json!({"name": "Alice"}));
    let hash2 = hash_body(&json!({"name": "Bob"}));
    assert_ne!(hash1, hash2);
}

#[tokio::test]
async fn create_store_returns_arc() {
    let store = create_store(3600);
    let body_hash = hash_body(&json!({}));
    assert!(matches!(store.check(&k("key1"), body_hash).await, IdempotencyCheck::New));
}

// ---------------------------------------------------------------------------
// #911 — the body hash must not depend on key order
// ---------------------------------------------------------------------------

/// Two renderings of the same request must hash equally.
///
/// `hash_body` hashed `serde_json::to_vec(body)` — the *rendered* JSON — and since
/// `serde_json/preserve_order` became an unconditional workspace feature, `Value`
/// preserves insertion order in every build. A client that round-tripped its body through
/// a map between attempts (Go's `encoding/json` sorts keys; Python retry wrappers commonly
/// use `sort_keys=True`) therefore got `409 Conflict` on the retry the key exists to make
/// safe.
#[test]
fn body_hash_is_insensitive_to_key_order() {
    let a = hash_body(&json!({"name": "Alice", "email": "a@x"}));
    let b = hash_body(&json!({"email": "a@x", "name": "Alice"}));
    assert_eq!(a, b, "the same request in a different key order must hash the same");
}

/// Nested objects too — the normalizer recurses, and a shallow sort would miss this.
#[test]
fn body_hash_is_insensitive_to_nested_key_order() {
    let a = hash_body(&json!({"outer": {"x": 1, "y": [{"p": 1, "q": 2}]}}));
    let b = hash_body(&json!({"outer": {"y": [{"q": 2, "p": 1}], "x": 1}}));
    assert_eq!(a, b, "nested objects must normalize at every level");
}

/// Order-insensitivity must not become value-insensitivity.
#[test]
fn body_hash_still_separates_different_values() {
    assert_ne!(
        hash_body(&json!({"a": 1, "b": 2})),
        hash_body(&json!({"a": 2, "b": 1})),
        "swapping values is a different request and must hash differently"
    );
}

/// Array order is meaningful and must be preserved.
#[test]
fn body_hash_respects_array_order() {
    assert_ne!(
        hash_body(&json!({"items": [1, 2]})),
        hash_body(&json!({"items": [2, 1]})),
        "arrays are ordered; normalizing them would merge distinct requests"
    );
}

// ---------------------------------------------------------------------------
// #915 — a key is valid only within its scope
// ---------------------------------------------------------------------------

/// The same client key on two resources must not cross-replay.
///
/// This is the sharpest form of #915 and needs no second tenant: one client using one key
/// per logical operation rather than per endpoint was enough to receive a `201` describing
/// a user in response to a request that created nothing on orders.
#[tokio::test]
async fn the_same_client_key_does_not_cross_resources() {
    let store = make_store(3600);
    let body = json!({});
    let hash = hash_body(&body);

    let users = IdempotencyScope {
        tenant: Some("tenant-a".to_string()),
        method: "POST".to_string(),
        path:   "/users".to_string(),
    };
    let orders = IdempotencyScope {
        path: "/orders".to_string(),
        ..users.clone()
    };

    store.store(users.key("op-1"), hash, make_response()).await;

    assert!(
        matches!(store.check(&orders.key("op-1"), hash).await, IdempotencyCheck::New),
        "a key stored against /users must not replay for /orders"
    );
    assert!(
        matches!(store.check(&users.key("op-1"), hash).await, IdempotencyCheck::Replay(_)),
        "the key must still replay within its own scope"
    );
}

/// The same client key and body from two tenants must not cross-replay.
#[tokio::test]
async fn the_same_client_key_does_not_cross_tenants() {
    let store = make_store(3600);
    let hash = hash_body(&json!({"sku": "X", "qty": 1}));

    let a = IdempotencyScope {
        tenant: Some("tenant-a".to_string()),
        method: "POST".to_string(),
        path:   "/orders".to_string(),
    };
    let b = IdempotencyScope {
        tenant: Some("tenant-b".to_string()),
        ..a.clone()
    };

    store.store(a.key("order-42"), hash, make_response()).await;

    assert!(
        matches!(store.check(&b.key("order-42"), hash).await, IdempotencyCheck::New),
        "tenant B must not receive tenant A's stored response"
    );
}

/// A key must not cross HTTP methods either.
#[tokio::test]
async fn the_same_client_key_does_not_cross_methods() {
    let store = make_store(3600);
    let hash = hash_body(&json!({}));

    let post = IdempotencyScope {
        tenant: None,
        method: "POST".to_string(),
        path:   "/users".to_string(),
    };
    let put = IdempotencyScope {
        method: "PUT".to_string(),
        ..post.clone()
    };

    store.store(post.key("k"), hash, make_response()).await;

    assert!(matches!(store.check(&put.key("k"), hash).await, IdempotencyCheck::New));
}

/// Scope segments are length-prefixed, so no two different scopes can compose the same
/// storage key by shifting content across the separator.
#[test]
fn scope_segments_cannot_be_forged_into_one_another() {
    let a = IdempotencyScope {
        tenant: Some("ab".to_string()),
        method: "POST".to_string(),
        path:   "/x".to_string(),
    };
    let b = IdempotencyScope {
        tenant: Some("a".to_string()),
        method: "bPOST".to_string(),
        path:   "/x".to_string(),
    };
    assert_ne!(a.key("k").as_str(), b.key("k").as_str());
}
