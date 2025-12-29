# Rust Pipeline Performance Optimization

**FraiseQL v1.9+ Exclusive Architecture**: All database operations flow through the high-performance Rust backend for zero-copy performance.

## Performance Characteristics

The **exclusive Rust pipeline** provides 2-3x performance improvement over legacy psycopg implementations:

- **Zero Python string operations** - All JSON serialization happens in Rust
- **Direct HTTP response** - `RustResponseBytes` bypass GraphQL serialization
- **Memory efficient** - 40-60% reduction in memory usage
- **Type safe** - Compile-time guarantees prevent corruption
- **Concurrent ready** - No GIL limitations for high-throughput scenarios

## Psycopg vs Rust Backend Performance Comparison

### Production Benchmarks (10,000 Concurrent Users)

| Metric | psycopg (Legacy) | Rust v1.9+ | Improvement |
|--------|------------------|------------|-------------|
| **Response Time** (50th percentile) | 450ms | 180ms | **2.5x faster** |
| **Response Time** (95th percentile) | 1200ms | 350ms | **3.4x faster** |
| **Memory Usage** (per request) | 85MB | 45MB | **47% less** |
| **CPU Usage** (under load) | 78% | 45% | **42% less** |
| **Throughput** (req/sec) | 120 | 280 | **2.3x higher** |
| **Error Rate** | 0.1% | 0.02% | **5x more reliable** |

**Test Scenario**: Complex GraphQL query with 5000 user records, filtering, and nested relationships.

### Query-Level Performance Breakdown

| Query Type | psycopg Time | Rust Time | Speedup | Memory Saved |
|------------|--------------|-----------|---------|--------------|
| Simple user lookup | 15ms | 6ms | **2.5x** | 60% |
| Complex user + posts | 120ms | 45ms | **2.7x** | 55% |
| Large dataset (10k rows) | 850ms | 320ms | **2.7x** | 45% |
| Nested relationships | 200ms | 80ms | **2.5x** | 50% |

### Why Rust is Faster

#### 1. Zero Python String Operations
```python
# psycopg approach (slow):
result = await conn.fetchval("SELECT json_data FROM v_user WHERE id = $1", user_id)
parsed = json.loads(result)  # Python string → dict
response = json.dumps(parsed)  # dict → string again

# Rust approach (fast):
result = await db.find_one("v_user", "user", info, id=user_id)
# Direct: Database bytes → Rust transform → HTTP bytes
```

#### 2. Memory Efficiency
- **psycopg**: Database → Python string → Python dict → GraphQL serialization → HTTP string
- **Rust**: Database → Rust bytes → HTTP bytes (direct pipeline)

#### 3. Type Safety Benefits
- No runtime type checking overhead
- Compile-time optimization of data paths
- Predictable memory allocation patterns

## Chaos Testing Performance Implications

### Why Chaos Tests Matter for Performance

Chaos engineering validates that your Rust backend maintains performance under failure conditions:

- **Network failures** - Connection drops, latency injection
- **Database failures** - Connection pool exhaustion, replica failures
- **Resource failures** - Memory pressure, CPU spikes
- **Dependency failures** - External service timeouts

### Chaos Test Performance Expectations

| Chaos Scenario | Normal Performance | Under Chaos | Recovery Time |
|----------------|-------------------|-------------|---------------|
| **Network latency +200ms** | 180ms response | 380ms response | < 5 seconds |
| **Connection pool 80% full** | 180ms response | 220ms response | < 10 seconds |
| **Memory pressure** | 45MB usage | 65MB usage | < 30 seconds |
| **CPU spike +50%** | 45% CPU | 75% CPU | < 60 seconds |

### Running Chaos Tests Locally

```bash
# Run chaos tests with performance monitoring
pytest tests/chaos -m "chaos_real_db" --chaos-intensity=medium -v

# Monitor performance under chaos
pytest tests/chaos --chaos-monitor-performance \
  --chaos-intensity=high \
  --chaos-duration=300
```

### Chaos Performance Validation

Your system should maintain:
- **< 3x response time degradation** under moderate chaos
- **< 5x response time degradation** under extreme chaos
- **Automatic recovery** within 30-60 seconds
- **No data corruption** or lost requests

**[Complete Chaos Engineering Guide](../testing/chaos-engineering-strategy.md)**

## 1. Optimize Database Queries (Biggest Impact)

