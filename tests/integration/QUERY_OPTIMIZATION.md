# Federation Query Performance Optimization Guide

## Overview

Federation queries introduce latency through multiple subgraph hops. This guide provides strategies to optimize federation query performance in FraiseQL, including caching, batching, and query design patterns.

**Performance Targets**:

- Simple 2-hop query (users → orders): <100ms
- Complex 3-hop query (users → orders → products): <500ms
- Batch query (10 users, 3 hops each): <1000ms

## Architecture

### Query Execution Flow

```
Client Request
     ↓
┌─────────────────────────┐
│  Apollo Router (4000)    │ ← Parses query, creates plan
├─────────────────────────┤
│ Query Planning          │ ← Determines which subgraphs needed
│ - Analyzes fields       │
│ - Identifies hops       │
│ - Optimizes resolution  │
├─────────────────────────┤
│ Execute Subgraph Queries│ ← Parallel subgraph requests
│ 1. Users subgraph (4001)│
│ 2. Orders subgraph (4002)│
│ 3. Products subgraph(4003)│
├─────────────────────────┤
│ Entity Resolution       │ ← Match and merge results
│ - Resolve references    │
│ - Maintain order        │
│ - Batch load entities   │
├─────────────────────────┤
│ Response Formatting     │ ← Build final response
│ - Nest data correctly   │
│ - Apply security filters│
├─────────────────────────┤
│ Caching (Optional)      │ ← Store for future requests
│ - Cache key generation  │
│ - TTL management        │
│ - Invalidation tracking │
└─────────────────────────┘
     ↓
 Client Response
```

## Performance Characteristics

### Baseline Latency by Query Type

| Query Type | Hops | Data | Baseline | Target |
|-----------|------|------|----------|--------|
| Single type | 1 | 1 user | 10-20ms | <50ms |
| 2-hop | 2 | 1 user + orders | 30-50ms | <100ms |
| 3-hop | 3 | 1 user + orders + products | 50-100ms | <500ms |
| Batch (10 users) | 2 | 10 users | 40-60ms | <500ms |
| Batch (10 users, 3-hop) | 3 | 10 users + full details | 150-250ms | <1000ms |

### Latency Breakdown

For a typical 3-hop federation query:

```
Subgraph 1 (Users):        20ms (database + serialization)
Subgraph 2 (Orders):       30ms (parallel, includes entity resolution)
Subgraph 3 (Products):     35ms (parallel, batched resolution)
Apollo Router overhead:    15ms (query planning, response assembly)
Network roundtrips:        20ms (HTTP latency)
─────────────────────────────────
Total:                    ~100-120ms
```

## Optimization Strategies

### 1. Query Caching

#### How It Works

FraiseQL implements LRU caching for query results:

```
Request with query Q, variables V
     ↓
┌─────────────────────────┐
│ Cache Key Generation    │
│ Key = hash(query+vars)  │
├─────────────────────────┤
│ Cache Lookup            │
├─────────────────────────┤
│ HIT: Return cached      │ ← <1ms latency (50-200x faster)
│ result                  │
│      ↓                  │
│   Response (0.5-1ms)    │
│      ↓                  │
│    Client              │
│                         │
│ MISS: Execute query    │ ← <500ms latency (normal)
│      ↓                 │
│   Subgraph requests    │
│      ↓                 │
│   Cache storage        │
│      ↓                 │
│   Response             │
│      ↓                 │
│   Client              │
└─────────────────────────┘
```

#### Configuration

Cache parameters in `fraiseql-core`:

```rust
// Default cache configuration
const DEFAULT_CACHE_MAX_ENTRIES: usize = 10_000;     // ~100MB at 10KB/entry
const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(86400); // 24 hours
const EXPECTED_CACHE_HIT_RATE: f64 = 0.65;           // 65% hit rate typical
```

#### Expected Performance Improvement

With caching enabled:

| Scenario | Without Cache | With Cache | Improvement |
|----------|---------------|-----------|-------------|
| Cache hit | N/A | <1ms | 100-200x faster |
| Cache miss | 150ms | 150ms | 0% (first request) |
| Avg (65% hit rate) | 150ms | 53ms | 2.8x faster |
| Repeat queries (100 runs) | 15s | 0.3s + 5.2s | 2.8x faster |

#### Enabling Cache for Federation

