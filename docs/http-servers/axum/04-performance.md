# Performance Tuning: Axum

**Version**: 2.0.0+
**Reading Time**: 35 minutes
**Audience**: DevOps engineers, backend developers
**Prerequisites**: Completed [Production Deployment](./03-deployment.md)

---

## Overview

This guide covers optimizing your Axum GraphQL server for high performance:
- ✅ HTTP/2 connection optimization
- ✅ Connection pool tuning
- ✅ Buffer and payload optimization
- ✅ Batch request processing
- ✅ Caching strategies
- ✅ Benchmarking techniques
- ✅ Profiling and measurement
- ✅ Common bottlenecks and fixes

---

## Measuring Current Performance

### Baseline Benchmarks

Before optimizing, establish baselines:

```bash
# Simple benchmark tool
cargo install apache2-utils

# Baseline: 100 concurrent requests, 1000 total
ab -n 1000 -c 100 http://localhost:8000/graphql

# Result: Requests per second, response times
# Example output:
# Requests per second:    2500.0 [#/sec] (mean)
# Time per request:       4.0ms (mean, across all concurrent requests)
```

### Rust Benchmarking with Criterion

**Add to Cargo.toml**:
```toml
[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }
```

**Create benchmark**:
```rust
// benches/graphql_benchmark.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn graphql_query_benchmark(c: &mut Criterion) {
    c.bench_function("simple_query", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let query = black_box(r#"{ users { id name } }"#);
                execute_graphql(query).await
            });
    });
}

criterion_group!(benches, graphql_query_benchmark);
criterion_main!(benches);
```

**Run benchmarks**:
```bash
cargo bench

# Output: Detailed timing with confidence intervals
# criterion generates HTML reports in target/criterion/
```

### Profiling Tools

**flamegraph (identify hot spots)**:
```bash
# Install
cargo install flamegraph

# Run with profiling
cargo flamegraph --bin my-graphql-api

# Generates flamegraph.svg showing where CPU time is spent
```

**perf (Linux)**:
```bash
# Profile execution
perf record -g target/release/my-graphql-api

# View results
perf report

# Generate flamegraph
perf script | stackcollapse-perf.pl | flamegraph.pl > perf.svg
```

---

## HTTP/2 Optimization

### Enable HTTP/2

**Via tokio-tungstenite for HTTP/2 support**:
```rust
use axum::http::Version;

// Axum automatically supports HTTP/2 via hyper
let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await?;

// HTTP/2 enabled by default with modern versions
axum::serve(listener, app).await?;
```

### Multiplexing Configuration

HTTP/2 multiplexes multiple streams over one connection:

```rust
use hyper::server::conn::http2;

// Configure HTTP/2 settings
let h2 = http2::Builder::new(executor)
    .max_concurrent_streams(Some(128))  // Per connection
    .initial_window_size(65535)         // Flow control
    .initial_connection_window_size(1048576);  // Connection-level

// Apply to server configuration
```

### Connection Management

**Keep-Alive Configuration**:
```rust
use std::time::Duration;

// Configure TCP keep-alive
let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await?;
listener.set_socket_config(|socket| {
    socket.set_tcp_nodelay(true)?;      // Disable Nagle's algorithm
    socket.set_tcp_keepalive(
        Duration::from_secs(120)
    )?;
    Ok(())
})?;
```

### Flow Control Tuning

```rust
// Large payloads need larger windows
const INITIAL_WINDOW_SIZE: u32 = 65_535;
const CONNECTION_WINDOW_SIZE: u32 = 1_048_576;  // 1MB

// For applications with large GraphQL responses:
// Increase window sizes proportionally
const LARGE_RESPONSE_WINDOW: u32 = 10_485_760;  // 10MB
```

---

## Connection Pool Tuning

### Database Connection Pool

**Optimal sizing formula**:
```
connections = (core_count * 2) + max_overflow

For 4-core server:  (4 * 2) + 3 = 11 connections
For 8-core server:  (8 * 2) + 3 = 19 connections
For 16-core server: (16 * 2) + 3 = 35 connections
```

**Configuration**:
```rust
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

let pool = PgPoolOptions::new()
    .min_connections(5)              // Minimum idle connections
    .max_connections(20)             // Maximum total connections
    .acquire_timeout(Duration::from_secs(30))
    .idle_timeout(Duration::from_secs(900))  // 15 minutes
    .max_lifetime(Duration::from_secs(1800)) // 30 minutes
    .test_before_acquire(true)       // Verify connection health
    .connect(&database_url)
    .await?;
```