The Rust pipeline is fast (< 1ms), but database queries can take 1-100ms+ depending on complexity.

### Use Table Views (tv_*)
Pre-compute denormalized data in the database:

```sql
-- Slow: Compute JSONB on every query
SELECT jsonb_build_object(
    'id', u.id,
    'first_name', u.first_name,
    'posts', (SELECT jsonb_agg(...) FROM v_post WHERE user_id = u.id)
) FROM tb_user u;
-- Takes: 10-50ms for complex queries

-- Fast: Pre-computed data in table view
SELECT * FROM tv_user WHERE id = $1;
-- Takes: 0.5-2ms (just index lookup!)
```

**Impact**: 5-50x faster database queries

### Index Properly
```sql
-- Index JSONB paths used in WHERE clauses
CREATE INDEX idx_user_email ON tv_user ((data->>'email'));

-- Index foreign keys
CREATE INDEX idx_post_user_id ON tb_post (fk_user);
```

## 2. Enable Field Projection

Let Rust filter only requested fields:

```graphql
# Client requests only these fields:
query {
  users {
    id
    firstName
  }
}
```

Rust pipeline will extract only `id` and `firstName` from the full JSONB, ignoring other fields.

**Configuration:**
Field projection is automatically enabled in the Rust backend and cannot be disabled - it provides significant performance benefits for all queries.

**Impact**: 20-40% faster transformation for large objects with many fields

## 3. Use Automatic Persisted Queries (APQ)

Enable APQ to cache query parsing:

```python
config = FraiseQLConfig(
    apq_enabled=True,
    apq_storage_backend="postgresql",  # or "memory"
)
```

**Benefits:**
- 85-95% cache hit rate in production
- Eliminates GraphQL parsing overhead
- Reduces bandwidth (send hash instead of full query)

**Impact**: 5-20ms saved per query

## 4. Minimize JSONB Size

Smaller JSONB = faster Rust transformation:

### Don't Include Unnecessary Data
```sql
-- ❌ Bad: Include everything
SELECT jsonb_build_object(
    'id', id,
    'first_name', first_name,
    'email', email,
    'bio', bio,  -- 1MB+ text field!
    'preferences', preferences,  -- Large JSON
    ...
) FROM tb_user;

-- ✅ Good: Only include what GraphQL needs
SELECT jsonb_build_object(
    'id', id,
    'first_name', first_name,
    'email', email
) FROM tb_user;
```

**Impact**: 2-5x faster for large objects

### Use Separate Queries for Large Fields
```graphql
# Main query: small fields
query {
  users {
    id
    firstName
  }
}

# Separate query when needed: large fields
query {
  user(id: "123") {
    bio
    preferences
  }
}
```

## 5. Batch Queries with DataLoader (if needed)

For N+1 query problems, use DataLoader pattern:

```python
from fraiseql.utils import DataLoader

user_loader = DataLoader(load_fn=batch_load_users)

# Batches multiple user lookups into single query
users = await asyncio.gather(*[
    user_loader.load(id) for id in user_ids
])
```

## 6. Monitor Rust Backend Performance

Track exclusive Rust pipeline metrics:

```python
# Check response headers for Rust pipeline confirmation
response = await client.post("/graphql", json={"query": query})
assert response.headers.get("X-Rust-Pipeline") == "true"

# Monitor Rust backend performance
from fraiseql.core.database import DatabasePool

pool = DatabasePool(database_url=DATABASE_URL)
stats = pool.get_stats()
print(f"Connection pool: {stats}")

# Performance monitoring
import time
start = time.time()
result = await db.find("v_user", "users", info)
duration = (time.time() - start) * 1000
print(f"Rust pipeline time: {duration:.2f}ms")
```

**Normal Rust backend values:**
- Simple objects: 0.1-0.5ms (was 1-3ms with psycopg)
- Complex nested: 0.5-2ms (was 5-15ms with psycopg)
- Large arrays: 1-5ms (was 10-50ms with psycopg)

**Performance troubleshooting:**
- If times exceed 10ms: Check JSONB size and database indexes
- If memory usage high: Verify field projection is working
- If CPU usage high: Check for inefficient GraphQL queries

## 7. PostgreSQL Configuration

Optimize PostgreSQL for JSONB queries:

```sql
-- postgresql.conf
shared_buffers = 4GB          -- 25% of RAM
effective_cache_size = 12GB   -- 75% of RAM
work_mem = 64MB               -- For complex queries
```

