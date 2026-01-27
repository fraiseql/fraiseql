# Cycle 16-5: RED Phase - Resolution Strategies & Database Linking

**Cycle**: 5 of 8
**Phase**: RED (Write failing tests first)
**Duration**: ~3-4 days
**Focus**: Define resolution strategies (Direct DB, HTTP fallback) through failing tests

**Prerequisites**:
- Cycles 1-2 (Core Federation Runtime) complete
- Cycles 3-4 (Multi-Language Authoring) complete
- Ready to implement advanced resolution strategies

---

## Objective

Define federation entity resolution strategies:
1. Direct database federation (FraiseQL-to-FraiseQL same DB type)
2. HTTP fallback (external subgraphs or when DB fails)
3. Connection pooling and management
4. Batching optimization

All tests must fail initially.

---

## Requirements Definition

### Requirement 1: Direct Database Federation

**Description**: Resolve entities via direct database connection (no HTTP)

**Scenario**:
```
FraiseQL Subgraph A (PostgreSQL, owns User)
FraiseQL Subgraph B (MySQL, owns Order)

To resolve Order.user_id references:
- Query: SELECT * FROM SubgraphA.users WHERE id IN (...)
- Connection: Direct DB link from B's MySQL to A's PostgreSQL
- Latency: <20ms (one DB round-trip)
```

**Acceptance Criteria**:
- [ ] Direct DB connections configured per remote subgraph
- [ ] Query execution across databases works
- [ ] Connection pooling per remote database
- [ ] Batching works across database boundaries
- [ ] Latency <20ms verified
- [ ] Fallback to HTTP if DB connection fails

---

### Requirement 2: HTTP Fallback Resolution

**Description**: Fallback to HTTP POST when DB unavailable

**Scenario**:
```
Apollo Router sends entity resolution request
→ FraiseQL tries direct DB connection
→ Connection fails or DB unreachable
→ Fallback to HTTP: POST /graphql to remote subgraph
→ Send standard GraphQL _entities query
← Receive response
→ Cache result for similar requests
```

**Acceptance Criteria**:
- [ ] HTTP client configured for each remote subgraph
- [ ] `_entities` query sent via HTTP
- [ ] Response parsing matches federation spec
- [ ] Timeout handling (5-10 second timeout)
- [ ] Retry logic (exponential backoff)
- [ ] Latency <200ms verified
- [ ] Error handling (partial failures)

---

### Requirement 3: Connection Management

**Description**: Multi-database connection pooling

**Architecture**:
```rust
struct ConnectionManager {
    local_pool: Arc<ConnectionPool>,           // Local database
    remote_pools: HashMap<String, ConnectionPool>,  // Per remote DB
    http_client: reqwest::Client,              // HTTP client
}

Connection Strategy for "resolve User from SubgraphA":
1. Check if SubgraphA has direct DB connection
2. If yes: Get connection from pool, execute query
3. If no: Use HTTP client
4. Cache strategy for next request
```

**Acceptance Criteria**:
- [ ] Connection pools created per remote database
- [ ] Pool size configurable
- [ ] Connection reuse verified
- [ ] No connection leaks under load
- [ ] Automatic reconnection on failure
- [ ] Pool monitoring (active connections, wait time)

---

### Requirement 4: Batching Optimization

**Description**: Batch multiple entity resolutions

**Example**:
```
Input: Resolve 100 users (50 from SubgraphA, 50 from SubgraphB)
Process:
  1. Group by typename + strategy
  2. Batch 1: 50 users from SubgraphA (direct DB)
  3. Batch 2: 50 users from SubgraphB (HTTP)
  4. Execute both batches in parallel
  5. Combine results in input order
Output: 100 resolved users in 20ms instead of 200+ms
```

**Acceptance Criteria**:
- [ ] Entities grouped by typename + strategy
- [ ] Batch execution parallelized
- [ ] Results combined in original order
- [ ] N+1 queries eliminated
- [ ] Latency improvements verified (2-10x speedup)
- [ ] Memory efficient (no copying)

---

### Requirement 5: Error Handling & Resilience

**Description**: Graceful error handling for partial failures