### Connection Pool Monitoring

```rust
// Check pool statistics
let available = pool.num_idle();
let busy = pool.size() - pool.num_idle();

println!("Pool - Idle: {}, Busy: {}, Total: {}",
    available, busy, pool.size());

// Alert if pool exhaustion approaching
if busy > (pool.size() as f32 * 0.9) as u32 {
    log::warn!("Connection pool nearing capacity!");
}
```

### Warm Up Pool on Startup

```rust
// Pre-allocate connections on startup
async fn warmup_pool(pool: &PgPool) -> Result<()> {
    for _ in 0..pool.max_connections {
        let _ = pool.acquire().await?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let pool = create_pool().await?;
    warmup_pool(&pool).await?;  // Pre-warm

    // Start server...
    Ok(())
}
```

---

## Buffer and Payload Optimization

### Request Body Size Limits

```rust
use axum::extract::DefaultBodyLimit;

// Set appropriate limit based on use case
let body_limit = DefaultBodyLimit::max(10_485_760);  // 10MB

let app = Router::new()
    .layer(body_limit);
```

### Response Streaming for Large Payloads

```rust
use axum::response::{IntoResponse, StreamBody};
use futures::stream::StreamExt;

async fn large_query_handler() -> impl IntoResponse {
    let stream = futures::stream::iter(vec![
        serde_json::json!({"id": 1, "name": "User 1"}),
        serde_json::json!({"id": 2, "name": "User 2"}),
        // ... large dataset
    ])
    .map(Ok::<_, Infallible>);

    StreamBody::new(stream)
}
```

### Compression Tuning

```rust
use tower_http::compression::CompressionLayer;

let compression = CompressionLayer::new()
    .br(true)                              // Brotli (best compression)
    .zstd(true)                            // Zstandard (balanced)
    .gzip(true)                            // Gzip (compatibility)
    .compress_when(
        tower_http::compression::predicate::SizeAbove::new(1024)
    )
    .compress_when(
        tower_http::compression::predicate::Predicate::and(
            tower_http::compression::predicate::SizeAbove::new(1024),
            tower_http::compression::predicate::ContentTypeFilter::new()
                .compress_text()
                .compress_json()
        )
    );

let app = Router::new().layer(compression);
```

### JSON Serialization Optimization

```rust
use serde_json::json;

// Use simd-json for faster JSON parsing/serialization
// Add to Cargo.toml: simd-json = "0.13"

use simd_json::to_string;

// Benchmark difference:
// serde_json:  ~1000 ns per operation
// simd-json:   ~300 ns per operation (3x faster)
```

---

## Batch Request Processing

### Batch GraphQL Requests

```rust
use axum::Json;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct BatchRequest {
    requests: Vec<GraphQLRequest>,
}

#[derive(Serialize)]
struct BatchResponse {
    results: Vec<GraphQLResponse>,
}

async fn batch_graphql_handler(
    Json(batch): Json<BatchRequest>,
) -> impl IntoResponse {
    let results = futures::future::join_all(
        batch.requests.into_iter()
            .map(|req| execute_query(req))
    )
    .await;

    Json(BatchResponse { results })
}

let app = Router::new()
    .route("/graphql/batch", post(batch_graphql_handler));
```

### Request Deduplication

```rust
use std::collections::HashMap;

async fn deduplicated_batch_handler(
    Json(batch): Json<BatchRequest>,
) -> impl IntoResponse {
    let mut unique_queries: HashMap<String, Vec<usize>> = HashMap::new();

    // Group identical queries
    for (idx, req) in batch.requests.iter().enumerate() {
        unique_queries
            .entry(req.query.clone())
            .or_insert_with(Vec::new)
            .push(idx);
    }

    // Execute only unique queries
    let mut results = vec![None; batch.requests.len()];
    for (query, indices) in unique_queries {
        let result = execute_query(&query).await;
        for idx in indices {
            results[idx] = Some(result.clone());
        }
    }

    Json(BatchResponse {
        results: results.into_iter().flatten().collect(),
    })
}
```

### Parallel Execution