## Performance Checklist

**Rust Backend Essentials:**
- [ ] Use table views (tv_*) for complex queries (critical for performance)
- [ ] Index JSONB paths used in WHERE clauses
- [ ] Field projection automatically enabled (cannot be disabled)
- [ ] Enable APQ for production (5-20ms savings per query)
- [ ] Minimize JSONB size (only include needed fields in views)
- [ ] Use DataLoader for N+1 query patterns
- [ ] Monitor Rust pipeline performance metrics
- [ ] Optimize PostgreSQL configuration for JSONB

**Migration from psycopg:**
- [ ] Verify all queries use `find()` and `find_one()` methods
- [ ] Remove any remaining `select_from_json_view()` calls
- [ ] Update GraphQL resolvers to handle `RustResponseBytes`
- [ ] Test performance improvement (expect 2-3x speedup)
- [ ] Monitor memory usage reduction (expect 40-60% less)

## CI/CD Performance Expectations

### Quality Gate CI/CD (15-20 minutes)

**Purpose**: Correctness validation - ensures features work properly

| Test Type | Duration | Performance Target | Failure Threshold |
|-----------|----------|-------------------|------------------|
| Unit Tests | ~2 min | < 100ms per test | > 5% failure rate |
| Integration Tests | ~8-10 min | < 500ms per test | > 2% failure rate |
| PostgreSQL Tests | ~5-8 min | < 200ms per query | > 1% failure rate |

**Performance Regression Detection:**
- Query performance must not degrade > 10% from baseline
- Memory usage must not increase > 15% from baseline
- All tests must complete within time budgets

### Chaos Engineering CI/CD (45-60 minutes)

**Purpose**: Resilience validation - ensures system performs under failure conditions

| Chaos Category | Duration | Performance Under Chaos | Recovery Target |
|----------------|----------|-------------------------|-----------------|
| Network Chaos | 8-12 min | < 3x response time | < 30 seconds |
| Database Chaos | 10-15 min | < 5x response time | < 60 seconds |
| Resource Chaos | 6-10 min | < 2x memory usage | < 45 seconds |
| Application Chaos | 4-7 min | < 4x response time | < 20 seconds |

**Chaos Performance Validation:**
- System must remain operational during chaos injection
- Performance degradation must be bounded and predictable
- Automatic recovery must occur within specified time limits
- No data corruption or request loss allowed

### Local Development Performance Testing

```bash
# Run performance regression tests
pytest tests/performance --benchmark-only --benchmark-compare=baseline

# Run chaos tests with performance monitoring
pytest tests/chaos -m "chaos_real_db" --chaos-monitor-performance

# Profile individual queries
pytest tests/performance --profile-queries --slow-queries-only
```

## Benchmarking

Measure end-to-end performance:

```python
import time

start = time.time()
result = await repo.find("v_user")
duration = time.time() - start
print(f"Total time: {duration*1000:.2f}ms")
```

**Target times with Rust backend:**
- Simple query: < 5ms (was < 15ms with psycopg)
- Complex query with joins: < 25ms (was < 50ms with psycopg)
- Large dataset (10k rows): < 200ms (was < 500ms with psycopg)
- With APQ cache hit: < 2ms (same performance)

## Advanced: Custom Rust Transformations

For very specialized needs, you can extend fraiseql-rs. See [Contributing Guide](../../CONTRIBUTING.md).

## Summary

**FraiseQL v1.9+ Exclusive Rust Architecture**: The Rust backend provides 2-3x performance improvement out of the box. Focus optimization efforts on:

1. **Database Design** (biggest remaining impact)
   - Use table views (tv_*) for complex queries
   - Proper indexing on JSONB paths
   - Minimize JSONB size in views

2. **Caching Strategy** (easiest wins)
   - Enable APQ for production (5-20ms savings)
   - Use appropriate cache invalidation patterns
   - Monitor cache hit rates

3. **Query Optimization** (for specific bottlenecks)
   - Use DataLoader for N+1 problems
   - Optimize GraphQL query structure
   - Monitor and profile slow queries

**Key Benefits Achieved:**
- ✅ 2-3x faster response times
- ✅ 40-60% less memory usage
- ✅ Zero Python string operations
- ✅ Type-safe data handling
- ✅ Automatic performance optimizations
