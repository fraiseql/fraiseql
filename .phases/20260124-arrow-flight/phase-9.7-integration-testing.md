# Phase 9.7: Integration & Performance Testing

**Duration**: 3-4 days
**Priority**: ⭐⭐⭐⭐⭐
**Dependencies**: Phases 9.1-9.6 complete
**Status**: Ready to implement (after 9.6)

---

## Objective

Validate the complete Arrow Flight integration with comprehensive testing:
- **End-to-end pipeline tests** (GraphQL → Arrow → ClickHouse/Elasticsearch)
- **Performance benchmarks** (HTTP/JSON vs Arrow Flight)
- **Stress testing** (1M+ rows, sustained load)
- **Chaos testing** (ClickHouse/Elasticsearch failures, network issues)
- **Regression prevention** (ensure HTTP/JSON still works)

**Success Metric**: Arrow Flight is 50x faster than HTTP/JSON for 100k+ row queries with zero regressions.

---

## Test Categories

### 1. Integration Tests
- GraphQL query → Arrow → Client deserialization
- Observer events → NATS → Arrow → ClickHouse
- Observer events → NATS → JSONB → Elasticsearch
- Dual dataplane validation (both running simultaneously)

### 2. Performance Benchmarks
- Throughput (rows/sec)
- Latency (ms per query)
- Memory usage (constant vs growing)
- CPU utilization

### 3. Stress Tests
- 1M row queries
- Sustained load (10k events/sec for 1 hour)
- Concurrent clients (100+ simultaneous connections)

### 4. Chaos Tests
- ClickHouse crashes mid-stream
- Elasticsearch unavailable
- Network partitions (NATS down)
- Redis cache failures

---

## Files to Create

### Test Files
1. `tests/e2e/arrow_flight_pipeline_test.rs`
2. `tests/performance/arrow_vs_http_benchmark.rs`
3. `tests/stress/million_row_test.rs`
4. `tests/chaos/failure_scenarios_test.rs`

### Benchmarking
5. `benches/arrow_flight_benchmarks.rs`

### Test Infrastructure
6. `tests/support/test_harness.rs`
7. `docker-compose.test.yml`

---

## Implementation Steps

### Step 1: End-to-End Pipeline Test (1-2 hours)

**File**: `tests/e2e/arrow_flight_pipeline_test.rs`

```rust
//! End-to-end test: GraphQL → Arrow Flight → Python client

use arrow_flight::{flight_service_client::FlightServiceClient, Ticket};
use fraiseql_arrow::FlightTicket;
use std::process::Command;

#[tokio::test]
async fn test_graphql_to_arrow_to_client() {
    // 1. Start FraiseQL server (HTTP + Arrow Flight)
    let _server = start_test_server().await;

    // 2. Execute GraphQL query via Arrow Flight
    let mut client = FlightServiceClient::connect("http://localhost:50051")
        .await
        .expect("Failed to connect");

    let ticket = FlightTicket::GraphQLQuery {
        query: "{ users(limit: 1000) { id name email } }".to_string(),
        variables: None,
    };

    let mut stream = client
        .do_get(Ticket {
            ticket: ticket.encode().unwrap(),
        })
        .await
        .expect("DoGet failed")
        .into_inner();

    // 3. Validate Arrow batches
    let mut total_rows = 0;
    while let Some(batch) = stream.message().await.unwrap() {
        // Decode FlightData to RecordBatch
        total_rows += decode_flight_data(batch).num_rows();
    }

    assert!(total_rows > 0, "Should fetch at least 1 row");
    assert!(total_rows <= 1000, "Should respect limit");

    // 4. Verify via Python client (subprocess call)
    let output = Command::new("python3")
        .args(&[
            "examples/python/fraiseql_client.py",
            "query",
            "{ users(limit: 10) { id } }",
        ])
        .output()
        .expect("Failed to run Python client");

    assert!(output.status.success(), "Python client should succeed");
    assert!(output.stdout.len() > 0, "Should have output");
}

#[tokio::test]
async fn test_observer_events_pipeline() {
    // 1. Start infrastructure (NATS, ClickHouse, Elasticsearch)
    start_test_infrastructure().await;

    // 2. Publish test events to NATS
    let event = EntityEvent {
        id: Uuid::new_v4(),
        event_type: "Order.Created".to_string(),
        entity_type: "Order".to_string(),
        entity_id: "test-order-1".to_string(),
        timestamp: Utc::now(),
        data: json!({"total": 100.50}),
        user_id: Some("user-1".to_string()),
        org_id: Some("org-1".to_string()),
    };

    publish_event_to_nats(&event).await;

    // 3. Wait for event to propagate
    tokio::time::sleep(Duration::from_secs(5)).await;

    // 4. Verify in ClickHouse (analytics dataplane)
    let clickhouse_count = query_clickhouse(
        "SELECT count() FROM fraiseql_events WHERE entity_id = 'test-order-1'"
    ).await;
    assert_eq!(clickhouse_count, 1, "Event should be in ClickHouse");

    // 5. Verify in Elasticsearch (operational dataplane)
    let es_results = search_elasticsearch(json!({
        "query": {
            "term": { "entity_id": "test-order-1" }
        }
    })).await;
    assert_eq!(es_results.len(), 1, "Event should be in Elasticsearch");

    // 6. Fetch via Arrow Flight
    let ticket = FlightTicket::ObserverEvents {
        entity_type: "Order".to_string(),
        start_date: None,
        end_date: None,
        limit: Some(1),
    };

    let mut client = FlightServiceClient::connect("http://localhost:50051").await.unwrap();
    let stream = client.do_get(Ticket { ticket: ticket.encode().unwrap() }).await.unwrap();

    // Validate Arrow stream contains the event
    // ... (verify batch data)
}

// Helper functions
async fn start_test_server() -> TestServer { /* ... */ }
async fn start_test_infrastructure() { /* docker-compose up */ }
fn decode_flight_data(data: FlightData) -> RecordBatch { /* ... */ }
```

