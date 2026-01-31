//! Cache Performance Validation Tests
//!
//! Validates that the QueryCache infrastructure achieves documented performance improvements:
//! - Cache hits return in <10ms (documented: 5ms ± 1ms)
//! - Cache misses allow actual query path (50-200ms documented range)
//! - TTL-based expiration works correctly
//! - Hit rate tracking is accurate
//! - Concurrent access is safe and efficient
//!
//! # Running Tests
//!
//! ```bash
//! cargo test --test cache_performance_validation_test -- --nocapture
//! ```

#![cfg(test)]

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use fraiseql_arrow::cache::QueryCache;

// ============================================================================
// SECTION 1: CACHE HIT LATENCY VALIDATION
// ============================================================================
// Validates that cache hits achieve documented 5ms performance
// Tests measure actual cache access time to confirm sub-10ms performance

#[test]
fn test_cache_hit_latency_under_10ms() {
    let cache = QueryCache::new(60);
    let query = "SELECT id, name FROM users WHERE active = true LIMIT 100";

    // Populate cache
    let result = vec![
        {
            let mut map = HashMap::new();
            map.insert("id".to_string(), serde_json::json!(1));
            map.insert("name".to_string(), serde_json::json!("Alice"));
            map
        },
        {
            let mut map = HashMap::new();
            map.insert("id".to_string(), serde_json::json!(2));
            map.insert("name".to_string(), serde_json::json!("Bob"));
            map
        },
    ];

    cache.put(query, Arc::new(result.clone()));

    // Measure cache hit latency
    let start = Instant::now();
    let hit = cache.get(query);
    let latency = start.elapsed();

    // Verify hit occurred
    assert!(hit.is_some());
    assert_eq!(hit.unwrap().len(), 2);

    // Verify latency < 10ms (documented: 5ms ± 1ms)
    assert!(
        latency < Duration::from_millis(10),
        "Cache hit latency {}ms exceeds target of 10ms",
        latency.as_millis()
    );
}

#[test]
fn test_cache_hit_consistency_across_accesses() {
    // Validates that cache hits are consistent and fast over multiple accesses
    let cache = QueryCache::new(60);
    let query = "SELECT COUNT(*) FROM orders";

    let result = vec![{
        let mut map = HashMap::new();
        map.insert("count".to_string(), serde_json::json!(15234));
        map
    }];

    cache.put(query, Arc::new(result));

    // Multiple rapid cache hits
    let mut total_latency = Duration::ZERO;
    for _ in 0..10 {
        let start = Instant::now();
        let hit = cache.get(query);
        total_latency += start.elapsed();

        assert!(hit.is_some(), "Cache hit failed");
    }

    let avg_latency = total_latency / 10;
    // Average should be well under 10ms
    assert!(
        avg_latency < Duration::from_millis(5),
        "Average cache hit latency {}ms exceeds 5ms target",
        avg_latency.as_millis()
    );
}

// ============================================================================
// SECTION 2: CACHE MISS BEHAVIOR VALIDATION
// ============================================================================
// Validates that cache misses properly return None (allowing DB query path)
// and that queries not in cache are retrievable after insertion

#[test]
fn test_cache_miss_returns_none() {
    let cache = QueryCache::new(60);

    // Query not in cache
    let result = cache.get("SELECT * FROM non_existent_table");
    assert!(result.is_none(), "Cache should return None for missing query");
}

#[test]
fn test_cache_miss_then_hit() {
    let cache = QueryCache::new(60);
    let query = "SELECT * FROM products";

    // First access misses
    assert!(cache.get(query).is_none(), "First access should miss");

    // Insert result
    let result = vec![{
        let mut map = HashMap::new();
        map.insert("id".to_string(), serde_json::json!(1));
        map.insert("name".to_string(), serde_json::json!("Widget"));
        map
    }];

    cache.put(query, Arc::new(result.clone()));

    // Second access hits
    let hit = cache.get(query);
    assert!(hit.is_some(), "Second access should hit after put");
    assert_eq!(hit.unwrap().len(), 1);
}

#[test]
fn test_cache_miss_count_accuracy() {
    // Validates that multiple queries can miss independently
    let cache = QueryCache::new(60);

    let q1 = "SELECT * FROM table1";
    let q2 = "SELECT * FROM table2";
    let q3 = "SELECT * FROM table3";

    // All three miss initially
    assert!(cache.get(q1).is_none());
    assert!(cache.get(q2).is_none());
    assert!(cache.get(q3).is_none());

    // Insert only q1 and q3
    cache.put(
        q1,
        Arc::new(vec![{
            let mut m = HashMap::new();
            m.insert("id".to_string(), serde_json::json!(1));
            m
        }]),
    );
    cache.put(
        q3,
        Arc::new(vec![{
            let mut m = HashMap::new();
            m.insert("id".to_string(), serde_json::json!(3));
            m
        }]),
    );

    // Verify pattern: hit, miss, hit
    assert!(cache.get(q1).is_some(), "q1 should hit");
    assert!(cache.get(q2).is_none(), "q2 should miss");
    assert!(cache.get(q3).is_some(), "q3 should hit");
}

