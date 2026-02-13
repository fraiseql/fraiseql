# Connection Pooling Integration Guide

This guide demonstrates how to integrate fraiseql-wire with connection pooling libraries to manage multiple concurrent connections efficiently.

## Overview

fraiseql-wire is designed for streaming individual queries with bounded memory. For applications requiring concurrent access to multiple connections, connection pooling is essential.

## Connection Pool Architecture

### Why Connection Pooling Matters

- **Resource Efficiency**: Reuse TCP connections across queries
- **Concurrency Control**: Limit simultaneous Postgres connections
- **Query Isolation**: Each stream maintains its own connection
- **Graceful Degradation**: Pool queue backs up under load instead of failing

### fraiseql-wire Connection Model

Each `FraiseClient` connection:

- Maintains **one active query** at a time
- Automatically cancels queries when streams are dropped
- Supports pause/resume for backpressure control
- Is safe to use across `await` boundaries

## Integration Patterns

### Pattern 1: Manual Connection Management

Suitable for applications with light concurrency (< 10 concurrent queries):

```rust
use fraiseql_wire::FraiseClient;
use tokio::task;

async fn query_with_manual_pool() -> Result<(), Box<dyn std::error::Error>> {
    let mut clients = Vec::new();

    // Create a small pool of connections
    for i in 0..5 {
        let client = FraiseClient::connect("postgres://localhost/mydb").await?;
        clients.push(client);
    }

    // Use clients in parallel
    let handles: Vec<_> = clients.into_iter().enumerate().map(|(idx, client)| {
        task::spawn(async move {
            client
                .query("project")
                .where_sql(&format!("id = {}", idx))
                .execute()
                .await
        })
    }).collect();

    // Wait for all queries
    for handle in handles {
        let _stream = handle.await??;
    }

    Ok(())
}
```

**Pros**: Simple, no external dependencies
**Cons**: Manual connection lifecycle management, no dynamic pooling

### Pattern 2: deadpool Integration

Suitable for production applications with moderate concurrency (10-50 connections):

```toml
[dependencies]
fraiseql-wire = "0.1"
deadpool = "0.10"
tokio = { version = "1", features = ["full"] }
```

```rust
use deadpool::managed::{Object, Pool, PoolError};
use fraiseql_wire::FraiseClient;
use std::fmt;

/// Error type for deadpool integration
#[derive(Debug)]
pub struct PoolConnectionError(String);

impl fmt::Display for PoolConnectionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for PoolConnectionError {}

/// Manager for creating fraiseql-wire connections
pub struct FraiseClientManager {
    connection_string: String,
}

impl FraiseClientManager {
    pub fn new(connection_string: String) -> Self {
        Self { connection_string }
    }
}

#[async_trait::async_trait]
impl deadpool::managed::Manager for FraiseClientManager {
    type Type = FraiseClient;
    type Error = PoolConnectionError;

    async fn create(&self) -> Result<Self::Type, Self::Error> {
        FraiseClient::connect(&self.connection_string)
            .await
            .map_err(|e| PoolConnectionError(e.to_string()))
    }

    async fn recycle(
        &self,
        _conn: &mut Self::Type,
    ) -> deadpool::managed::RecycleResult<Self::Error> {
        // fraiseql-wire handles connection validation internally
        Ok(())
    }
}

/// Create a connection pool
pub fn create_pool(
    connection_string: &str,
    pool_size: usize,
) -> Pool<FraiseClientManager> {
    let manager = FraiseClientManager::new(connection_string.to_string());
    Pool::builder(manager)
        .max_size(pool_size)
        .build()
        .expect("Failed to create pool")
}

// Usage
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = create_pool("postgres://localhost/mydb", 10);

    // Get a connection from the pool
    let client = pool.get().await?;

    let stream = client
        .query("project")
        .where_sql("status = 'active'")
        .execute()
        .await?;

    // Stream is automatically released when dropped
    drop(stream);

    // Connection is returned to the pool automatically
    drop(client);

    Ok(())
}
```

**Pros**: Async-native, automatic connection reuse, configurable pool size
**Cons**: Requires async-trait dependency

