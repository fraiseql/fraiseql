# Connection Pool Tuning Guide

**Version**: 2.0.0-a1
**Framework**: deadpool-postgres (async connection pool)
**Impact**: Critical for production performance

## Overview

Connection pooling is essential for GraphQL API performance. A properly tuned connection pool can improve throughput by 2-3x and reduce latency variance.

## Current Configuration

### Default Settings

| Setting | Value | Notes |
|---------|-------|-------|
| **Max Connections** | 10 | Good for small-medium apps |
| **Initialization** | Lazy | Connections created on-demand |
| **Recycling** | Fast | Connections reused immediately |
| **Idle Timeout** | 900s | Connections dropped after 15 minutes idle |

### What This Means

```
Behavior:
1. Server starts with 0 connections
2. First request creates 1 connection
3. Pool grows to max_size (10) as needed
4. Unused connections closed after 15 minutes
```

## Tuning by Workload

### Small Applications (Development, Low Traffic)

**Characteristics**:
- < 100 requests/hour
- < 10 concurrent connections needed
- Single server deployment

**Configuration**:
```rust
let adapter = PostgresAdapter::with_pool_size(
    connection_string,
    5  // Small pool is sufficient
).await?;
```

**Expected Metrics**:
- Pool utilization: 20-40%
- Connection reuse: High
- Latency: < 50ms p95

### Medium Applications (Staging, Moderate Traffic)

**Characteristics**:
- 1K-10K requests/hour
- 10-50 concurrent connections
- Single or dual server

**Configuration** (Recommended):
```rust
let adapter = PostgresAdapter::with_pool_size(
    connection_string,
    20  // Default 10 is often too small
).await?;
```

**Expected Metrics**:
- Pool utilization: 40-70%
- Connection reuse: High
- Latency: < 100ms p95

### Large Applications (Production)

**Characteristics**:
- 100K+ requests/hour
- 50-200 concurrent connections
- Multiple servers (load balanced)

**Configuration**:
```rust
let adapter = PostgresAdapter::with_pool_size(
    connection_string,
    50 + num_cpus::get() as usize  // Scale with CPU cores
).await?;
```

**Expected Metrics**:
- Pool utilization: 50-80%
- Connection reuse: High
- Latency: < 150ms p95
- Queueing: < 10ms p95

## Tuning for Concurrency

### Rule of Thumb

```
max_pool_size = (core_count × 2) + effective_spindle_count

For typical cloud VMs:
- 2 cores:  5 connections
- 4 cores:  10 connections (default)
- 8 cores:  20 connections
- 16 cores: 35 connections
- 32 cores: 65 connections
```

### Rationale

Each connection:
- Occupies ~1-2 MB of database memory
- Requires PostgreSQL backend process (~5-10 MB)
- Handles one query at a time

Too small pool → Requests queue, latency increases
Too large pool → Wasted memory, database may struggle

## Monitoring Pool Health

### Key Metrics

```rust
let metrics = adapter.pool_metrics();

println!("Total connections:  {}", metrics.total_connections);
println!("Idle connections:   {}", metrics.idle_connections);
println!("Active connections: {}", metrics.active_connections);
println!("Waiting requests:   {}", metrics.waiting_requests);

// Calculate utilization
let utilization = (metrics.active_connections as f64
    / metrics.total_connections as f64) * 100.0;
println!("Pool utilization: {:.1}%", utilization);
```

### Health Signals

| Metric | Good | Warning | Critical |
|--------|------|---------|----------|
| Utilization | 40-70% | 70-90% | >90% |
| Waiting Requests | 0 | 1-5 | >5 |
| Idle Connections | >0 | = 0 | - |
| Acquisition Time | < 1ms | 1-10ms | > 10ms |

### How to Monitor

**Option 1: Debug Logging**

```rust
// Add monitoring to your GraphQL handler
async fn handle_graphql(req: GraphQLRequest) -> Result<String> {
    let before = Instant::now();
    let metrics_before = executor.adapter.pool_metrics();

    let result = executor.execute(&req.query, &req.variables).await?;

    let metrics_after = executor.adapter.pool_metrics();
    let elapsed = before.elapsed();

    if elapsed.as_millis() > 100 {
        eprintln!(
            "Slow query ({:.1}ms): active={}, waiting={}",
            elapsed.as_secs_f64() * 1000.0,
            metrics_after.active_connections,
            metrics_after.waiting_requests
        );
    }

    Ok(result)
}
```

**Option 2: Prometheus Metrics**

```rust
// Export pool metrics to Prometheus
prometheus::histogram_timer!("pool_acquisition_time_ms", {
    executor.adapter.execute_query(query).await?
});

prometheus::gauge!("pool_active_connections",
    executor.adapter.pool_metrics().active_connections as i64);
```

**Option 3: Structured Logging**

```rust
// Log pool state with every query
tracing::info!(
    pool_metrics = ?executor.adapter.pool_metrics(),
    query_latency_ms = ?elapsed.as_millis(),
    "GraphQL query executed"
);
```

## Optimization Techniques

### 1. Pre-warm the Pool

Initialize connections on startup:

```rust
async fn initialize_adapter(connection_string: &str) -> Result<PostgresAdapter> {
    let adapter = PostgresAdapter::with_pool_size(connection_string, 20).await?;

    // Pre-warm by creating initial connections
    for _ in 0..5 {
        adapter.health_check().await?;
    }

    println!("Pool initialized with 5 connections");
    Ok(adapter)
}
```

**Benefit**: Eliminates cold-start latency spike

### 2. Connection Recycling

Fast recycling is already enabled. Verify:

```rust
cfg.manager = Some(ManagerConfig {
    recycling_method: RecyclingMethod::Fast,  // Reuse immediately
});
```