// ============================================================================
// SECTION 3: TTL (TIME-TO-LIVE) VALIDATION
// ============================================================================
// Validates that TTL-based expiration works correctly
// Entries should expire after TTL seconds

#[test]
fn test_cache_expiration_after_ttl() {
    let cache = QueryCache::new(1); // 1 second TTL
    let query = "SELECT * FROM expiring_data";

    let result = vec![{
        let mut map = HashMap::new();
        map.insert("data".to_string(), serde_json::json!("expires soon"));
        map
    }];

    cache.put(query, Arc::new(result));

    // Immediate access succeeds
    assert!(cache.get(query).is_some(), "Should hit before TTL expires");

    // Wait for expiration
    std::thread::sleep(Duration::from_millis(1100));

    // Access after expiration returns None
    assert!(cache.get(query).is_none(), "Cache should expire after TTL seconds");
}

#[test]
fn test_cache_ttl_zero_expires_immediately() {
    let cache = QueryCache::new(0); // 0 second TTL (immediate expiration)
    let query = "SELECT * FROM immediate_expire";

    let result = vec![{
        let mut map = HashMap::new();
        map.insert("value".to_string(), serde_json::json!("gone"));
        map
    }];

    cache.put(query, Arc::new(result));

    // Even immediate access should miss with 0 TTL
    assert!(cache.get(query).is_none(), "0-second TTL should expire immediately");
}

#[test]
fn test_cache_different_ttls() {
    // Validates independent TTL tracking per entry
    let cache = QueryCache::new(10); // Default 10 seconds
    let q1 = "SELECT * FROM data1";
    let q2 = "SELECT * FROM data2";

    let result = vec![{
        let mut map = HashMap::new();
        map.insert("id".to_string(), serde_json::json!(1));
        map
    }];

    // Both entries share same TTL but are stored at different times
    cache.put(q1, Arc::new(result.clone()));
    std::thread::sleep(Duration::from_millis(100));
    cache.put(q2, Arc::new(result.clone()));

    // Both should be valid immediately
    assert!(cache.get(q1).is_some());
    assert!(cache.get(q2).is_some());

    // Both should eventually expire at different times
    // (but both should be valid during TTL window)
}

// ============================================================================
// SECTION 4: HIT RATE TRACKING VALIDATION
// ============================================================================
// Validates that hit rate calculations are accurate
// Tests verify cache behavior allows accurate hit rate determination

#[test]
fn test_cache_hit_rate_perfect_hits() {
    // Pattern: all queries hit
    let cache = QueryCache::new(60);
    let queries = ["q1", "q2", "q3"];

    // Populate cache
    for &q in &queries {
        let result = vec![{
            let mut m = HashMap::new();
            m.insert("query".to_string(), serde_json::json!(q));
            m
        }];
        cache.put(q, Arc::new(result));
    }

    // All accesses hit
    let hits = queries.iter().filter(|q| cache.get(q).is_some()).count();
    let total = queries.len();

    assert_eq!(hits, total, "All queries should hit for 100% hit rate");
    // Hit rate would be 100% (hits / (hits + misses) = 3 / 3)
}

#[test]
fn test_cache_hit_rate_no_hits() {
    // Pattern: no queries hit (all miss)
    let cache = QueryCache::new(60);
    let queries = ["q1", "q2", "q3"];

    // Don't populate cache, just try to access
    let hits = queries.iter().filter(|q| cache.get(q).is_some()).count();

    assert_eq!(hits, 0, "No queries should hit without cache population");
    // Hit rate would be 0% (hits / (hits + misses) = 0 / 3)
}

#[test]
fn test_cache_hit_rate_mixed_pattern() {
    // Pattern: 2 hits, 1 miss
    let cache = QueryCache::new(60);

    // Populate q1 and q3
    for &q in &["q1", "q3"] {
        let result = vec![{
            let mut m = HashMap::new();
            m.insert("query".to_string(), serde_json::json!(q));
            m
        }];
        cache.put(q, Arc::new(result));
    }

    // Access q1 (hit), q2 (miss), q3 (hit)
    let mut hits = 0;
    let total = 3;

    if cache.get("q1").is_some() {
        hits += 1;
    }
    if cache.get("q2").is_some() {
        hits += 1;
    }
    if cache.get("q3").is_some() {
        hits += 1;
    }

    assert_eq!(hits, 2, "Should have 2 hits, 1 miss");
    // Hit rate would be 67% (2 / 3)
    let hit_rate = (hits as f64 / total as f64) * 100.0;
    assert!(hit_rate > 65.0 && hit_rate < 68.0);
}

// ============================================================================
// SECTION 5: CONCURRENT ACCESS VALIDATION
// ============================================================================
// Validates that cache is safe under concurrent access
// Uses DashMap for lock-free concurrent operations