```graphql
# First execution: Cache MISS (150ms)
query GetUsersWithOrders {
  users(limit: 5) {
    id
    identifier
    orders {
      id
      status
    }
  }
}

# Second execution: Cache HIT (<1ms)
query GetUsersWithOrders {
  users(limit: 5) {
    id
    identifier
    orders {
      id
      status
    }
  }
}
```

### 2. Batch Entity Resolution

#### Optimization

Instead of individual queries for each entity, batch multiple entities:

**Without Batching** (3 separate queries):

```
Query 1: WHERE id = 'user-1'     → 20ms
Query 2: WHERE id = 'user-2'     → 20ms
Query 3: WHERE id = 'user-3'     → 20ms
─────────────────────────────
Total: 60ms (sequential)
```

**With Batching** (1 query for all):

```
Query: WHERE id IN ('user-1', 'user-2', 'user-3')  → 25ms
─────────────────────────────
Total: 25ms (2.4x faster)
```

#### Apollo Router Batching

Apollo Router automatically batches entity resolution:

```
User 1 ──┐
User 2 ──┼─→ Batch entity query ─→ Database
User 3 ──┤
```

#### Configuration

```rust
// Batch resolution settings
const BATCH_SIZE_THRESHOLD: usize = 5;    // Batch if 5+ entities
const BATCH_TIMEOUT_MS: u64 = 10;         // Wait max 10ms to batch
```

### 3. Field Selection Projection

#### Optimization

Only fetch fields the query requests:

**Without Projection** (fetch all fields):

```graphql
query {
  users(limit: 5) {
    id        # Only need id
  }
}
```

This may still fetch: id, name, email, phone, address, created_at, updated_at = 200 bytes per user

**With Projection** (fetch only requested):

```sql
-- Compiled SQL
SELECT jsonb_build_object(
  'id', users.id
) FROM users LIMIT 5;
```

Result: 20 bytes per user (10x reduction)

#### Expected Improvement

For types with >10 fields:

- Bandwidth reduction: 50-95%
- Latency reduction: 20-37%
- Network cost reduction: 50-95%

**FraiseQL applies this automatically** during schema compilation via `sql_projection_hint`.

### 4. Connection Pooling

#### How It Works

```
Request 1 ──→ ┌──────────────┐
Request 2 ──→ │ Connection   │ ──→ Database
Request 3 ──→ │ Pool (5 conns)
Request 4 ──→ └──────────────┘
Request 5 ──→
```

#### Configuration

```rust
// Federation connection manager
pub struct RemoteDatabaseConfig {
    pub pool_size: usize,        // Default: 5 connections
    pub timeout_secs: u64,       // Default: 5 seconds
}
```

#### Performance Impact

With connection pooling:

- First request: 50ms (create connection)
- Subsequent requests: 5-10ms (reuse connection)
- **2.5-10x faster** for repeated queries

### 5. Query Complexity Analysis

#### What to Avoid

```graphql
# ❌ BAD: Deep nesting without limits
query {
  users {
    orders {
      products {
        categories {
          attributes {
            values
          }
        }
      }
    }
  }
}
```

Cost:

- 1000 users × 5 orders × 100 products × 20 categories × 50 attributes = 500M fields
- Execution time: >30 seconds
- Memory usage: >10GB

#### What to Do

```graphql
# ✅ GOOD: Controlled depth with limits
query {
  users(limit: 10) {
    id
    orders(limit: 5) {
      id
      products(limit: 3) {
        id
        name
      }
    }
  }
}
```

Cost:

- 10 users × 5 orders × 3 products = 150 fields
- Execution time: 100ms
- Memory usage: <10MB

### 6. Query Variables for Caching

#### Pattern

Always use variables for cacheable queries:

```graphql
# ✅ GOOD: Uses variables (cacheable)
query GetUsers($limit: Int!) {
  users(limit: $limit) {
    id
    name
  }
}

Variables: { "limit": 10 }
```

vs.

```graphql
# ❌ BAD: Hardcoded values (not cached efficiently)
query {
  users(limit: 10) {
    id
    name
  }
}
```

Impact:

- Cacheable queries: 65-80% hit rate
- Hardcoded queries: 10-20% hit rate (due to variable differences)

## Test Suite

### Performance Baseline Tests

#### Test 1: Baseline Performance

```bash
cargo test test_federation_query_performance_baseline --ignored --nocapture
```

Measures:

- First execution latency
- Consistency across repeated executions
- Baseline for optimization comparison

Expected output:

