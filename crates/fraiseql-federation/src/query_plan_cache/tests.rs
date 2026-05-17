#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

#[test]
fn test_cache_put_and_get() {
    let cache = QueryPlanCache::new(100);
    let plan = QueryPlan {
        fetches:            vec![SubgraphFetch {
            subgraph:     "users".to_string(),
            query:        "{ user(id: $id) { name } }".to_string(),
            entity_types: vec!["User".to_string()],
            depends_on:   None,
        }],
        schema_fingerprint: "abc123".to_string(),
    };

    cache.put("query GetUser { user { name } }", plan);
    let cached = cache.get("query GetUser { user { name } }", "abc123");
    assert!(cached.is_some());
    assert_eq!(cached.unwrap().fetches.len(), 1);
}

#[test]
fn test_cache_miss_on_different_fingerprint() {
    let cache = QueryPlanCache::new(100);
    let plan = QueryPlan {
        fetches:            vec![],
        schema_fingerprint: "abc123".to_string(),
    };

    cache.put("query { user { name } }", plan);
    let cached = cache.get("query { user { name } }", "different_fingerprint");
    assert!(cached.is_none(), "should not match stale schema fingerprint");
}

#[test]
fn test_cache_eviction() {
    let cache = QueryPlanCache::new(2);
    for i in 0..3 {
        let plan = QueryPlan {
            fetches:            vec![],
            schema_fingerprint: "fp".to_string(),
        };
        cache.put(&format!("query{i}"), plan);
    }

    assert_eq!(cache.len(), 2, "LRU should evict oldest entry");
    assert!(cache.get("query0", "fp").is_none(), "query0 should be evicted");
    assert!(cache.get("query2", "fp").is_some(), "query2 should be present");
}

#[test]
fn test_cache_clear() {
    let cache = QueryPlanCache::new(100);
    let plan = QueryPlan {
        fetches:            vec![],
        schema_fingerprint: "fp".to_string(),
    };
    cache.put("q1", plan);

    assert!(!cache.is_empty());
    cache.clear();
    assert!(cache.is_empty());
}

#[test]
fn test_normalize_query() {
    let q1 = "query  GetUser  {\n  user(id: 1)  {\n    name\n  }\n}";
    let q2 = "query GetUser { user(id: 1) { name } }";
    assert_eq!(normalize_query(q1), normalize_query(q2));
}

#[test]
fn test_schema_fingerprint_deterministic() {
    let fp1 = schema_fingerprint(&[("User", &["id"]), ("Order", &["id"])]);
    let fp2 = schema_fingerprint(&[("User", &["id"]), ("Order", &["id"])]);
    assert_eq!(fp1, fp2);
}

#[test]
fn test_schema_fingerprint_changes_on_key_change() {
    let fp1 = schema_fingerprint(&[("User", &["id"])]);
    let fp2 = schema_fingerprint(&[("User", &["id", "email"])]);
    assert_ne!(fp1, fp2, "fingerprint should change when keys change");
}

#[test]
fn test_schema_fingerprint_order_independent() {
    let fp1 = schema_fingerprint(&[("User", &["id"]), ("Order", &["id"])]);
    let fp2 = schema_fingerprint(&[("Order", &["id"]), ("User", &["id"])]);
    assert_eq!(fp1, fp2, "fingerprint should be order-independent");
}

#[test]
fn test_multi_fetch_plan() {
    let cache = QueryPlanCache::new(100);
    let plan = QueryPlan {
        fetches:            vec![
            SubgraphFetch {
                subgraph:     "users".to_string(),
                query:        "{ user { id } }".to_string(),
                entity_types: vec!["User".to_string()],
                depends_on:   None,
            },
            SubgraphFetch {
                subgraph:     "orders".to_string(),
                query:        "{ orders { id } }".to_string(),
                entity_types: vec!["Order".to_string()],
                depends_on:   Some(0),
            },
        ],
        schema_fingerprint: "fp".to_string(),
    };

    cache.put("query { user { orders { id } } }", plan);
    let cached = cache.get("query { user { orders { id } } }", "fp").unwrap();
    assert_eq!(cached.fetches.len(), 2);
    assert_eq!(cached.fetches[1].depends_on, Some(0));
}
