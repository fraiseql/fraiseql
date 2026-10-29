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
use fraiseql_core::cache::{CacheConfig, QueryResultCache, ViewName};
use fraiseql_db::JsonbValue;

/// An enabled cache of `max_entries`: `CacheConfig::with_max_entries` is disabled, and a
/// disabled cache answers every `get` before reaching the store.
const fn enabled(max_entries: usize) -> CacheConfig {
    CacheConfig {
        enabled: true,
        ..CacheConfig::with_max_entries(max_entries)
    }
}

fn make_result() -> Vec<JsonbValue> {
    vec![serde_json::from_str(r#"{"data": {"id": 1, "name": "bench"}}"#).unwrap()]
}

fn bench_cache_put_get(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_latency");

    // Single put + immediate hit (cold → warm)
    group.bench_function(BenchmarkId::new("put_hit", "single"), |b| {
        let cache = QueryResultCache::new(enabled(1_000));
        let result = make_result();
        let mut i: u64 = 0;
        b.iter(|| {
            let key = i;
            i = i.wrapping_add(1);
            cache
                .put(key, result.clone(), vec!["v_user".to_string()], None, Some("User"), None)
                .unwrap();
            let _ = cache.get(key).unwrap();
        });
    });

    // Cache miss path (key never inserted)
    group.bench_function(BenchmarkId::new("miss", "cold"), |b| {
        let cache = QueryResultCache::new(enabled(1_000));
        b.iter(|| {
            let _ = cache.get(criterion::black_box(u64::MAX)).unwrap();
        });
    });

    // Hot read path: pre-populated cache, always-hit
    group.bench_function(BenchmarkId::new("get", "hot"), |b| {
        let cache = QueryResultCache::new(enabled(10_000));
        let result = make_result();
        // Warm up 100 entries
        for i in 0..100_u64 {
            cache
                .put(i, result.clone(), vec!["v_user".to_string()], None, None, None)
                .unwrap();
        }
        let mut i: u64 = 0;
        b.iter(|| {
            let key = i % 100;
            i = i.wrapping_add(1);
            let _ = cache.get(criterion::black_box(key)).unwrap();
        });
    });

    // Invalidate by view
    // The sweep alone: the 100 puts are the batch's setup, not timed.
    group.bench_function(BenchmarkId::new("invalidate_view", "100_entries"), |b| {
        let result = make_result();
        b.iter_batched(
            || {
                let cache = QueryResultCache::new(enabled(10_000));
                for i in 0..100_u64 {
                    cache
                        .put(i, result.clone(), vec!["v_user".to_string()], None, None, None)
                        .unwrap();
                }
                cache
            },
            |cache| {
                let _ = cache.invalidate_views(&[ViewName::from("v_user")]);
                cache
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

/// A list of `rows` rows of `User`, each with a string `id`.
fn make_list(rows: usize) -> Vec<JsonbValue> {
    (0..rows)
        .map(|i| JsonbValue::new(serde_json::json!({"id": format!("u{i}"), "name": "bench"})))
        .collect()
}

/// A hit on an entry of 1,000 rows tagged by entity: quiet, then while another thread
/// evicts rows the entry does not hold, of its type and of another.
fn bench_list_hit(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_list_hit");
    let fill = |cache: &QueryResultCache| {
        let fence = cache.invalidation_generation();
        cache
            .put(7, make_list(1_000), vec!["v_user".to_string()], None, Some("User"), Some(fence))
            .unwrap();
    };

    group.bench_function(BenchmarkId::new("rows_1000", "quiet"), |b| {
        let cache = QueryResultCache::new(enabled(1_000));
        fill(&cache);
        b.iter(|| {
            assert!(cache.get(criterion::black_box(7)).unwrap().is_some(), "the list is a hit");
        });
    });

    group.bench_function(BenchmarkId::new("rows_1000", "under_writes"), |b| {
        let cache = std::sync::Arc::new(QueryResultCache::new(enabled(1_000)));
        fill(&cache);
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writer = {
            let (cache, stop) = (std::sync::Arc::clone(&cache), std::sync::Arc::clone(&stop));
            std::thread::spawn(move || {
                let mut i: u64 = 0;
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    // Rows the entry does not hold: it stays served.
                    cache.invalidate_by_entity("User", &format!("w{i}")).unwrap();
                    cache.invalidate_by_entity("Post", &format!("p{i}")).unwrap();
                    i = i.wrapping_add(1);
                }
            })
        };
        b.iter(|| {
            assert!(cache.get(criterion::black_box(7)).unwrap().is_some(), "the list is a hit");
        });
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        writer.join().unwrap();
    });

    group.finish();
}

criterion_group!(benches, bench_cache_put_get, bench_list_hit);
criterion_main!(benches);
