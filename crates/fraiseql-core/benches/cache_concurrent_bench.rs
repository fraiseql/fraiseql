#![allow(
    clippy::unwrap_used,
    clippy::missing_docs_in_private_items,
    missing_docs
)] // Reason: benchmark code

//! Cache concurrency benchmark.
//!
//! Measures read throughput, write throughput, and mixed (90/10) throughput
//! of [`QueryResultCache`] at increasing thread counts to demonstrate
//! concurrency scaling.
//!
//! # Running
//!
//! ```bash
//! cargo bench -p fraiseql-core --bench cache_concurrent_bench
//!
//! # Save a baseline for regression comparison:
//! cargo bench -p fraiseql-core --bench cache_concurrent_bench \
//!   -- --save-baseline cache-v1
//!
//! # Compare against baseline:
//! critcmp cache-v1 cache-v2
//! ```

use std::{sync::Arc, thread};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use fraiseql_core::cache::{CacheConfig, QueryResultCache};
use fraiseql_db::JsonbValue;
use rand::Rng;

/// Number of distinct keys to use in the benchmark.
const KEY_COUNT: usize = 1_000;
/// Operations per thread per iteration.
const OPS_PER_THREAD: usize = 1_000;

fn make_cache() -> Arc<QueryResultCache> {
    Arc::new(QueryResultCache::new(CacheConfig::with_max_entries(10_000)))
}

fn make_result() -> Vec<JsonbValue> {
    vec![serde_json::from_str(r#"{"data": {"id": 1, "name": "test"}}"#).unwrap()]
}

/// Pre-populate the cache with `KEY_COUNT` entries.
fn populate(cache: &QueryResultCache) {
    for i in 0..KEY_COUNT {
        let _ = cache.put(i as u64, make_result(), vec!["users".to_string()], None, Some("users"));
    }
}

// ============================================================================
// Read benchmark
// ============================================================================

fn bench_cache_reads(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_concurrent_reads");
    group.throughput(Throughput::Elements(OPS_PER_THREAD as u64));

    for &n_threads in &[1usize, 4, 8, 16, 32] {
        group.bench_with_input(
            BenchmarkId::from_parameter(n_threads),
            &n_threads,
            |b, &threads| {
                let cache = make_cache();
                populate(&cache);

                b.iter(|| {
                    let handles: Vec<_> = (0..threads)
                        .map(|_| {
                            let c: Arc<QueryResultCache> = Arc::clone(&cache);
                            thread::spawn(move || {
                                for i in 0..OPS_PER_THREAD {
                                    let _ = c.get((i % KEY_COUNT) as u64);
                                }
                            })
                        })
                        .collect();
                    for h in handles {
                        h.join().unwrap();
                    }
                });
            },
        );
    }
    group.finish();
}

// ============================================================================
// Write benchmark
// ============================================================================

fn bench_cache_writes(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_concurrent_writes");
    group.throughput(Throughput::Elements(OPS_PER_THREAD as u64));

    for &n_threads in &[1usize, 4, 8, 16, 32] {
        group.bench_with_input(
            BenchmarkId::from_parameter(n_threads),
            &n_threads,
            |b, &threads| {
                let cache = make_cache();
                let result = Arc::new(make_result());

                b.iter(|| {
                    let handles: Vec<_> = (0..threads)
                        .map(|t| {
                            let c = Arc::clone(&cache);
                            let r = Arc::clone(&result);
                            thread::spawn(move || {
                                for i in 0..OPS_PER_THREAD {
                                    // Use thread-local keys to avoid false sharing
                                    let _ = c.put(
                                        ((t * OPS_PER_THREAD + i) % KEY_COUNT) as u64,
                                        (*r).clone(),
                                        vec!["users".to_string()],
                                        None,
                                        Some("users"),
                                    );
                                }
                            })
                        })
                        .collect();
                    for h in handles {
                        h.join().unwrap();
                    }
                });
            },
        );
    }
    group.finish();
}

// ============================================================================
// Mixed 90% read / 10% write benchmark
// ============================================================================

fn bench_cache_mixed(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_concurrent_mixed_90r_10w");
    group.throughput(Throughput::Elements(OPS_PER_THREAD as u64));

    // Fixed at 8 threads (realistic server concurrency)
    let n_threads = 8usize;
    group.bench_function("8_threads", |b| {
        let cache = make_cache();
        populate(&cache);
        let result = Arc::new(make_result());

        b.iter(|| {
            let handles: Vec<_> = (0..n_threads)
                .map(|t| {
                    let c = Arc::clone(&cache);
                    let r = Arc::clone(&result);
                    thread::spawn(move || {
                        for i in 0..OPS_PER_THREAD {
                            // 90% reads, 10% writes
                            if (t * OPS_PER_THREAD + i).is_multiple_of(10) {
                                let _ = c.put(
                                    (i % KEY_COUNT) as u64,
                                    (*r).clone(),
                                    vec!["users".to_string()],
                                    None,
                                    Some("users"),
                                );
                            } else {
                                let _ = c.get((i % KEY_COUNT) as u64);
                            }
                        }
                    })
                })
                .collect();
            for h in handles {
                h.join().unwrap();
            }
        });
    });
    group.finish();
}

// ============================================================================
// Latency benchmark (P50/P99 at 8 threads, steady-state)
// ============================================================================

fn bench_cache_latency(c: &mut Criterion) {
    let cache = make_cache();
    populate(&cache);

    let mut group = c.benchmark_group("cache_get_latency_steady_state");
    // Measure per-operation latency (not throughput)
    group.bench_function("single_get", |b| {
        let mut rng = rand::thread_rng();
        b.iter(|| {
            let _ = cache.get(rng.gen_range(0..KEY_COUNT) as u64);
        });
    });
    group.bench_function("single_put", |b| {
        let mut rng = rand::thread_rng();
        b.iter(|| {
            let _ = cache.put(
                rng.gen_range(0..KEY_COUNT) as u64,
                make_result(),
                vec!["users".to_string()],
                None,
                Some("users"),
            );
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_cache_reads,
    bench_cache_writes,
    bench_cache_mixed,
    bench_cache_latency
);
criterion_main!(benches);