---

### Step 2: Performance Benchmark (2-3 hours)

**File**: `benches/arrow_flight_benchmarks.rs`

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use fraiseql_test_utils::TestDatabase;

fn benchmark_http_json_query(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let db = rt.block_on(TestDatabase::new());

    let mut group = c.benchmark_group("graphql_queries");

    for size in [100, 1_000, 10_000, 100_000].iter() {
        group.bench_with_input(
            BenchmarkId::new("http_json", size),
            size,
            |b, &size| {
                b.to_async(&rt).iter(|| async {
                    let query = format!("{{ users(limit: {}) {{ id name email }} }}", size);
                    let response = http_client.query(&query).await.unwrap();
                    black_box(response);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("arrow_flight", size),
            size,
            |b, &size| {
                b.to_async(&rt).iter(|| async {
                    let query = format!("{{ users(limit: {}) {{ id name email }} }}", size);
                    let stream = arrow_client.query(&query).await.unwrap();
                    black_box(stream);
                });
            },
        );
    }

    group.finish();
}

fn benchmark_event_streaming(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("event_streaming");

    for count in [1_000, 10_000, 100_000, 1_000_000].iter() {
        group.bench_with_input(
            BenchmarkId::new("arrow_flight", count),
            count,
            |b, &count| {
                b.to_async(&rt).iter(|| async {
                    let ticket = FlightTicket::ObserverEvents {
                        entity_type: "Order".to_string(),
                        start_date: None,
                        end_date: None,
                        limit: Some(count),
                    };
                    let stream = arrow_client.do_get(ticket).await.unwrap();
                    black_box(stream);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, benchmark_http_json_query, benchmark_event_streaming);
criterion_main!(benches);
```

**Run benchmarks**:
```bash
cargo bench --bench arrow_flight_benchmarks

# Expected output:
# http_json/100        time: [5.2 ms ...]
# arrow_flight/100     time: [1.1 ms ...]  (5x faster)
#
# http_json/100000     time: [30 s ...]
# arrow_flight/100000  time: [2 s ...]     (15x faster)
```

---

### Step 3: Stress Test - Million Row Query (1-2 hours)

**File**: `tests/stress/million_row_test.rs`

```rust
#[tokio::test]
#[ignore] // Run manually: cargo test --test million_row_test --ignored
async fn test_million_row_query() {
    // 1. Seed database with 1M rows
    let db = TestDatabase::new().await;
    db.seed_users(1_000_000).await;

    // 2. Query via Arrow Flight
    let start = Instant::now();

    let ticket = FlightTicket::GraphQLQuery {
        query: "{ users { id name email createdAt } }".to_string(),
        variables: None,
    };

    let mut client = FlightServiceClient::connect("http://localhost:50051").await.unwrap();
    let mut stream = client.do_get(Ticket { ticket: ticket.encode().unwrap() }).await.unwrap().into_inner();

    let mut total_rows = 0;
    let mut peak_memory = 0;

    while let Some(batch) = stream.message().await.unwrap() {
        let batch = decode_flight_data(batch);
        total_rows += batch.num_rows();

        // Track memory usage
        let current_memory = get_process_memory();
        if current_memory > peak_memory {
            peak_memory = current_memory;
        }
    }

    let duration = start.elapsed();

    // Assertions
    assert_eq!(total_rows, 1_000_000, "Should fetch all rows");
    assert!(duration.as_secs() < 60, "Should complete in < 60 seconds");
    assert!(peak_memory < 500_000_000, "Should use < 500MB RAM (streaming)");

    println!("✅ Million row test passed:");
    println!("   Duration: {:?}", duration);
    println!("   Throughput: {} rows/sec", total_rows as f64 / duration.as_secs_f64());
    println!("   Peak memory: {} MB", peak_memory / 1_000_000);
}

fn get_process_memory() -> usize {
    // Platform-specific memory measurement
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/self/status")
            .unwrap()
            .lines()
            .find(|line| line.starts_with("VmRSS:"))
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|kb| kb.parse::<usize>().ok())
            .unwrap_or(0)
            * 1024 // Convert KB to bytes
    }
    #[cfg(not(target_os = "linux"))]
    {
        0 // Not implemented for other platforms in this example
    }
}
```

---

### Step 4: Chaos Testing (1-2 hours)

**File**: `tests/chaos/failure_scenarios_test.rs`

```rust
#[tokio::test]
async fn test_clickhouse_crash_during_stream() {
    start_test_infrastructure().await;

    // Start streaming events
    let (tx, mut rx) = mpsc::channel(100);

    tokio::spawn(async move {
        stream_events_to_clickhouse(tx).await;
    });

    // Wait for streaming to start
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Crash ClickHouse
    Command::new("docker")
        .args(&["stop", "fraiseql-clickhouse"])
        .status()
        .unwrap();

    // Events should buffer or go to DLQ (not crash)
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Restart ClickHouse
    Command::new("docker")
        .args(&["start", "fraiseql-clickhouse"])
        .status()
        .unwrap();

    // Wait for recovery
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Verify events eventually made it
    let count = query_clickhouse("SELECT count() FROM fraiseql_events").await;
    assert!(count > 0, "Events should eventually be inserted");
}

#[tokio::test]
async fn test_elasticsearch_unavailable() {
    // Similar test for Elasticsearch failures
    // Should not block Arrow Flight streaming
}

#[tokio::test]
async fn test_nats_network_partition() {
    // Test NATS disconnect/reconnect
    // Should buffer events and resume
}
```

---

### Step 5: Docker Compose Test Infrastructure (30 min)

**File**: `docker-compose.test.yml`

```yaml
version: '3.8'

services:
  fraiseql-server:
    build: .
    ports:
      - "8080:8080"  # HTTP GraphQL
      - "50051:50051"  # Arrow Flight
    environment:
      DATABASE_URL: postgresql://postgres:postgres@postgres:5432/fraiseql_test
      NATS_URL: nats://nats:4222
      REDIS_URL: redis://redis:6379
      CLICKHOUSE_URL: http://clickhouse:8123
      ELASTICSEARCH_URL: http://elasticsearch:9200
    depends_on:
      - postgres
      - nats
      - redis
      - clickhouse
      - elasticsearch

  postgres:
    image: postgres:16
    environment:
      POSTGRES_DB: fraiseql_test
      POSTGRES_PASSWORD: postgres
    tmpfs:
      - /var/lib/postgresql/data  # In-memory for faster tests

  nats:
    image: nats:latest
    command: ["-js"]

  redis:
    image: redis:7

  clickhouse:
    image: clickhouse/clickhouse-server:24
    environment:
      CLICKHOUSE_DB: default

  elasticsearch:
    image: elasticsearch:8.15.0
    environment:
      - discovery.type=single-node
      - xpack.security.enabled=false
      - "ES_JAVA_OPTS=-Xms256m -Xmx256m"
```

---

## Verification Commands

```bash
# 1. Start test infrastructure
docker-compose -f docker-compose.test.yml up -d

# 2. Run integration tests
cargo test --test arrow_flight_pipeline_test

# 3. Run performance benchmarks
cargo bench --bench arrow_flight_benchmarks

# 4. Run stress test (manual)
cargo test --test million_row_test --ignored -- --nocapture

# 5. Run chaos tests
cargo test --test failure_scenarios_test

# Expected results:
# ✅ Integration tests: 100% passing
# ✅ Benchmarks: Arrow Flight 15-50x faster
# ✅ Stress test: 1M rows in < 60 seconds, < 500MB RAM
# ✅ Chaos tests: System recovers from failures
```

---

## Acceptance Criteria

- ✅ End-to-end pipeline tests passing (GraphQL → Arrow → Clients)
- ✅ Observer events flow to both ClickHouse and Elasticsearch
- ✅ Performance benchmarks show 50x improvement for 100k+ rows
- ✅ Million row test completes in < 60 seconds with constant memory
- ✅ Stress test: 10k events/sec sustained for 1 hour
- ✅ Chaos tests: System recovers from infrastructure failures
- ✅ Zero regressions in HTTP/JSON API
- ✅ All tests documented with expected results

---

## Performance Targets

| Metric | Target | Measured |
|--------|--------|----------|
| **GraphQL (100k rows)** | < 3 seconds | TBD |
| **Events streaming** | 1M+ events/sec | TBD |
| **Memory usage** | Constant (< 500MB) | TBD |
| **Latency (P99)** | < 100ms | TBD |
| **Concurrent clients** | 100+ | TBD |

---

## Next Steps

**[Phase 9.8: Documentation & Migration Guide](./phase-9.8-documentation.md)**
