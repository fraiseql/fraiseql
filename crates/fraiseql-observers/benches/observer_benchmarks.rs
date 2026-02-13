//! Performance Benchmarks for FraiseQL Observer System
//!
//! These benchmarks measure:
//! - Event processing throughput
//! - Deduplication overhead
//! - Cache hit/miss performance
//! - Concurrent vs sequential execution
//!
//! **Run benchmarks**:
//! ```bash
//! # Basic benchmarks
//! cargo bench --bench observer_benchmarks
//!
//! # With Redis features
//! cargo bench --bench observer_benchmarks --features "postgres,dedup,caching"
//! ```
#![allow(missing_docs)]

use std::sync::Arc;

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
// Only available with testing feature - provide simple mock if not available
#[cfg(feature = "testing")]
use fraiseql_observers::testing::mocks::MockDeadLetterQueue;
use fraiseql_observers::{
    event::{EntityEvent, EventKind},
    executor::ObserverExecutor,
    matcher::EventMatcher,
};
use serde_json::json;
use uuid::Uuid;

#[cfg(not(feature = "testing"))]
mod mock_dlq {
    use async_trait::async_trait;
    use fraiseql_observers::{
        Result,
        config::ActionConfig,
        event::EntityEvent,
        traits::{DeadLetterQueue, DlqItem},
    };
    use uuid::Uuid;

    #[derive(Clone)]
    pub struct MockDeadLetterQueue;

    impl MockDeadLetterQueue {
        pub fn new() -> Self {
            Self
        }
    }

    #[async_trait]
    impl DeadLetterQueue for MockDeadLetterQueue {
        async fn push(
            &self,
            _event: EntityEvent,
            _action: ActionConfig,
            _error: String,
        ) -> Result<Uuid> {
            Ok(Uuid::new_v4())
        }

        async fn get_pending(&self, _limit: i64) -> Result<Vec<DlqItem>> {
            Ok(vec![])
        }

        async fn mark_success(&self, _id: Uuid) -> Result<()> {
            Ok(())
        }

        async fn mark_retry_failed(&self, _id: Uuid, _error: &str) -> Result<()> {
            Ok(())
        }
    }
}

#[cfg(not(feature = "testing"))]
use mock_dlq::MockDeadLetterQueue;

// ============================================================================
// Benchmark Utilities
// ============================================================================

fn create_test_event(kind: EventKind, entity_type: &str, data: serde_json::Value) -> EntityEvent {
    EntityEvent::new(kind, entity_type.to_string(), Uuid::new_v4(), data)
}

fn setup_executor() -> ObserverExecutor {
    let matcher = EventMatcher::new();
    let dlq = Arc::new(MockDeadLetterQueue::new());
    ObserverExecutor::new(matcher, dlq)
}

// ============================================================================
// Benchmark 1: Event Processing Baseline
// ============================================================================

