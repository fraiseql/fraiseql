#![allow(clippy::unwrap_used, clippy::print_stdout, clippy::print_stderr)] // Reason: benchmark setup code, panics acceptable
#![allow(missing_docs)] // Reason: test harness binary, no public API

//! Memory profiling benchmarks using dhat.
//!
//! Run with: `cargo test --bench memory_profile -p fraiseql-core --features dhat-heap --
//! --nocapture --test-threads=1`
//!
//! All dhat tests MUST live in this single file (one `#[global_allocator]` per binary).
//! Each `Profiler::builder().testing().build()` resets counters for its scope.
//! Tests MUST run sequentially (`--test-threads=1`) because only one dhat `Profiler`
//! can be active at a time per process.

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

use std::sync::Arc;

use fraiseql_core::{
    cache::{CacheConfig, QueryResultCache},
    runtime::{Executor, ResultProjector},
    schema::CompiledSchema,
};
use fraiseql_db::JsonbValue;
use serde_json::json;

#[path = "support/fixed_rows.rs"]
mod fixed_rows;
use fixed_rows::{FixedRows, user_row};

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
            .put(i, result.clone(), vec!["view_a".to_string()], None, Some("User"), None)
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

/// Bytes and blocks one warm `users` read allocates, against the bench schema with the
/// `users` definition's description set to `description`.
#[cfg(feature = "dhat-heap")]
fn warm_read_allocations(description: Option<String>) -> (u64, u64) {
    let (bytes, blocks, _) =
        warm_allocations(description, "{ users(limit: 20) { id name email } }");
    (bytes, blocks)
}

/// Bytes and blocks one warm read of `query` allocates, and its response, against the bench
/// schema with the `users` definition's description set to `description`.
#[cfg(feature = "dhat-heap")]
fn warm_allocations(description: Option<String>, query: &str) -> (u64, u64, serde_json::Value) {
    let mut schema = CompiledSchema::from_json(fixed_rows::SCHEMA, false).unwrap();
    let users = schema.queries.iter_mut().find(|q| q.name == "users").unwrap();
    users.description = description;
    let rows = (0..20).map(|i| user_row(i, false)).collect();
    let executor = Executor::new(schema, Arc::new(FixedRows::new(rows)));
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let warm = runtime.block_on(executor.execute(query, None)).unwrap();
    assert!(warm.get("errors").is_none(), "the read must succeed: {warm}");

    let before = dhat::HeapStats::get();
    let out = runtime.block_on(executor.execute(query, None)).unwrap();
    let after = dhat::HeapStats::get();
    (
        after.total_bytes - before.total_bytes,
        after.total_blocks - before.total_blocks,
        out,
    )
}

/// A repeated document reuses what was compiled for it: a warm read allocates the same
/// whatever the size of the query definition it reads, because it copies no part of it.
#[test]
fn a_warm_read_copies_nothing_of_its_query_definition() {
    #[cfg(feature = "dhat-heap")]
    {
        let _profiler = dhat::Profiler::builder().testing().build();
        let (lean_bytes, lean_blocks) = warm_read_allocations(None);
        let (fat_bytes, fat_blocks) = warm_read_allocations(Some("x".repeat(64 * 1024)));
        eprintln!(
            "warm read: lean {lean_bytes} B / {lean_blocks} blocks, \
             64 KiB description {fat_bytes} B / {fat_blocks} blocks"
        );
        assert_eq!(
            fat_bytes,
            lean_bytes,
            "a warm read allocated {} more bytes for a 64 KiB description it never reads: it \
             copies the query definition",
            fat_bytes.saturating_sub(lean_bytes)
        );
    }
}

/// The same selection written through a chain of `n` fragments, each spreading the next
/// and the last selecting the fields: it resolves to exactly the plain selection.
#[cfg(feature = "dhat-heap")]
fn through_fragments(n: usize) -> String {
    use std::fmt::Write as _;
    let mut document = String::from("{ users(limit: 20) { ...F0 } }");
    for i in 0..n - 1 {
        let _ = write!(document, " fragment F{i} on User {{ ...F{} }}", i + 1);
    }
    let _ = write!(document, " fragment F{} on User {{ id name email }}", n - 1);
    document
}

/// A repeated document is compiled once: a warm read allocates the same whether its
/// selection is written plainly or through a chain of fragments that resolves to it,
/// because no per-request stage walks the document as written: not the parse, not
/// spread expansion, and not the depth and cost gate.
#[test]
fn a_warm_read_does_not_parse_its_document_again() {
    #[cfg(feature = "dhat-heap")]
    {
        let _profiler = dhat::Profiler::builder().testing().build();
        let plain = warm_allocations(None, "{ users(limit: 20) { id name email } }");
        let spread = warm_allocations(None, &through_fragments(8));
        assert_eq!(spread.2, plain.2, "both documents select the same fields");
        eprintln!(
            "warm read: plain {} B / {} blocks, 8 chained fragments {} B / {} blocks",
            plain.0, plain.1, spread.0, spread.1
        );
        assert_eq!(
            (spread.0, spread.1),
            (plain.0, plain.1),
            "a warm read of the same selection allocated more for a document written through \
             fragments: a per-request stage still walks the document as written"
        );
    }
}

/// A warm read copies nothing of its selection: it allocates the same whether its nested
/// list selects one field or three, when the list is empty in every row. The two answer
/// the same body, so only a per-request copy of the selection (to classify a level's
/// fields, or to plan its nested reads) could tell them apart.
#[test]
fn a_warm_read_copies_nothing_of_its_selection() {
    #[cfg(feature = "dhat-heap")]
    {
        let _profiler = dhat::Profiler::builder().testing().build();
        let narrow = warm_allocations(None, "{ users(limit: 20) { id posts { id } } }");
        let wide =
            warm_allocations(None, "{ users(limit: 20) { id posts { id title published } } }");
        assert_eq!(wide.2, narrow.2, "both documents answer the same body");
        eprintln!(
            "warm read: posts {{ id }} {} B / {} blocks, posts {{ id title published }} {} B / \
             {} blocks",
            narrow.0, narrow.1, wide.0, wide.1
        );
        assert_eq!(
            (wide.0, wide.1),
            (narrow.0, narrow.1),
            "a warm read allocated more for a wider selection of an empty list: a per-request \
             stage still copies the selection"
        );
    }
}

/// A repeated document is compiled once per set of `@skip`/`@include` outcomes: a warm read
/// allocates the same whether its selection is written once or repeats fields that merge
/// into it (§ 6.3.2), because no per-request stage collects the written fields again.
#[test]
fn a_warm_read_does_not_collect_its_fields_again() {
    #[cfg(feature = "dhat-heap")]
    {
        let _profiler = dhat::Profiler::builder().testing().build();
        let plain = warm_allocations(None, "{ users { id name email } }");
        let repeated =
            warm_allocations(None, &format!("{{ users {{ {} }} }}", "id name email ".repeat(10)));
        assert_eq!(repeated.2, plain.2, "both documents select the same fields");
        eprintln!(
            "warm read: plain {} B / {} blocks, fields written 10 times {} B / {} blocks",
            plain.0, plain.1, repeated.0, repeated.1
        );
        assert_eq!(
            (repeated.0, repeated.1),
            (plain.0, plain.1),
            "a warm read of the same selection allocated more for a document that repeats its \
             fields: a per-request stage still collects the fields as written"
        );
    }
}