### Pattern 3: bb8 Integration

Suitable for applications needing connection pooling with built-in health checks:

```toml
[dependencies]
fraiseql-wire = "0.1"
bb8 = "0.8"
tokio = { version = "1", features = ["full"] }
```

```rust
use bb8::Pool;
use fraiseql_wire::FraiseClient;
use std::error::Error as StdError;
use std::fmt;

#[derive(Debug)]
pub struct FraiseConnectionManager {
    connection_string: String,
}

impl FraiseConnectionManager {
    pub fn new(connection_string: String) -> Self {
        Self { connection_string }
    }
}

#[async_trait::async_trait]
impl bb8::ManageConnection for FraiseConnectionManager {
    type Connection = FraiseClient;
    type Error = Box<dyn StdError + Send + Sync>;

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        FraiseClient::connect(&self.connection_string)
            .await
            .map_err(|e| Box::new(e) as Box<dyn StdError + Send + Sync>)
    }

    async fn is_valid(
        &self,
        _conn: &mut Self::Connection,
    ) -> Result<(), Self::Error> {
        // fraiseql-wire connections are valid until dropped
        Ok(())
    }

    fn has_broken(&self, _conn: &mut Self::Connection) -> bool {
        // Connection remains valid unless explicitly dropped
        false
    }
}

// Usage
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = FraiseConnectionManager::new(
        "postgres://localhost/mydb".to_string(),
    );

    let pool = Pool::builder()
        .max_size(20)
        .build(manager)
        .await?;

    // Get a connection from the pool
    let client = pool.get().await?;

    let stream = client
        .query("project")
        .execute()
        .await?;

    Ok(())
}
```

**Pros**: Mature library, built-in health checks, excellent documentation
**Cons**: Slightly heavier weight than deadpool

### Pattern 4: SQLx-style Connection Pool (Custom)

For applications wanting a lightweight pool with query timeout support:

```rust
use fraiseql_wire::FraiseClient;
use tokio::sync::Semaphore;
use std::sync::Arc;

pub struct FraisePool {
    semaphore: Arc<Semaphore>,
    connection_string: String,
}

impl FraisePool {
    pub fn new(connection_string: String, max_connections: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_connections)),
            connection_string,
        }
    }

    pub async fn acquire(&self) -> Result<FraisePoolConnection, Box<dyn std::error::Error>> {
        let permit = self.semaphore.acquire().await?;
        let client = FraiseClient::connect(&self.connection_string).await?;

        Ok(FraisePoolConnection {
            _permit: permit,
            client,
        })
    }
}

pub struct FraisePoolConnection {
    _permit: tokio::sync::SemaphorePermit<'static>,
    pub client: FraiseClient,
}

// Usage
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = FraisePool::new(
        "postgres://localhost/mydb".to_string(),
        10,
    );

    let conn = pool.acquire().await?;
    let stream = conn.client
        .query("project")
        .execute()
        .await?;

    Ok(())
}
```

**Pros**: Minimal dependencies, lightweight, direct control
**Cons**: No automatic health checks, manual lifecycle management

## Best Practices

### 1. Pool Size Configuration

```rust
// Rule of thumb: connections = CPU cores + disk spindles
// For most cloud deployments:
let pool_size = if std::thread::available_parallelism().is_ok() {
    std::thread::available_parallelism().unwrap().get() * 2
} else {
    16  // reasonable default
};
```

### 2. Query Timeout Patterns

fraiseql-wire queries can be wrapped with timeouts:

```rust
use tokio::time::{timeout, Duration};

let pool = create_pool("postgres://localhost/mydb", 10);
let client = pool.get().await?;

match timeout(
    Duration::from_secs(30),
    client.query("project").execute()
) {
    Ok(Ok(stream)) => {
        // Process stream
    }
    Ok(Err(e)) => eprintln!("Query error: {}", e),
    Err(_) => eprintln!("Query timeout after 30s"),
}
```

### 3. Graceful Shutdown