```rust
use tokio::task;

async fn parallel_batch_handler(
    Json(batch): Json<BatchRequest>,
) -> impl IntoResponse {
    // Spawn tasks for parallel execution
    let handles: Vec<_> = batch.requests
        .into_iter()
        .map(|req| {
            task::spawn(async move {
                execute_query(req).await
            })
        })
        .collect();

    // Wait for all to complete
    let results = futures::future::try_join_all(handles)
        .await
        .unwrap_or_default();

    Json(BatchResponse { results })
}
```

---

## Caching Strategies

### Response Caching

```rust
use std::sync::Arc;
use dashmap::DashMap;
use std::time::{Duration, SystemTime};

#[derive(Clone)]
struct CacheEntry {
    data: String,
    created_at: SystemTime,
    ttl: Duration,
}

#[derive(Clone)]
struct ResponseCache {
    cache: Arc<DashMap<String, CacheEntry>>,
}

impl ResponseCache {
    fn is_expired(entry: &CacheEntry) -> bool {
        entry.created_at.elapsed()
            .map(|d| d > entry.ttl)
            .unwrap_or(false)
    }

    async fn get(&self, key: &str) -> Option<String> {
        self.cache.get(key)
            .and_then(|entry| {
                if Self::is_expired(&entry) {
                    None
                } else {
                    Some(entry.data.clone())
                }
            })
    }

    fn set(&self, key: String, data: String, ttl: Duration) {
        self.cache.insert(
            key,
            CacheEntry {
                data,
                created_at: SystemTime::now(),
                ttl,
            },
        );
    }
}

// Use in handler
async fn cached_graphql_handler(
    State(cache): State<ResponseCache>,
    Json(req): Json<GraphQLRequest>,
) -> impl IntoResponse {
    let cache_key = format!("{:?}", req);

    if let Some(cached) = cache.get(&cache_key).await {
        return cached;
    }

    let result = execute_query(&req).await;
    cache.set(cache_key, result.clone(), Duration::from_secs(300));
    result
}
```

### Query Result Caching

```rust
use moka::future::Cache;

let query_cache: Cache<String, String> = Cache::new(10_000);  // 10k entries

async fn execute_with_cache(
    query: &str,
    cache: &Cache<String, String>,
) -> Result<String> {
    // Check cache first
    if let Some(cached) = cache.get(query).await {
        return Ok(cached);
    }

    // Execute query
    let result = execute_query(query).await?;

    // Cache result
    cache.insert(query.to_string(), result.clone()).await;

    Ok(result)
}
```

### Cache Invalidation

```rust
// Tag-based invalidation
#[derive(Clone)]
struct TaggedCache {
    cache: Arc<DashMap<String, String>>,
    tags: Arc<DashMap<String, Vec<String>>>,  // tag -> keys
}

impl TaggedCache {
    fn set_with_tags(&self, key: String, value: String, tags: Vec<String>) {
        self.cache.insert(key.clone(), value);
        for tag in tags {
            self.tags
                .entry(tag)
                .or_insert_with(Vec::new)
                .push(key.clone());
        }
    }

    fn invalidate_tag(&self, tag: &str) {
        if let Some((_, keys)) = self.tags.remove(tag) {
            for key in keys {
                self.cache.remove(&key);
            }
        }
    }
}

// Usage
cache.set_with_tags(
    "user:123".to_string(),
    user_data,
    vec!["users".to_string(), "active".to_string()],
);

// When user is updated, invalidate all user-related caches
cache.invalidate_tag("users");
```

---

## Worker Thread Configuration

### Tokio Runtime Tuning

```rust
use tokio::runtime::Builder;

#[tokio::main(worker_threads = 8)]
async fn main() {
    // 8 worker threads instead of default (num_cpus)
}

// Or more control:
let runtime = Builder::new_multi_thread()
    .worker_threads(8)
    .max_blocking_threads(512)
    .thread_name("graphql-worker")
    .enable_all()
    .build()?;

runtime.block_on(async {
    // Your app
})
```

### Blocking Operations

```rust
use tokio::task;

// Don't block the async runtime
async fn blocking_operation_handler(
    Json(req): Json<Request>,
) -> impl IntoResponse {
    // Run CPU-intensive operation on blocking thread pool
    let result = task::block_in_place(|| {
        expensive_computation(&req)
    });

    Json(result)
}
```

---

## Memory Optimization

### Reduce Memory Allocations

```rust
// Use Vec::with_capacity to pre-allocate
let mut users = Vec::with_capacity(1000);

// Use String::with_capacity
let mut buffer = String::with_capacity(10_000);

// Pool buffers for reuse
use bytes::BytesMut;
let mut buffer = BytesMut::with_capacity(8192);
```

