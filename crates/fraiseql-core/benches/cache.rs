#![allow(clippy::unwrap_used)] // Reason: benchmark setup code, panics acceptable
#![allow(missing_docs)] // Reason: criterion_group!/criterion_main! macros generate undocumented items

//! Cache single-threaded latency benchmarks.
//!
//! Measures per-operation latency for cache `put` and `get` in a single-threaded
//! context to isolate the overhead of LRU book-keeping and TTL checks from
//! thread contention.  For concurrent throughput see `cache_concurrent_bench`.
//!
//! # Running
//!
//! ```bash
//! cargo bench -p fraiseql-core --bench cache
//!
//! # Save a baseline:
//! cargo bench -p fraiseql-core --bench cache -- --save-baseline cache-st-v1
//! ```

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use fraiseql_core::cache::{CacheConfig, QueryResultCache};
use fraiseql_db::JsonbValue;

fn make_result() -> Vec<JsonbValue> {
    vec![serde_json::from_str(r#"{"data": {"id": 1, "name": "bench"}}"#).unwrap()]
}

fn bench_cache_put_get(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_latency");

    // Single put + immediate hit (cold → warm)
    group.bench_function(BenchmarkId::new("put_hit", "single"), |b| {
        let cache = QueryResultCache::new(CacheConfig::with_max_entries(1_000));
        let result = make_result();
        let mut i: u64 = 0;
        b.iter(|| {
            let key = i;
            i = i.wrapping_add(1);
            cache
                .put(key, result.clone(), vec!["v_user".to_string()], None, Some("User"))
                .unwrap();
            let _ = cache.get(key).unwrap();
        });
    });

    // Cache miss path (key never inserted)
    group.bench_function(BenchmarkId::new("miss", "cold"), |b| {
        let cache = QueryResultCache::new(CacheConfig::with_max_entries(1_000));
        b.iter(|| {
            let _ = cache.get(criterion::black_box(u64::MAX)).unwrap();
        });
    });

    // Hot read path: pre-populated cache, always-hit
    group.bench_function(BenchmarkId::new("get", "hot"), |b| {
        let cache = QueryResultCache::new(CacheConfig::with_max_entries(10_000));
        let result = make_result();
        // Warm up 100 entries
        for i in 0..100_u64 {
            cache.put(i, result.clone(), vec!["v_user".to_string()], None, None).unwrap();
        }
        let mut i: u64 = 0;
        b.iter(|| {
            let key = i % 100;
            i = i.wrapping_add(1);
            let _ = cache.get(criterion::black_box(key)).unwrap();
        });
    });

    // Invalidate by view
    group.bench_function(BenchmarkId::new("invalidate_view", "100_entries"), |b| {
        let result = make_result();
        b.iter(|| {
            let cache = QueryResultCache::new(CacheConfig::with_max_entries(10_000));
            for i in 0..100_u64 {
                cache.put(i, result.clone(), vec!["v_user".to_string()], None, None).unwrap();
            }
            cache.invalidate_views(&["v_user".to_string()]).unwrap();
        });
    });

    group.finish();
}

criterion_group!(benches, bench_cache_put_get);
criterion_main!(benches);
