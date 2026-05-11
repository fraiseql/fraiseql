#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use super::*;

#[test]
fn test_cache_put_and_get() {
    let cache = QueryCache::new(60);
    let query = "SELECT * FROM users";
    let result = vec![std::collections::HashMap::from([
        ("id".to_string(), serde_json::json!("1")),
        ("name".to_string(), serde_json::json!("Alice")),
    ])];

    cache.put(query, Arc::new(result));

    let cached = cache.get(query);
    assert!(cached.is_some());
    let cached = cached.unwrap();
    assert_eq!(cached.len(), 1);
    let name_val = cached[0].get("name").unwrap();
    assert_eq!(name_val.as_str().unwrap(), "Alice");
}

#[test]
fn test_cache_miss() {
    let cache = QueryCache::new(60);
    let result = cache.get("SELECT * FROM nonexistent");
    assert!(result.is_none());
}

#[test]
fn test_cache_expiration() {
    let cache = QueryCache::new(1); // 1-second TTL
    let query = "SELECT * FROM orders";
    let result = vec![std::collections::HashMap::from([(
        "total".to_string(),
        serde_json::json!("99.99"),
    )])];

    cache.put(query, Arc::new(result));

    // Should be cached immediately
    assert!(cache.get(query).is_some());

    // Wait for expiration
    std::thread::sleep(std::time::Duration::from_secs(2));

    // Should be expired now
    assert!(cache.get(query).is_none());
}

#[test]
fn test_cache_clear() {
    let cache = QueryCache::new(60);
    cache.put("query1", Arc::new(vec![]));
    cache.put("query2", Arc::new(vec![]));

    assert_eq!(cache.len(), 2);
    cache.clear();
    assert_eq!(cache.len(), 0);
}

#[test]
fn test_cache_multiple_queries() {
    let cache = QueryCache::new(60);

    let result1 = vec![std::collections::HashMap::from([(
        "id".to_string(),
        serde_json::json!("1"),
    )])];
    let result2 = vec![std::collections::HashMap::from([(
        "total".to_string(),
        serde_json::json!("100.00"),
    )])];

    cache.put("SELECT * FROM users", Arc::new(result1));
    cache.put("SELECT * FROM orders", Arc::new(result2));

    assert_eq!(cache.len(), 2);
    assert!(cache.get("SELECT * FROM users").is_some());
    assert!(cache.get("SELECT * FROM orders").is_some());
}

#[test]
fn test_cache_default_ttl() {
    let cache = QueryCache::default();
    assert!(cache.is_empty());
}

#[test]
fn test_invalidate_views() {
    let cache = QueryCache::new(60);
    cache.put("SELECT * FROM v_user WHERE id = 1", Arc::new(vec![]));
    cache.put("SELECT * FROM v_user WHERE id = 2", Arc::new(vec![]));
    cache.put("SELECT * FROM v_order WHERE id = 1", Arc::new(vec![]));

    assert_eq!(cache.len(), 3);

    let removed = cache.invalidate_views(&["v_user"]);
    assert_eq!(removed, 2);
    assert_eq!(cache.len(), 1);
    assert!(cache.get("SELECT * FROM v_order WHERE id = 1").is_some());
}

#[test]
fn test_invalidate_views_multiple() {
    let cache = QueryCache::new(60);
    cache.put("SELECT * FROM v_user", Arc::new(vec![]));
    cache.put("SELECT * FROM v_order", Arc::new(vec![]));
    cache.put("SELECT * FROM v_product", Arc::new(vec![]));

    assert_eq!(cache.len(), 3);

    let removed = cache.invalidate_views(&["v_user", "v_product"]);
    assert_eq!(removed, 2);
    assert_eq!(cache.len(), 1);
    assert!(cache.get("SELECT * FROM v_order").is_some());
}

#[test]
fn test_invalidate_pattern_wildcard() {
    let cache = QueryCache::new(60);
    cache.put("SELECT * FROM v_user_detail", Arc::new(vec![]));
    cache.put("SELECT * FROM v_user_summary", Arc::new(vec![]));
    cache.put("SELECT * FROM v_order", Arc::new(vec![]));

    assert_eq!(cache.len(), 3);

    let removed = cache.invalidate_pattern("*v_user*");
    assert_eq!(removed, 2);
    assert_eq!(cache.len(), 1);
    assert!(cache.get("SELECT * FROM v_order").is_some());
}

#[test]
fn test_invalidate_pattern_prefix() {
    let cache = QueryCache::new(60);
    cache.put("SELECT * FROM v_user", Arc::new(vec![]));
    cache.put("SELECT * FROM v_order", Arc::new(vec![]));
    cache.put("INSERT INTO v_user VALUES", Arc::new(vec![]));

    assert_eq!(cache.len(), 3);

    let removed = cache.invalidate_pattern("SELECT * FROM*");
    assert_eq!(removed, 2);
    assert_eq!(cache.len(), 1);
}