### Monitor Memory Usage

```rust
// Add to Cargo.toml: psutil = "3"

use std::process;

fn log_memory_usage() {
    let process = process::Command::new("ps")
        .args(&["-p", &std::process::id().to_string(), "-o", "rss"])
        .output()
        .ok();

    if let Some(output) = process {
        let rss = String::from_utf8_lossy(&output.stdout);
        println!("Memory usage: {} KB", rss.trim());
    }
}
```

### Avoid Large Clones

```rust
// ❌ Expensive
let response_clone = response.clone();

// ✅ Use Arc for shared ownership
use std::sync::Arc;
let response = Arc::new(response);
let response_ref = response.clone();  // Cheap reference count increment

// ✅ Use references
let response_ref = &response;
```

---

## Database Query Optimization

### Query Analysis

```bash
# Enable query logging
RUST_LOG=sqlx=debug cargo run

# Look for slow queries (> 100ms)
# Identify N+1 query patterns
```

### Batch Database Queries

```rust
// ❌ N+1 Query Problem
for user_id in user_ids {
    let user = fetch_user(user_id).await;  // 100 queries for 100 users!
}

// ✅ Batch Query
let users = fetch_users_batch(&user_ids).await;  // 1 query for 100 users!
```

### Connection Reuse

```rust
// ✅ Reuse single connection
let pool = create_pool().await?;

let user1 = fetch_user(&pool, 1).await?;
let user2 = fetch_user(&pool, 2).await?;
// Both use connections from same pool
```

---

## Common Performance Issues

### Issue 1: Connection Pool Exhaustion

**Symptoms**: "connection pool timed out"

**Diagnosis**:
```rust
// Check pool stats every 10 seconds
let pool = create_pool().await?;

tokio::spawn(async move {
    loop {
        tokio::time::sleep(Duration::from_secs(10)).await;
        let idle = pool.num_idle();
        let size = pool.size();
        let busy = size - idle;

        if busy > size / 2 {
            log::warn!("Pool busy: {}/{}", busy, size);
        }
    }
});
```

**Fix**:
```rust
// Increase pool size
.max_connections(50)  // Was 20

// Reduce query time (see database optimization)

// Add connection timeout monitoring
.acquire_timeout(Duration::from_secs(60))
```

### Issue 2: Memory Leaks

**Symptoms**: Memory usage grows over time

**Diagnosis**:
```bash
# Run with profiling
cargo flamegraph

# Check for:
# - Unbounded caches
# - Leaked connections
# - Circular Arc references
```

**Fix**: Use bounded caches with TTL:
```rust
let cache = Cache::builder()
    .max_capacity(10_000)  // Bound size
    .build();
```

### Issue 3: Slow JSON Serialization

**Symptoms**: High CPU, requests taking > 10ms

**Diagnosis**:
```rust
let start = std::time::Instant::now();
let json = serde_json::to_string(&result)?;
println!("Serialization: {}ms", start.elapsed().as_millis());
```

**Fix**: Use simd-json or cache serialized responses

### Issue 4: Lock Contention

**Symptoms**: CPU high but individual requests fast

**Diagnosis**:
```rust
// Identify locks with flamegraph
cargo flamegraph

// Look for functions spending time in locks
```

**Fix**: Use lock-free data structures (DashMap instead of RwLock<HashMap>)

---

## Performance Checklist

Before deploying to production:

- [ ] Baselines established (ab, criterion)
- [ ] HTTP/2 enabled and verified
- [ ] Connection pool sized for your workload
- [ ] Response compression configured
- [ ] Batch requests supported
- [ ] Caching strategy implemented
- [ ] Database queries optimized (no N+1)
- [ ] Memory allocations minimized
- [ ] Worker threads configured
- [ ] Monitoring/alerting in place
- [ ] Load tested (1000+ QPS)
- [ ] Flamegraph reviewed
- [ ] Memory usage stable over time

---

## Next Steps

- **Seeing slowness in production?** → [Troubleshooting](./05-troubleshooting.md)
- **Back to Deployment?** → [Production Deployment](./03-deployment.md)
- **Getting started?** → [Getting Started](./01-getting-started.md)

---

**You're now optimized for scale!** Monitor metrics and adjust based on real-world performance. 🚀