**Benefit**: Faster connection reuse, lower latency

### 3. Tune Idle Timeout

```rust
// Default: 900 seconds (15 minutes)
// Reduces unnecessary cleanup for long-lived connections
cfg.manager = Some(ManagerConfig {
    recycling_method: RecyclingMethod::Fast,
});

// For high-churn workloads, reduce timeout:
// cfg.manager = Some(ManagerConfig {
//     idle_timeout: Some(Duration::from_secs(300)),  // 5 minutes
// });
```

**Benefit**: Reduces idle connection overhead

### 4. Use Connection Pooling in Client Code

```rust
// ✅ GOOD - Use single adapter instance
let adapter = Arc::new(PostgresAdapter::new(connection_string).await?);

#[tokio::main]
async fn main() {
    // Share adapter across handlers
    let adapter = adapter.clone();

    for _ in 0..100 {
        let adapter = adapter.clone();
        tokio::spawn(async move {
            adapter.execute_query(query).await
        });
    }
}
```

```rust
// ❌ BAD - Create new pool per request
async fn handle_request() {
    // Creates new connection pool each time!
    let adapter = PostgresAdapter::new(connection_string).await?;
    adapter.execute_query(query).await
}
```

### 5. Batch Queries When Possible

```rust
// ❌ SLOW - Each query gets different connection
for user_id in user_ids {
    let result = adapter.execute_where_query(
        "v_user",
        Some(&where_clause),
        None,
        None
    ).await?;
    process_result(result);
}

// ✅ FAST - Batch reduces connection churn
let results = futures::future::join_all(
    user_ids.iter().map(|id| {
        let clause = build_clause(id);
        adapter.execute_where_query("v_user", Some(&clause), None, None)
    })
).await;
```

**Benefit**: Reduces connection acquisition overhead

## Troubleshooting

### Problem: "Too many connections" Error

**Symptom**: Queries fail with connection pool exhaustion

**Cause**: Pool size too small for load

**Solutions**:

1. **Increase pool size**:
   ```rust
   // From 10 to 30
   let adapter = PostgresAdapter::with_pool_size(
       connection_string,
       30
   ).await?;
   ```

2. **Reduce query latency** (queries hold connections longer if slow):
   ```
   Enable SQL projection (already done) ✅
   Add database indexes
   Optimize WHERE clauses
   ```

3. **Load balance** across multiple servers:
   ```
   Server A: 8 connections
   Server B: 8 connections
   Total: 16 connections to database
   (vs 16 from single server)
   ```

### Problem: High Latency with Low CPU Usage

**Symptom**: p95 latency > 100ms even with low CPU

**Cause**: Connections are the bottleneck

**Solution**: Check pool metrics

```rust
let metrics = adapter.pool_metrics();
if metrics.waiting_requests > 0 {
    // Requests are waiting for connections
    // Increase pool size
}
```

### Problem: Connection Leaks (Pool Never Shrinks)

**Symptom**: `total_connections` keeps growing

**Cause**: Connections not being returned to pool (bug in code)

**Solution**: Ensure `DatabaseAdapter` calls are awaited properly

```rust
// ❌ WRONG - Connection held indefinitely
let result = adapter.execute_query(query);  // Not awaited!

// ✅ CORRECT - Connection returned after await
let result = adapter.execute_query(query).await?;
```

## Configuration Reference

### Creating Custom Pool Configuration

```rust
use deadpool_postgres::{Config, ManagerConfig, RecyclingMethod};

pub async fn create_adapter_with_config(
    connection_string: &str,
    max_connections: usize,
) -> Result<PostgresAdapter> {
    let mut cfg = Config::new();
    cfg.url = Some(connection_string.to_string());

    cfg.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    });

    cfg.pool = Some(deadpool_postgres::PoolConfig::new(max_connections));

    let pool = cfg.create_pool(
        Some(deadpool_postgres::Runtime::Tokio1),
        tokio_postgres::NoTls
    )?;

    Ok(PostgresAdapter::from_pool(pool))
}
```

## Benchmarking Pool Performance

### Simple Benchmark

```rust
#[tokio::test]
async fn bench_pool_acquisition() {
    let adapter = PostgresAdapter::with_pool_size(
        "postgresql://...",
        20
    ).await.unwrap();

    let iterations = 1000;
    let start = Instant::now();

    for _ in 0..iterations {
        let _ = adapter.health_check().await;
    }

    let elapsed = start.elapsed();
    let per_acquisition = elapsed.as_micros() / iterations;

    println!("Connection acquisition: {}µs", per_acquisition);
    assert!(per_acquisition < 1000, "Should be < 1ms");
}
```

### Expected Results

```
Pool size: 10
Acquisition time: 10-50µs (when connection available)
Acquisition time: 100-500µs (when creating new connection)
```

## Production Checklist

- [ ] Pool size configured based on core count
- [ ] Pre-warming enabled on startup
- [ ] Monitoring/logging in place
- [ ] Health check passes on startup
- [ ] Load testing confirms pool is adequate
- [ ] Alerts configured for pool exhaustion
- [ ] Idle timeout appropriate for workload
- [ ] Connection recycling fast method enabled

## Next Steps

1. **Measure your workload**: Monitor pool metrics
2. **Profile queries**: Identify slow queries
3. **Optimize**: Use SQL projection (already done)
4. **Re-tune**: Adjust pool size based on metrics
5. **Monitor production**: Track pool health

## Related Documentation

- [SQL Projection Optimization](./projection-optimization.md) - Reduce query latency
- [Performance Baselines](./projection-baseline-results.md) - Benchmark data
- [Architecture](../architecture/) - How database connections work

---

**Last Updated**: 2026-01-31
**Framework**: deadpool-postgres v0.14+