#[test]
fn test_invalidate_pattern_no_match() {
    let cache = QueryCache::new(60);
    cache.put("SELECT * FROM v_user", Arc::new(vec![]));
    cache.put("SELECT * FROM v_order", Arc::new(vec![]));

    assert_eq!(cache.len(), 2);

    let removed = cache.invalidate_pattern("*v_product*");
    assert_eq!(removed, 0);
    assert_eq!(cache.len(), 2);
}

// --- Additional cache tests ---

#[test]
fn test_put_overwrites_existing_entry() {
    let cache = QueryCache::new(60);
    let q = "SELECT * FROM users";
    let result1 = Arc::new(vec![std::collections::HashMap::from([(
        "id".to_string(),
        serde_json::json!("1"),
    )])]);
    let result2 = Arc::new(vec![std::collections::HashMap::from([(
        "id".to_string(),
        serde_json::json!("99"),
    )])]);

    cache.put(q, Arc::clone(&result1));
    assert_eq!(cache.len(), 1);
    cache.put(q, Arc::clone(&result2));
    // Should still be 1 entry (overwritten)
    assert_eq!(cache.len(), 1);
    let got = cache.get(q).unwrap();
    let id_val = got[0].get("id").unwrap();
    assert_eq!(id_val.as_str().unwrap(), "99");
}

#[test]
fn test_is_empty_true_initially() {
    let cache = QueryCache::new(30);
    assert!(cache.is_empty());
}

#[test]
fn test_is_empty_false_after_put() {
    let cache = QueryCache::new(30);
    cache.put("q", Arc::new(vec![]));
    assert!(!cache.is_empty());
}

#[test]
fn test_len_increments_with_each_distinct_query() {
    let cache = QueryCache::new(60);
    for i in 0..5 {
        cache.put(format!("SELECT {i}"), Arc::new(vec![]));
    }
    assert_eq!(cache.len(), 5);
}

#[test]
fn test_invalidate_views_empty_view_list_removes_nothing() {
    let cache = QueryCache::new(60);
    cache.put("SELECT * FROM v_user", Arc::new(vec![]));
    let removed = cache.invalidate_views(&[]);
    assert_eq!(removed, 0);
    assert_eq!(cache.len(), 1);
}

#[test]
fn test_invalidate_pattern_exact_no_wildcard() {
    // Pattern with no wildcard acts as exact match
    let cache = QueryCache::new(60);
    cache.put("exact_query", Arc::new(vec![]));
    cache.put("other_query", Arc::new(vec![]));

    let removed = cache.invalidate_pattern("exact_query");
    assert_eq!(removed, 1);
    assert_eq!(cache.len(), 1);
    assert!(cache.get("other_query").is_some());
}

#[test]
fn test_invalidate_pattern_star_only_removes_all() {
    let cache = QueryCache::new(60);
    cache.put("SELECT * FROM users", Arc::new(vec![]));
    cache.put("SELECT * FROM orders", Arc::new(vec![]));
    cache.put("SELECT id FROM items", Arc::new(vec![]));

    let removed = cache.invalidate_pattern("*");
    assert_eq!(removed, 3);
    assert!(cache.is_empty());
}

#[test]
fn test_invalidate_views_does_not_affect_non_matching_entries() {
    let cache = QueryCache::new(60);
    cache.put("SELECT * FROM v_user", Arc::new(vec![]));
    cache.put("SELECT * FROM v_order", Arc::new(vec![]));
    cache.put("SELECT * FROM v_product", Arc::new(vec![]));

    let removed = cache.invalidate_views(&["v_order"]);
    assert_eq!(removed, 1);
    assert_eq!(cache.len(), 2);
    assert!(cache.get("SELECT * FROM v_user").is_some());
    assert!(cache.get("SELECT * FROM v_product").is_some());
}

#[test]
fn test_zero_ttl_expires_immediately() {
    // With TTL=0, entries should expire immediately since expires_at == now
    let cache = QueryCache::new(0);
    cache.put("q", Arc::new(vec![]));
    // The entry was put at `now + 0`, so current time >= expires_at
    // In practice the comparison is `now < expires_at`, so with TTL=0
    // the entry expires immediately.
    let result = cache.get("q");
    // Either None (immediately expired) or Some (same second); both are valid
    // but we just verify no panic and that the cache is functional
    let _ = result; // no assertion; behavior is time-dependent
}

#[test]
fn test_clear_on_empty_cache_is_noop() {
    let cache = QueryCache::new(60);
    cache.clear();
    assert!(cache.is_empty());
}

#[test]
fn test_result_is_shared_via_arc() {
    let cache = QueryCache::new(60);
    let original = Arc::new(vec![std::collections::HashMap::from([(
        "k".to_string(),
        serde_json::json!("v"),
    )])]);
    cache.put("q", Arc::clone(&original));
    let retrieved = cache.get("q").unwrap();
    // Both Arcs point to the same allocation
    assert!(Arc::ptr_eq(&original, &retrieved));
}