**Scenarios**:
```
Scenario A: One entity not found
Input: [User(id=123), User(id=999)]
Result: [Some(User), None]  // Mixed success/failure

Scenario B: Database connection fails
→ Retry with exponential backoff
→ If persistent: fall back to HTTP
→ If all fail: return error

Scenario C: Partial batch failure
Input: 100 entities
Result: 95 resolved + 5 errors
Action: Return partial results + errors in metadata
```

**Acceptance Criteria**:
- [ ] Null entities handled gracefully
- [ ] Partial batch failures supported
- [ ] Retry logic with exponential backoff
- [ ] Clear error messages
- [ ] Error context preserved (typename, keys, strategy)
- [ ] Performance not degraded by errors

---

### Requirement 6: Performance & Monitoring

**Description**: Latency and throughput verification

**Targets**:
```
Direct Database Resolution:
  - Single entity: <5ms
  - 100 entities: <15ms
  - Throughput: >100 batches/sec

HTTP Resolution:
  - Single entity: <50ms
  - 100 entities: <200ms
  - Throughput: >5 batches/sec

Batching Improvement:
  - Sequential (no batching): 100 entities = 2000ms
  - Batched (1 query): 100 entities = 15ms
  - Improvement: 133x speedup
```

**Acceptance Criteria**:
- [ ] Latency targets verified with benchmarks
- [ ] Throughput targets verified
- [ ] Memory usage acceptable (<100MB for 1000 concurrent)
- [ ] No performance degradation under load
- [ ] Connection pool doesn't become bottleneck

---

## Test Files to Create

### 1. Direct Database Tests: `crates/fraiseql-core/tests/federation/test_direct_db_resolution.rs`

```rust
#[cfg(test)]
mod direct_db_resolution_tests {
    use super::*;

    #[tokio::test]
    async fn test_direct_db_connection_postgres_to_postgres() {
        // Setup: Two PostgreSQL databases
        // Execute: Resolve entities via direct connection
        // Assert: Results match expected data
    }

    #[tokio::test]
    async fn test_direct_db_connection_postgres_to_mysql() {
        // Setup: PostgreSQL and MySQL databases
        // Execute: Cross-database entity resolution
        // Assert: Type coercion and data types handled correctly
    }

    #[tokio::test]
    async fn test_connection_pooling() {
        // Execute: Multiple parallel resolutions
        // Assert: Connections reused from pool
        // Assert: No connection leaks
    }

    #[tokio::test]
    async fn test_batch_across_databases() {
        // Setup: 50 entities from DB A, 50 from DB B
        // Execute: Single _entities query
        // Assert: Both batches executed
        // Assert: Latency < 20ms
    }

    #[test]
    fn test_connection_pool_size_config() {
        // Assert: Pool size configurable
        // Assert: Pool size verified
    }
}
```

### 2. HTTP Fallback Tests: `crates/fraiseql-core/tests/federation/test_http_resolution.rs`

```rust
#[cfg(test)]
mod http_resolution_tests {
    use super::*;

    #[tokio::test]
    async fn test_http_entity_resolution() {
        // Setup: Mock HTTP server returning _entities response
        // Execute: Resolve entities via HTTP
        // Assert: Response parsed correctly
    }

    #[tokio::test]
    async fn test_http_timeout() {
        // Setup: HTTP server doesn't respond
        // Execute: Resolution with 5s timeout
        // Assert: Timeout triggered after 5s
        // Assert: Fallback strategy triggered (if applicable)
    }

    #[tokio::test]
    async fn test_http_retry_with_backoff() {
        // Setup: HTTP server fails 3 times, succeeds on 4th
        // Execute: Resolution with retry logic
        // Assert: Retried with exponential backoff
        // Assert: Eventually succeeds
    }

    #[tokio::test]
    async fn test_http_partial_failure() {
        // Setup: HTTP response contains errors
        // Execute: Resolution
        // Assert: Partial results returned
        // Assert: Errors included in response
    }
}
```

### 3. Connection Management Tests: `crates/fraiseql-core/tests/federation/test_connection_management.rs`