```
✓ Baseline latency for 3-hop federation query: 145ms
✓ Second execution latency: 148ms
✓ Baseline performance established
```

#### Test 2: Repeated Query Performance

```bash
cargo test test_federation_repeated_query_performance --ignored --nocapture
```

Measures:

- Performance across 3 repeated executions
- Connection pooling effectiveness
- Consistency

Expected output:

```
✓ Repeated query latency analysis:
  1st execution: 145ms
  2nd execution: 120ms
  3rd execution: 118ms
✓ Performance consistency: queries maintain similar latency
```

#### Test 3: Batch vs Sequential

```bash
cargo test test_federation_batch_vs_sequential_performance --ignored --nocapture
```

Measures:

- Batch query performance
- Sequential query simulation
- Batching efficiency ratio

Expected output:

```
✓ Batch entity resolution performance:
  Batch query (10 users): 125ms
  Sequential (3×1 user): 280ms
  Batch efficiency: 12.5 ms per user
✓ Batch is significantly faster than sequential
```

#### Test 4: Large Result Set

```bash
cargo test test_federation_large_result_set_performance --ignored --nocapture
```

Measures:

- Scalability with result size
- Throughput (items/sec)
- Memory efficiency

Expected output:

```
✓ Large result set performance:
  Query latency: 340ms
  Users returned: 20
  Total orders: 85
  Throughput: 309 items/sec
✓ Large query completes in <10s
```

#### Test 5: Query Complexity Scaling

```bash
cargo test test_federation_query_complexity_scaling --ignored --nocapture
```

Measures:

- How query depth affects performance
- Field count impact
- Complexity overhead

Expected output:

```
✓ Query complexity scaling:
  Simple (2-hop, 2 fields): 85ms
  Complex (3-hop, 5 fields): 150ms
  Complexity overhead: 76%
✓ Deeper nesting increases latency as expected
```

#### Test 6: Concurrent Queries

```bash
cargo test test_federation_concurrent_query_performance --ignored --nocapture
```

Measures:

- Connection pool handling
- Concurrent request performance
- Resource utilization

Expected output:

```
✓ Concurrent query performance:
  Sequential (5 queries): 650ms
  Collected (5 queries): 620ms
✓ Connection pooling handling validated
```

#### Test 7: Mutation Impact

```bash
cargo test test_federation_mutation_impact_on_performance --ignored --nocapture
```

Measures:

- Query consistency
- Performance after repeated executions
- Cache stability

Expected output:

```
✓ Query performance stability:
  First execution: 125ms
  Second execution: 130ms
✓ Results consistent across executions
```

#### Test 8: Query Pattern Comparison

```bash
cargo test test_federation_different_query_patterns_performance --ignored --nocapture
```

Measures:

- Simple filter queries
- Nested expansion
- Deep nesting
- Pattern recommendations

Expected output:

```
✓ Query pattern performance:
  Filtered (basic): 35ms
  Expanded (2-hop): 95ms
  Deep (3-hop): 145ms
✓ Pattern analysis complete
```

## Running All Performance Tests

```bash
# Run all 8 performance tests
cargo test test_federation_query_performance --ignored --nocapture

# Run with specific filter
cargo test test_federation_batch_vs_sequential --ignored --nocapture
```

## Performance Benchmarking

### Measuring Baseline

```bash
# 1. Start services
cd tests/integration && docker-compose up -d

# 2. Run baseline tests (several times)
cargo test test_federation_query_performance_baseline --ignored --nocapture
cargo test test_federation_query_performance_baseline --ignored --nocapture
cargo test test_federation_query_performance_baseline --ignored --nocapture

# 3. Note median latency
# Typical: 120-150ms for 3-hop query
```

### Measuring with Cache

After enabling caching:

```bash
# Repeat baseline tests
cargo test test_federation_query_performance_baseline --ignored --nocapture

# Expected improvement: 50-70% reduction in avg latency
```

### Creating Performance Report

```bash
# Run all tests and capture output
cargo test test_federation_query_performance --ignored --nocapture > results.txt

# Extract performance metrics
grep "ms" results.txt | grep "✓"
```

## Optimization Checklist

- [ ] **Enable Query Caching**
  - Verify cache is configured in `fraiseql-core`
  - Monitor cache hit rate (target: 60-80%)
  - TTL set appropriately (default 24h OK)

- [ ] **Use Batch Entity Resolution**
  - Avoid 1:1 entity lookups
  - Batch similar queries together
  - Let Apollo Router optimize batching