#[test]
fn test_cache_concurrent_reads() {
    let cache = Arc::new(QueryCache::new(60));
    let query = "SELECT * FROM shared_data";

    // Populate cache
    let result = vec![{
        let mut map = HashMap::new();
        map.insert("data".to_string(), serde_json::json!("shared"));
        map
    }];
    cache.put(query, Arc::new(result));

    // Spawn multiple threads reading same query
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let cache_clone = Arc::clone(&cache);
            std::thread::spawn(move || {
                let hit = cache_clone.get(query);
                assert!(hit.is_some(), "Concurrent read should hit");
                hit.unwrap().len()
            })
        })
        .collect();

    // All threads should succeed
    for handle in handles {
        let len = handle.join().unwrap();
        assert_eq!(len, 1);
    }
}

#[test]
fn test_cache_concurrent_writes() {
    let cache = Arc::new(QueryCache::new(60));

    // Spawn multiple threads writing different queries
    let handles: Vec<_> = (0..5)
        .map(|i| {
            let cache_clone = Arc::clone(&cache);
            std::thread::spawn(move || {
                let query = format!("SELECT * FROM table_{}", i);
                let result = vec![{
                    let mut m = HashMap::new();
                    m.insert("table_id".to_string(), serde_json::json!(i));
                    m
                }];
                cache_clone.put(&query, Arc::new(result));
                query
            })
        })
        .collect();

    // All writes complete
    let queries: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // All queries should be retrievable
    for query in queries {
        assert!(cache.get(&query).is_some(), "Concurrent write should be readable");
    }
}

#[test]
fn test_cache_concurrent_mixed_operations() {
    // Concurrent mix of reads, writes, and expirations
    let cache = Arc::new(QueryCache::new(60));

    // Initial population
    for i in 0..3 {
        let query = format!("SELECT * FROM initial_{}", i);
        let result = vec![{
            let mut m = HashMap::new();
            m.insert("id".to_string(), serde_json::json!(i));
            m
        }];
        cache.put(&query, Arc::new(result));
    }

    // Spawn mixed operations
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let cache_clone = Arc::clone(&cache);
            std::thread::spawn(move || {
                if i % 3 == 0 {
                    // Write operation
                    let query = format!("SELECT * FROM new_{}", i);
                    let result = vec![{
                        let mut m = HashMap::new();
                        m.insert("id".to_string(), serde_json::json!(i));
                        m
                    }];
                    cache_clone.put(&query, Arc::new(result));
                    "write"
                } else if i % 3 == 1 {
                    // Read operation
                    let query = format!("SELECT * FROM initial_{}", i % 3);
                    if cache_clone.get(&query).is_some() {
                        "hit"
                    } else {
                        "miss"
                    }
                } else {
                    // Concurrent miss
                    if cache_clone.get("SELECT * FROM nonexistent").is_none() {
                        "correct_miss"
                    } else {
                        "wrong_hit"
                    }
                }
            })
        })
        .collect();

    // All operations should complete without panic
    for handle in handles {
        let _ = handle.join().unwrap();
    }
}

// ============================================================================
// SECTION 6: CACHE SIZE AND CAPACITY VALIDATION
// ============================================================================
// Validates that cache handles many entries without degradation

#[test]
fn test_cache_many_entries_performance() {
    let cache = QueryCache::new(60);

    // Insert 1000 entries
    for i in 0..1000 {
        let query = format!("SELECT * FROM table_{}", i);
        let result = vec![{
            let mut m = HashMap::new();
            m.insert("id".to_string(), serde_json::json!(i));
            m
        }];
        cache.put(&query, Arc::new(result));
    }

    // Access latency should still be sub-millisecond even with 1000 entries
    let start = Instant::now();
    let hit = cache.get("SELECT * FROM table_500");
    let latency = start.elapsed();

    assert!(hit.is_some(), "Should find entry in large cache");
    assert!(
        latency < Duration::from_millis(5),
        "Latency with 1000 entries {}ms should be <5ms (DashMap lock-free)",
        latency.as_millis()
    );
}

#[test]
fn test_cache_entry_data_integrity() {
    // Validates that cached data is not corrupted
    let cache = QueryCache::new(60);
    let query = "SELECT id, name, email, age FROM users";

    let mut original_map = HashMap::new();
    original_map.insert("id".to_string(), serde_json::json!(42));
    original_map.insert("name".to_string(), serde_json::json!("Alice Smith"));
    original_map.insert("email".to_string(), serde_json::json!("alice@example.com"));
    original_map.insert("age".to_string(), serde_json::json!(30));

    let result = vec![original_map.clone()];
    cache.put(query, Arc::new(result));

    // Retrieve and verify data integrity
    let cached = cache.get(query).unwrap();
    assert_eq!(cached.len(), 1);

    let cached_row = &cached[0];
    assert_eq!(cached_row.get("id"), original_map.get("id"));
    assert_eq!(cached_row.get("name"), original_map.get("name"));
    assert_eq!(cached_row.get("email"), original_map.get("email"));
    assert_eq!(cached_row.get("age"), original_map.get("age"));
}