```rust
#[cfg(test)]
mod connection_management_tests {
    use super::*;

    #[tokio::test]
    async fn test_connection_manager_creation() {
        // Create connection manager with multiple remotes
        // Assert: Pools created for each remote
    }

    #[tokio::test]
    async fn test_connection_reuse() {
        // Get connection twice for same remote
        // Assert: Same connection reused
    }

    #[tokio::test]
    async fn test_connection_pool_exhaustion() {
        // Setup: Pool size = 2, execute 5 concurrent
        // Assert: Queue forms
        // Assert: No deadlock
    }

    #[tokio::test]
    async fn test_connection_error_recovery() {
        // Setup: Connection fails
        // Execute: Retry connection
        // Assert: New connection created
    }
}
```

### 4. Batching Tests: `crates/fraiseql-core/tests/federation/test_batching.rs`

```rust
#[cfg(test)]
mod batching_tests {
    use super::*;

    #[test]
    fn test_batch_grouping_by_typename() {
        // Input: [User(1), Order(1), User(2), Order(2)]
        // Assert: Grouped as [User(1), User(2)] + [Order(1), Order(2)]
    }

    #[test]
    fn test_batch_grouping_by_strategy() {
        // Input: Users from 2 different databases
        // Assert: Grouped by resolution strategy
    }

    #[tokio::test]
    async fn test_batch_parallel_execution() {
        // Setup: 100 entities from DB A, 100 from DB B
        // Execute: Batched resolution
        // Assert: Both batches executed in parallel
        // Assert: Total latency < single sequential
    }

    #[test]
    fn test_batch_preserve_order() {
        // Input: [User(3), User(1), User(2)]
        // Batch and execute
        // Assert: Results match input order
    }
}
```

### 5. Error Handling Tests: `crates/fraiseql-core/tests/federation/test_error_handling.rs`

```rust
#[cfg(test)]
mod error_handling_tests {
    use super::*;

    #[tokio::test]
    async fn test_missing_entity_returns_null() {
        // Request entity that doesn't exist
        // Assert: Returns null (not error)
    }

    #[tokio::test]
    async fn test_partial_batch_failure() {
        // Batch with 10 entities, 2 not found
        // Assert: Returns 8 results + 2 nulls
    }

    #[tokio::test]
    async fn test_connection_failure_recovery() {
        // DB connection fails, should retry
        // Assert: Retried
        // Assert: Falls back if persistent
    }

    #[tokio::test]
    async fn test_error_context_preserved() {
        // Resolution fails
        // Assert: Error includes typename, keys, strategy
    }
}
```

### 6. Performance Benchmark Tests: `crates/fraiseql-core/benches/federation_resolution_benchmarks.rs`

```rust
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_direct_db_resolution(c: &mut Criterion) {
    let mut group = c.benchmark_group("direct_db_resolution");

    group.bench_function("single_entity", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                // Resolve 1 entity from remote DB
            });
    });

    group.bench_function("batch_100_entities", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                // Resolve 100 entities
            });
    });

    group.finish();
}

fn bench_http_resolution(c: &mut Criterion) {
    let mut group = c.benchmark_group("http_resolution");

    group.bench_function("single_entity", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                // Resolve via HTTP
            });
    });

    group.finish();
}

criterion_group!(benches, bench_direct_db_resolution, bench_http_resolution);
criterion_main!(benches);
```

---

## Test Execution & Verification

```bash
# All federation tests
cargo test --test federation

# Specific test module
cargo test --test federation test_direct_db_resolution

# With output
cargo test --test federation -- --nocapture

# Benchmarks
cargo bench --bench federation_resolution_benchmarks

# Expected output shows latency targets
```

---

## Validation Checklist

- [ ] All direct DB tests written (20+ tests)
- [ ] All HTTP tests written (15+ tests)
- [ ] All connection management tests (10+ tests)
- [ ] All batching tests (10+ tests)
- [ ] All error handling tests (10+ tests)
- [ ] Performance benchmark tests (10+ tests)
- [ ] All tests fail with clear messages
- [ ] Tests are focused and non-interdependent

---

**Status**: [~] In Progress (Writing tests)
**Next**: GREEN Phase - Implement resolution strategies