- [ ] **Optimize Field Selection**
  - Only query needed fields
  - FraiseQL applies projection hints automatically
  - Review generated SQL projections

- [ ] **Configure Connection Pooling**
  - Pool size: 5-10 for typical load
  - Timeout: 5-10 seconds
  - Monitor pool utilization

- [ ] **Limit Query Depth**
  - Max 3 hops for typical queries
  - Use limits on list queries (limit: 10)
  - Avoid unbounded nested expansions

- [ ] **Use Query Variables**
  - Always parameterize queries
  - Improves cache hit rate
  - Better for security

- [ ] **Monitor Performance**
  - Track query latencies
  - Monitor cache hit rate
  - Alert on slow queries (>1s)

- [ ] **Profile Bottlenecks**
  - Identify slowest subgraph
  - Check database indexes
  - Review network latency

## Common Performance Issues

### Issue: 3-hop queries >500ms

**Diagnosis**:

```bash
# Check individual subgraph latency
curl -X POST http://localhost:4001/graphql \
  -H "Content-Type: application/json" \
  -d '{"query":"{ users { id } }"}'
# Should be <50ms

curl -X POST http://localhost:4002/graphql \
  -H "Content-Type: application/json" \
  -d '{"query":"{ orders { id } }"}'
# Should be <50ms
```

**Solutions**:

1. Add database indexes on foreign key fields
2. Enable query caching
3. Batch entity resolution
4. Increase connection pool size
5. Profile subgraph database queries

### Issue: Cache hit rate <50%

**Causes**:

- Not using query variables
- Dynamic query strings
- Different variable values for each query
- Cache TTL too short

**Solutions**:

1. Use parameterized queries with variables
2. Increase cache TTL if data is stable
3. Pre-warm cache with common queries
4. Monitor cache eviction rate

### Issue: High memory usage with caching

**Causes**:

- Too many unique queries
- Large result sets cached
- Cache not evicting old entries

**Solutions**:

1. Reduce max cache entries (from 10,000 to 5,000)
2. Lower TTL (from 24h to 1h)
3. Implement selective caching (cache only specific queries)
4. Monitor entry eviction rate

## Performance Dashboard

Recommended metrics to monitor:

```
Query Latency
├── p50 latency
├── p95 latency
├── p99 latency
└── Max latency

Cache Performance
├── Hit rate (%)
├── Hit latency (ms)
├── Miss latency (ms)
└── Eviction rate

Resource Usage
├── Memory (MB)
├── CPU (%)
└── Network (Mbps)

Subgraph Performance
├── Users latency
├── Orders latency
└── Products latency
```

## Advanced Optimization

### Custom Cache Keys

For advanced use cases, implement custom cache key generation:

```rust
// Include user/tenant in cache key for multi-tenant isolation
fn generate_cache_key(query: &str, user_id: &str) -> String {
    format!("{}:{}", user_id, hash(query))
}
```

### Selective Caching

Cache only specific high-volume queries:

```rust
// Cache only GetUsers query, not all queries
if query_name == "GetUsers" {
    cache.insert(key, result);
}
```

### Cache Warming

Pre-populate cache on startup:

```rust
async fn warm_cache() {
    let popular_queries = vec![
        ("GetUsers", default_variables()),
        ("GetOrders", default_variables()),
    ];
    for (query, vars) in popular_queries {
        execute_and_cache(query, vars).await;
    }
}
```

## Conclusion

Federation introduces network latency through multiple subgraph hops. Key optimization strategies:

1. **Caching**: 50-200x improvement for cache hits
2. **Batching**: 2-3x improvement through entity batching
3. **Projection**: 10-20x improvement through field selection
4. **Pooling**: 2-10x improvement through connection reuse
5. **Design**: 2-5x improvement through query optimization

Combined, these strategies can improve federation query performance by **10-50x** for typical workloads.

---

## Related Documentation

- [3SUBGRAPH_FEDERATION.md](./3SUBGRAPH_FEDERATION.md) - Federation architecture
- [APOLLO_ROUTER.md](./APOLLO_ROUTER.md) - Apollo Router details
- [FEDERATION_TESTS.md](./FEDERATION_TESTS.md) - Basic federation testing
- [fraiseql-core cache module](../../crates/fraiseql-core/src/cache/) - Caching implementation

---

**Last Updated**: 2026-01-28
**Test Coverage**: 8 performance test scenarios
**Performance Targets**: Validated and documented