```rust
use tokio::signal;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = create_pool("postgres://localhost/mydb", 10);

    // Run server
    tokio::spawn(async {
        server_loop(&pool).await
    });

    // Wait for shutdown signal
    signal::ctrl_c().await?;

    // Drop pool - all connections close gracefully
    drop(pool);

    println!("Graceful shutdown complete");
    Ok(())
}
```

### 4. Connection Lifecycle Awareness

Remember fraiseql-wire's connection model:

```rust
let client = pool.get().await?;

// ✅ CORRECT: One query at a time
let stream1 = client.query("project").execute().await?;
drop(stream1);  // Explicitly drop when done
let stream2 = client.query("user").execute().await?;

// ❌ WRONG: Overlapping queries on same connection
let stream1 = client.query("project").execute().await?;
let stream2 = client.query("user").execute().await?;  // Will fail!
```

## Performance Considerations

### Connection Setup Overhead

- **TCP connection**: ~250ns (mostly network latency)
- **Postgres authentication**: 1-5ms (depends on auth method)
- **Query planning**: 2-10ms (depends on schema)

**Implication**: Reusing connections saves 5-20ms per query

### Pool Contention

Monitor pool queue depth:

```rust
// For deadpool
let pool_status = pool.status();
println!("Active: {}, Waiting: {}",
    pool_status.size(),
    pool_status.min_idle());
```

### Memory Per Connection

- Idle connection: ~1.3 KB (bounded by chunk size)
- Active streaming query: ~1.3 KB + channel buffers
- Typical pool overhead: < 100 KB for 50 connections

## Troubleshooting

### "Too many connections" errors

**Symptom**: Connection pool rejects new connections
**Cause**: Postgres `max_connections` exceeded
**Fix**:

```sql
-- Check current limit
SHOW max_connections;

-- Increase if safe (default: 100)
ALTER SYSTEM SET max_connections = 200;

-- Reload Postgres
SELECT pg_reload_conf();

-- Or adjust pool size down
```

### Connection timeout during pool.get()

**Symptom**: Requests wait forever for available connections
**Cause**: All pool connections in use, new queries blocked
**Fix**:

```rust
// Add timeout to pool acquisition
match timeout(Duration::from_secs(5), pool.get()).await {
    Ok(Ok(conn)) => { /* use connection */ }
    Ok(Err(e)) => eprintln!("Pool error: {}", e),
    Err(_) => eprintln!("Connection timeout after 5s"),
}
```

### Slow query performance despite pooling

**Symptom**: Individual queries are slow even with pooled connections
**Cause**: May not be pool-related (check SQL plan)
**Debug**:

```rust
// Add tracing to see actual execution times
client
    .query("project")
    .where_sql("project__status__name = 'active'")
    .execute()
    .await
    .inspect_err(|e| eprintln!("Error: {}", e))
```

## Example: Production-Ready Application

```rust
use deadpool::managed::Pool;
use fraiseql_wire::FraiseClient;
use std::env;
use tokio::signal;

type PooledClient = deadpool::managed::Object<FraiseClientManager>;

#[derive(Clone)]
pub struct AppState {
    pool: Pool<FraiseClientManager>,
}

impl AppState {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let db_url = env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://localhost/mydb".to_string());

        let pool_size = env::var("DB_POOL_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);

        let manager = FraiseClientManager::new(db_url);
        let pool = Pool::builder(manager)
            .max_size(pool_size)
            .build()?;

        Ok(Self { pool })
    }

    pub async fn get_connection(&self) -> Result<PooledClient, Box<dyn std::error::Error>> {
        self.pool.get().await.map_err(|e| Box::new(e) as _)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let state = AppState::new().await?;

    // Start server in background
    let server_handle = tokio::spawn(run_server(state.clone()));

    // Wait for shutdown
    signal::ctrl_c().await?;
    println!("Shutting down...");

    drop(server_handle);
    Ok(())
}

async fn run_server(state: AppState) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        let conn = state.get_connection().await?;
        let _stream = conn.client.query("project").execute().await?;
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}
```

## See Also

- **PERFORMANCE_TUNING.md** – Memory and latency optimization
- **TYPED_STREAMING_GUIDE.md** – Type-safe result deserialization
- **examples/config.rs** – Connection configuration patterns
