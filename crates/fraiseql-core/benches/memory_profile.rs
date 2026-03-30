#![allow(clippy::unwrap_used)] // Reason: benchmark setup code, panics acceptable
#![allow(missing_docs)] // Reason: test harness binary, no public API

//! Memory profiling benchmarks using dhat.
//!
//! Run with: `cargo test --bench memory_profile -p fraiseql-core --features dhat-heap -- --nocapture --test-threads=1`
//!
//! All dhat tests MUST live in this single file (one `#[global_allocator]` per binary).
//! Each `Profiler::builder().testing().build()` resets counters for its scope.
//! Tests MUST run sequentially (`--test-threads=1`) because only one dhat `Profiler`
//! can be active at a time per process.

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

use fraiseql_core::cache::{CacheConfig, QueryResultCache};
use fraiseql_core::runtime::ResultProjector;
use fraiseql_db::JsonbValue;
use serde_json::json;

fn make_cache_result() -> Vec<JsonbValue> {
    vec![JsonbValue::new(json!({"id": 1, "name": "bench"}))]
}

fn generate_sample_data(field_count: usize, row_count: usize) -> Vec<JsonbValue> {
    let mut rows = Vec::with_capacity(row_count);
    for row_id in 0..row_count {
        let mut obj = serde_json::Map::new();
        for field_id in 0..field_count {
            obj.insert(format!("field_{field_id}"), json!("value"));
        }
        obj.insert("id".to_string(), json!(row_id.to_string()));
        obj.insert("name".to_string(), json!(format!("User {row_id}")));
        obj.insert("email".to_string(), json!(format!("user{row_id}@example.com")));
        obj.insert("status".to_string(), json!("active"));
        rows.push(JsonbValue::new(serde_json::Value::Object(obj)));
    }
    rows
}

/// Asserts dhat heap stats are within bounds and prints them for CI capture.
///
/// # Panics
///
/// Panics if peak heap or total allocations exceed the given ceilings.
#[cfg(feature = "dhat-heap")]
fn assert_heap_bounds(label: &str, max_peak_bytes: usize, max_total_bytes: u64) {
    let stats = dhat::HeapStats::get();
    eprintln!(
        "DHAT[{label}]: peak_heap={} total_alloc={} total_blocks={}",
        stats.max_bytes, stats.total_bytes, stats.total_blocks
    );
    assert!(
        stats.max_bytes < max_peak_bytes,
        "DHAT[{label}]: peak heap {} exceeds {max_peak_bytes}",
        stats.max_bytes
    );
    assert!(
        stats.total_bytes < max_total_bytes,
        "DHAT[{label}]: total alloc {} exceeds {max_total_bytes}",
        stats.total_bytes
    );
}

#[test]
fn profile_cache_put_get() {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::builder().testing().build();

    let cache = QueryResultCache::new(CacheConfig::with_max_entries(10_000));
    let result = make_cache_result();

    // 1000 put + get cycles
    for i in 0_u64..1000 {
        cache
            .put(
                i,
                result.clone(),
                vec!["view_a".to_string()],
                None,
                Some("User"),
            )
            .unwrap();
    }
    for i in 0_u64..1000 {
        let _ = cache.get(i);
    }

    // 50 MB peak, 200 MB total
    #[cfg(feature = "dhat-heap")]
    assert_heap_bounds("cache_put_get", 50_000_000, 200_000_000);
}

#[test]
fn profile_sql_projection() {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::builder().testing().build();

    // Project 50 fields from 10K rows
    let fields: Vec<String> = (0..50).map(|i| format!("field_{i}")).collect();
    let data = generate_sample_data(50, 10_000);
    let projector = ResultProjector::new(fields);

    let _projected = projector.project_results(&data, false).unwrap();

    // 100 MB peak, 500 MB total
    #[cfg(feature = "dhat-heap")]
    assert_heap_bounds("sql_projection", 100_000_000, 500_000_000);
}
