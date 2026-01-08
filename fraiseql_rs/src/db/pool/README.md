# Connection Pool Abstraction Layer

## Overview

The pool abstraction layer provides a trait-based interface for database connection pooling, enabling the storage layer to work with any pool implementation without coupling to specific implementations like deadpool or sqlx.

## Architecture

```
Engine Initialization
    ↓
    Creates DatabaseConfig from URL
    ↓
    Creates ProductionPool (deadpool-postgres)
    ↓
    Wraps as Arc<dyn PoolBackend>
    ↓
PostgresBackend (storage layer)
    ↓
    Uses PoolBackend trait abstraction
    ↓
Executes queries/statements
```

## Key Components

### `PoolBackend` Trait (`traits.rs`)

Defines the interface all pool implementations must implement:

```rust
#[async_trait]
pub trait PoolBackend: Send + Sync {
    /// Execute a query (SELECT) - returns JSONB values from column 0
    async fn query(&self, sql: &str) -> PoolResult<Vec<serde_json::Value>>;

    /// Execute a statement (INSERT/UPDATE/DELETE) - returns rows affected
    async fn execute(&self, sql: &str) -> PoolResult<u64>;

    /// Get pool metadata/info
    fn pool_info(&self) -> serde_json::Value;

    /// Get backend name for identification
    fn backend_name(&self) -> &str;
}
```

### `ProductionPool` Implementation

Located in `pool_production.rs`, implements `PoolBackend` trait:

- Uses **deadpool-postgres** for connection pooling
- Implements JSONB extraction from column 0 (FraiseQL's CQRS pattern)
- Automatic retry logic with exponential backoff for deadlock errors
- SSL/TLS support (configurable at runtime)
- Metrics collection for monitoring

## Integration Points

### 1. Engine Initialization (`api/engine.rs`)

```rust
let db_config = DatabaseConfig::from_url(db_url)?;
let pool = ProductionPool::new(db_config)?;
let pool_backend: Arc<dyn PoolBackend> = Arc::new(pool);
let storage = PostgresBackend::with_pool(pool_backend)?;
```

### 2. Storage Backend (`api/storage/postgres.rs`)

```rust
pub struct PostgresBackend {
    pool: Arc<dyn PoolBackend>,
}

impl StorageBackend for PostgresBackend {
    async fn query(&self, sql: &str, _params: &[serde_json::Value]) -> Result<QueryResult, StorageError> {
        let rows = self.pool.query(sql).await?;
        Ok(QueryResult { rows, ... })
    }
}
```

## Benefits

1. **Abstraction**: Storage layer depends on trait, not concrete pool types
2. **Flexibility**: Easy to swap pool implementations (deadpool → sqlx, etc.)
3. **Testability**: Mock pools can implement the trait for unit testing
4. **Clear Separation**: Pool management is separate from query execution
5. **Composability**: Multiple pool implementations can coexist

## FraiseQL's JSONB Pattern

The pool abstraction enforces FraiseQL's CQRS pattern:

- **Query Results**: JSONB data is in column 0, returned as `Vec<serde_json::Value>`
- **No Row-to-JSON Conversion**: PostgreSQL handles JSONB directly
- **Direct Extraction**: `ProductionPool::execute_query()` extracts from column 0

Example:
```sql
SELECT entity_data FROM users_view;  -- entity_data is JSONB in column 0
-- Returned as Vec<serde_json::Value>
```

## File Structure

```
fraiseql_rs/src/db/
├── pool.rs                 # Main pool module + Python binding (DatabasePool)
├── pool/
│   ├── traits.rs          # PoolBackend trait definition
│   └── README.md          # This file
├── pool_config.rs         # DatabaseConfig for URL parsing
├── pool_production.rs     # ProductionPool (deadpool-postgres) implementation
└── ...
```

## Discoverability

To understand the pool abstraction:

1. **Start**: `db/pool.rs` - Overview and module exports
2. **Trait**: `db/pool/traits.rs` - PoolBackend trait interface
3. **Implementation**: `db/pool_production.rs` - ProductionPool
4. **Usage**: `api/engine.rs` - How engine creates and uses pools
5. **Storage**: `api/storage/postgres.rs` - How storage uses pools

## Extension Points

To add a new pool implementation:

1. Create a new type (e.g., `SqlxPool`)
2. Implement `PoolBackend` trait
3. Update engine initialization to support configuration
4. No changes needed to storage layer!

Example:

```rust
pub struct SqlxPool {
    pool: sqlx::PgPool,
}

#[async_trait]
impl PoolBackend for SqlxPool {
    async fn query(&self, sql: &str) -> PoolResult<Vec<serde_json::Value>> {
        // Implementation...
    }
    // ...
}
```

## Configuration

Pools are configured via `DatabaseConfig`:

```rust
let config = DatabaseConfig::from_url("postgresql://user:pass@localhost/db")?
    .with_max_size(20)
    .with_ssl_mode(SslMode::Require);

let pool = ProductionPool::new(config)?;
```

## Monitoring

`ProductionPool` provides metrics:

```rust
let metrics = pool.metrics();
let stats = pool.stats();  // size, available, max_size
let info = pool.pool_info();  // JSON with database info
```

## Thread Safety

- `PoolBackend` requires `Send + Sync`
- `ProductionPool` is `Clone` and thread-safe (Arc-wrapped)
- Multiple threads can safely share `Arc<dyn PoolBackend>`