/// Benchmarks single and batch event processing throughput
///
/// This benchmark group measures the core event processing pipeline:
///
/// - **Single event**: Processing cost for individual events
/// - **Batch sizes** (10, 50, 100): Measures throughput scaling with batch size
///
/// Results help identify:
/// - Maximum events/second the executor can sustain
/// - Whether throughput scales linearly with batch size
/// - Cost of event loop overhead
///
/// Baseline expectation: 10K+ events/second for typical payloads
fn benchmark_event_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_processing");

    // Benchmark single event processing
    group.bench_function("process_single_event", |b| {
        let executor = setup_executor();
        let event = create_test_event(EventKind::Created, "User", json!({"name": "Alice"}));

        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async { black_box(executor.process_event(&event).await) });
    });

    // Benchmark batch event processing
    for batch_size in &[10, 50, 100] {
        group.bench_with_input(
            BenchmarkId::new("process_batch", batch_size),
            batch_size,
            |b, &size| {
                let executor = Arc::new(setup_executor());
                let events: Vec<EntityEvent> = (0..size)
                    .map(|i| create_test_event(EventKind::Created, "User", json!({"id": i})))
                    .collect();

                b.to_async(tokio::runtime::Runtime::new().unwrap()).iter(|| async {
                    let executor = executor.clone();
                    let events = events.clone();
                    for event in events {
                        black_box(executor.process_event(&event).await).ok();
                    }
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Benchmark 2: Concurrent vs Sequential Execution
// ============================================================================

/// Benchmarks sequential vs. concurrent event processing
///
/// Compares processing 20 events in two modes:
///
/// - **Sequential**: Events processed one-by-one (baseline)
/// - **Concurrent**: Events spawned as async tasks and awaited
///
/// This measures the cost of async task spawning and the speedup from
/// true parallelization (if executor can parallelize event processing).
///
/// Performance characteristics:
/// - Concurrent should be faster if event processing involves I/O or blocking
/// - Results validate tokio runtime overhead is justified
/// - Helps identify contention in shared executor state
fn benchmark_concurrent_execution(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_vs_sequential");
    let event_count = 20;

    // Sequential execution
    group.bench_function("sequential_execution", |b| {
        let executor = setup_executor();
        let events: Vec<EntityEvent> = (0..event_count)
            .map(|i| create_test_event(EventKind::Created, "User", json!({"id": i})))
            .collect();

        b.to_async(tokio::runtime::Runtime::new().unwrap()).iter(|| async {
            for event in &events {
                black_box(executor.process_event(event).await).ok();
            }
        });
    });

    // Concurrent execution
    group.bench_function("concurrent_execution", |b| {
        let executor = Arc::new(setup_executor());
        let events: Vec<EntityEvent> = (0..event_count)
            .map(|i| create_test_event(EventKind::Created, "User", json!({"id": i})))
            .collect();

        b.to_async(tokio::runtime::Runtime::new().unwrap()).iter(|| async {
            let executor = executor.clone();
            let events = events.clone();
            let mut tasks = Vec::new();

            for event in events {
                let executor_clone = executor.clone();
                tasks.push(tokio::spawn(async move { executor_clone.process_event(&event).await }));
            }

            for task in tasks {
                black_box(task.await).ok();
            }
        });
    });

    group.finish();
}

// ============================================================================
// Benchmark 3: Event Matching Performance
// ============================================================================

/// Benchmarks observer pattern matching overhead with no active observers
///
/// Measures the baseline cost of event processing when there are no observers.
/// This isolates the matching engine overhead from any observer-specific costs.
///
/// Expected behavior:
/// - Should be very fast (microseconds per event)
/// - Baseline for comparing with populated observer sets
/// - Validates that no-observer case doesn't have hidden scans
fn benchmark_event_matching(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_matching");

    // Benchmark with no observers (baseline)
    group.bench_function("match_no_observers", |b| {
        let executor = setup_executor();
        let event = create_test_event(EventKind::Created, "User", json!({"name": "Alice"}));

        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async { black_box(executor.process_event(&event).await) });
    });

    group.finish();
}

// ============================================================================
// Benchmark 4: Event Creation Overhead
// ============================================================================

/// Benchmarks event object construction cost with varying complexity
///
/// Separates event creation overhead from event processing:
///
/// - **Simple event**: Single string field (minimal JSON payload)
/// - **Complex event**: Order with nested items array (realistic structure)
///
/// This helps identify:
/// - JSON serialization cost within event creation
/// - Whether payload complexity affects baseline object creation
/// - Cost attribution: creation vs. processing
fn benchmark_event_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_creation");

    group.bench_function("create_simple_event", |b| {
        b.iter(|| {
            black_box(create_test_event(EventKind::Created, "User", json!({"name": "Alice"})))
        });
    });

    group.bench_function("create_complex_event", |b| {
        b.iter(|| {
            black_box(create_test_event(
                EventKind::Updated,
                "Order",
                json!({
                    "id": 123,
                    "customer_id": 456,
                    "items": [
                        {"product_id": 1, "quantity": 2, "price": 19.99},
                        {"product_id": 2, "quantity": 1, "price": 49.99},
                    ],
                    "total": 89.97,
                    "status": "pending",
                }),
            ))
        });
    });

    group.finish();
}

// ============================================================================
// Benchmark 5: Throughput Measurement
// ============================================================================

/// Benchmarks sustained event processing throughput at scale
///
/// Measures events/second capacity across production-relevant batch sizes:
///
/// - **100 events**: Small batch, validates startup efficiency
/// - **500 events**: Medium batch, typical production workload
/// - **1000 events**: Large batch, stress tests event loop
///
/// This directly measures production capacity:
/// - Helps capacity planning (events/second = `1_000_000` / `avg_µs_per_event`)
/// - Identifies throughput ceiling under sustained load
/// - Validates batching strategy effectiveness
fn benchmark_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput");

    for event_count in &[100, 500, 1000] {
        #[allow(clippy::cast_sign_loss)] // event_count values are all positive
        group.throughput(Throughput::Elements(*event_count as u64));
        group.bench_with_input(
            BenchmarkId::new("events_per_second", event_count),
            event_count,
            |b, &count| {
                let executor = Arc::new(setup_executor());
                let events: Vec<EntityEvent> = (0..count)
                    .map(|i| create_test_event(EventKind::Created, "User", json!({"id": i})))
                    .collect();

                b.to_async(tokio::runtime::Runtime::new().unwrap()).iter(|| async {
                    let executor = executor.clone();
                    let events = events.clone();
                    for event in events {
                        black_box(executor.process_event(&event).await).ok();
                    }
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Benchmark Groups
// ============================================================================

// **Benchmark Groups Overview**
//
// This benchmark module orchestrates performance measurements across five critical areas
// of the observer system:
//
// 1. **Event Processing** (`benchmark_event_processing`) - Single and batch event throughput
//    - Tests 10, 50, and 100 event batches to measure scaling efficiency
//    - Validates baseline event processing performance
//
// 2. **Concurrent Execution** (`benchmark_concurrent_execution`) - Sequential vs. concurrent
//    - Compares single-threaded vs. async concurrent processing of 20 events
//    - Measures speedup from task parallelization
//
// 3. **Event Matching** (`benchmark_event_matching`) - Observer pattern matching overhead
//    - Baseline with no active observers
//    - Identifies matching engine overhead
//
// 4. **Event Creation** (`benchmark_event_creation`) - Object construction cost
//    - Simple events (minimal JSON payload)
//    - Complex events (realistic Order objects with nested items)
//    - Distinguishes serialization overhead from processing
//
// 5. **Throughput Measurement** (`benchmark_throughput`) - Sustained event rates at scale
//    - Tests 100, 500, and 1000 event batches
//    - Measures events/second capacity for production planning
//
// ## Interpretation Guide
//
// These benchmarks help identify:
// - **Throughput ceiling** - Maximum sustained events/second under load
// - **Concurrent benefits** - Speedup factor from parallel event processing
// - **Overhead sources** - Cost breakdown: event creation vs. observer matching
// - **Batch efficiency** - Performance scaling with batch size (linear vs. sublinear)
//
// ## Performance Baselines
//
// These measurements establish regression detection for:
// - P99 event processing latency
// - Minimum throughput for production workloads
// - Impact of feature toggles (caching, deduplication)
//
// ## Running Benchmarks
//
// ```bash
// # All benchmarks
// cargo bench --bench observer_benchmarks
//
// # With optional features
// cargo bench --bench observer_benchmarks --features "postgres,dedup,caching"
//
// # Single benchmark group
// cargo bench --bench observer_benchmarks -- event_processing
// ```

criterion_group!(
    benches,
    benchmark_event_processing,
    benchmark_concurrent_execution,
    benchmark_event_matching,
    benchmark_event_creation,
    benchmark_throughput,
);

criterion_main!(benches);
