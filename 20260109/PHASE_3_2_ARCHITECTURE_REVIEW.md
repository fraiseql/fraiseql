# Phase 3.2 Architecture Review: Query Execution & Parameter Binding

**Date**: January 8, 2026
**Framework**: FraiseQL v1.8.3+
**Scope**: Review of Phase 3.2 "Query Execution" against FraiseQL's architecture

---

## Executive Summary

Phase 3.2 aims to implement SQL query execution with parameter binding and result transformation. Given FraiseQL's **exclusive Rust pipeline** architecture and the **trait-based pool abstraction** already in place from Phase 3.1, this review identifies:

1. **Critical Architectural Patterns** to follow
2. **Antipatterns to avoid** that would violate FraiseQL's design principles
3. **Best practices** for integrating with existing components
4. **Integration points** with the Python layer

---

## Current FraiseQL Architecture

### Core Principle: Exclusive Rust Pipeline

```
GraphQL Query (Python)
       ↓
Rust Pipeline (fraiseql_rs)
       ↓
PostgreSQL (Real)
       ↓
Response (JSON)
```

**Key**: All query execution flows through the Rust pipeline. Python never directly queries the database—it delegates to Rust.

### Phase 3.1 Completion: Pool Abstraction

What we have now:
- ✅ `PoolBackend` trait in `fraiseql_rs/src/db/pool/traits.rs`
- ✅ `ProductionPool` implementing deadpool-postgres
- ✅ `DatabasePool` Python binding with context manager
- ✅ Configuration parsing in `DatabaseConfig`
- ✅ Connection pooling (max_size, timeouts, SSL modes)

---

## What Phase 3.2 Needs to Do

### Objective
Implement **query execution** that:
1. Takes SQL + parameters from the Planner
2. Binds parameters safely (no SQL injection)
3. Executes against PostgreSQL
4. Transforms results to JSON
5. Handles errors gracefully

### Scope
- Parameter binding for SELECT, INSERT, UPDATE, DELETE
- Result row transformation to JSON
- Transaction support (BEGIN, COMMIT, ROLLBACK)
- Error propagation with context
- No breaking changes to Phase 3.1 pool

---

## ✅ WHAT'S NEEDED (Correct Patterns)

### 1. Extend `PoolBackend` Trait (Not Replace It)

**Current State**: Phase 3.1 created `PoolBackend` trait with:
```rust
pub trait PoolBackend {
    async fn get_connection(&self) -> PoolResult<PooledConnection>;
    async fn health_check(&self) -> PoolResult<()>;
    // ... more methods
}
```

**Phase 3.2 Should**: Add query execution methods to `PoolBackend`:

```rust
pub trait PoolBackend {
    // Existing (Phase 3.1)
    async fn get_connection(&self) -> PoolResult<PooledConnection>;

    // New (Phase 3.2) - Query Execution
    async fn execute_query(
        &self,
        sql: &str,
        params: Vec<QueryParam>,
    ) -> PoolResult<QueryResult>;

    async fn execute_mutation(
        &self,
        sql: &str,
        params: Vec<QueryParam>,
    ) -> PoolResult<ExecuteResult>;

    async fn begin_transaction(&self) -> PoolResult<Transaction>;
}
```

**Why**: This keeps the pool abstraction complete. Storage implementations don't need to know about connection details.

### 2. Define Clear Data Types for Parameters & Results

**Create in `fraiseql_rs/src/db/types.rs`**:

```rust
/// A query parameter (handles different types safely)
#[derive(Clone, Debug)]
pub enum QueryParam {
    Null,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Json(serde_json::Value),
    Uuid(uuid::Uuid),
    DateTime(chrono::DateTime<chrono::Utc>),
    // PostgreSQL-specific types
    Array(Vec<QueryParam>),
}

/// A single row from a query result
pub type Row = serde_json::Map<String, serde_json::Value>;

/// Result of a SELECT query
#[derive(Clone, Debug)]
pub struct QueryResult {
    pub rows: Vec<Row>,
    pub columns: Vec<String>,
    pub execution_time_ms: u64,
}

/// Result of INSERT/UPDATE/DELETE
#[derive(Clone, Debug)]
pub struct ExecuteResult {
    pub rows_affected: u64,
    pub last_insert_id: Option<i64>,
    pub execution_time_ms: u64,
}
```

**Why**: Rust's type system prevents parameter injection. Clear types make intent obvious.

### 3. Implement Query Execution in `ProductionPool`

**File**: `fraiseql_rs/src/db/pool_production.rs`

```rust
impl PoolBackend for ProductionPool {
    async fn execute_query(
        &self,
        sql: &str,
        params: Vec<QueryParam>,
    ) -> PoolResult<QueryResult> {
        let start = std::time::Instant::now();

        let conn = self.get_connection().await?;

        // Build sqlx query with parameters
        let mut query = sqlx::query(sql);
        for param in params {
            query = bind_parameter(query, param)?;
        }

        let rows = query.fetch_all(&mut *conn).await
            .map_err(|e| PoolError::QueryFailed(e.to_string()))?;

        let columns = extract_columns(&rows);
        let result_rows = rows.into_iter()
            .map(row_to_json)
            .collect();

        Ok(QueryResult {
            rows: result_rows,
            columns,
            execution_time_ms: start.elapsed().as_millis() as u64,
        })
    }
}
```

**Why**: All execution stays in Rust. Python never directly queries. Thread-safe by design.

### 4. Create Parameter Binding Utilities

**File**: `fraiseql_rs/src/db/query_builder.rs`

```rust
/// Safely bind a parameter to a sqlx query
///
/// This function is the ONLY place where user input affects SQL execution.
/// All bindings use parameterized queries (no string interpolation).
pub fn bind_parameter<'q>(
    mut query: sqlx::query::Query<'q, sqlx::Postgres>,
    param: QueryParam,
) -> Result<sqlx::query::Query<'q, sqlx::Postgres>, PoolError> {
    match param {
        QueryParam::Null => query.bind::<Option<String>>(None),
        QueryParam::Boolean(b) => query.bind(b),
        QueryParam::Integer(i) => query.bind(i),
        QueryParam::Float(f) => query.bind(f),
        QueryParam::String(s) => query.bind(s),
        QueryParam::Json(j) => query.bind(j),
        // ... other types
    }
    Ok(query)
}

/// Extract column names and types from result set
fn extract_columns(rows: &[sqlx::postgres::PgRow]) -> Vec<String> {
    if rows.is_empty() {
        return vec![];
    }

    rows[0]
        .columns()
        .iter()
        .map(|col| col.name().to_string())
        .collect()
}

/// Convert a database row to JSON
fn row_to_json(row: sqlx::postgres::PgRow) -> Row {
    let mut map = serde_json::Map::new();

    for col in row.columns() {
        let value: serde_json::Value = /* extract value */;
        map.insert(col.name().to_string(), value);
    }

    map
}
```

**Why**: Single source of truth for parameter handling. Prevents SQL injection by construction.

### 5. Handle Transactions Properly

**File**: `fraiseql_rs/src/db/transaction.rs`

```rust
pub struct Transaction {
    conn: PooledConnection,
}

impl Transaction {
    pub async fn begin(pool: &dyn PoolBackend) -> PoolResult<Self> {
        let mut conn = pool.get_connection().await?;
        conn.execute("BEGIN").await
            .map_err(|e| PoolError::TransactionFailed(e.to_string()))?;
        Ok(Transaction { conn })
    }

    pub async fn execute(
        &mut self,
        sql: &str,
        params: Vec<QueryParam>,
    ) -> PoolResult<ExecuteResult> {
        // Execute within transaction
        // Don't commit yet—caller decides
    }

    pub async fn commit(self) -> PoolResult<()> {
        self.conn.execute("COMMIT").await
            .map_err(|e| PoolError::TransactionFailed(e.to_string()))?;
        Ok(())
    }

    pub async fn rollback(self) -> PoolResult<()> {
        self.conn.execute("ROLLBACK").await
            .map_err(|e| PoolError::TransactionFailed(e.to_string()))?;
        Ok(())
    }
}
```

**Why**: Transactions are critical for data consistency. Rust's move semantics ensure cleanup.

### 6. Update Executor to Use Query Execution

**File**: `fraiseql_rs/src/api/executor.rs`

```rust
pub struct Executor {
    pool: Arc<dyn PoolBackend>,  // Now has query methods
    cache: Arc<dyn CacheBackend>,
}

impl Executor {
    pub async fn execute_select(
        &self,
        sql: &str,
        params: Vec<QueryParam>,
    ) -> Result<QueryResult, ExecutorError> {
        // Try cache first
        let cache_key = format!("query:{}", sql);
        if let Ok(cached) = self.cache.get(&cache_key).await {
            return Ok(cached);
        }

        // Execute against pool
        let result = self.pool.execute_query(sql, params).await?;

        // Cache for future
        let _ = self.cache.set(&cache_key, result.clone(), 3600).await;

        Ok(result)
    }

    pub async fn execute_mutation(
        &self,
        sql: &str,
        params: Vec<QueryParam>,
    ) -> Result<ExecuteResult, ExecutorError> {
        // Mutations bypass cache
        let result = self.pool.execute_mutation(sql, params).await?;

        // Invalidate related cache entries
        self.cache.clear().await.ok();

        Ok(result)
    }
}
```

**Why**: Executor now has complete control. Cache is optional, pool is primary.

### 7. Create Comprehensive Error Types

**File**: `fraiseql_rs/src/db/errors.rs`

```rust
#[derive(Debug)]
pub enum PoolError {
    // Connection issues
    ConnectionFailed(String),
    ConnectionTimeout,
    PoolExhausted,

    // Query issues
    QueryFailed(String),
    InvalidParameter { param_name: String, reason: String },
    ParameterTypeMismatch { expected: String, got: String },

    // Transaction issues
    TransactionFailed(String),
    DeadlockDetected,

    // Generic
    Unknown(String),
}

impl PoolError {
    /// Extract the root cause for logging
    pub fn root_cause(&self) -> &str {
        match self {
            Self::ConnectionFailed(msg) => msg,
            Self::QueryFailed(msg) => msg,
            // ... etc
        }
    }
}
```

**Why**: Detailed errors help debugging. Match on them instead of string parsing.

### 8. Add Comprehensive Testing

**File**: `fraiseql_rs/src/db/tests.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Unit tests (no database needed)
    #[test]
    fn test_parameter_binding_prevents_injection() {
        let param = QueryParam::String("'; DROP TABLE users; --".to_string());
        // Should safely bind this, not execute
        assert!(bind_parameter(query, param).is_ok());
    }

    // Integration tests (require database)
    #[tokio::test]
    #[ignore]  // Run with: cargo test -- --ignored
    async fn test_execute_query_returns_rows() {
        let pool = create_test_pool().await;
        let result = pool.execute_query(
            "SELECT id, name FROM users WHERE id = $1",
            vec![QueryParam::Integer(1)],
        ).await;

        assert!(result.is_ok());
        let rows = result.unwrap().rows;
        assert!(!rows.is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn test_mutation_returns_affected_count() {
        let pool = create_test_pool().await;
        let result = pool.execute_mutation(
            "DELETE FROM users WHERE id = $1",
            vec![QueryParam::Integer(999)],
        ).await;

        assert!(result.is_ok());
        assert!(result.unwrap().rows_affected >= 0);
    }

    #[tokio::test]
    #[ignore]
    async fn test_transaction_isolation() {
        // Test that transactions don't interfere
        // Test rollback actually reverts changes
    }
}
```

**Why**: Tests verify safety and correctness. Ignored tests don't break CI.

---

## ❌ ANTIPATTERNS TO AVOID

### 1. **ANTIPATTERN: Python Directly Querying Database**

```rust
// ❌ DON'T DO THIS - breaks exclusive Rust pipeline
#[pyfunction]
fn execute_query(pool: PyRef<DatabasePool>, sql: String) -> PyResult<Vec<PyObject>> {
    // Directly use sqlx to query
    // This violates the Rust pipeline principle
}
```

**Why it's wrong**:
- Breaks the exclusive Rust pipeline
- Python types aren't guaranteed safe
- Parameter handling becomes inconsistent
- Makes caching impossible

**What to do instead**:
```rust
// ✅ DO THIS - keeps pipeline in Rust
#[pymethods]
impl DatabasePool {
    fn execute_query(&self, sql: &str, params: Vec<QueryParam>) -> PyResult<PyObject> {
        // Delegate to Rust pool
        // Pool handles everything
        // Return JSON result
    }
}
```

### 2. **ANTIPATTERN: String Interpolation for Parameters**

```rust
// ❌ DANGEROUS - SQL injection vulnerability
fn execute_query(sql: String, param: String) -> Result<QueryResult> {
    let full_sql = format!("SELECT * FROM users WHERE name = '{}'", param);
    // param could be: "'; DROP TABLE users; --"
    db.execute(&full_sql).await
}
```

**Why it's wrong**:
- Trivial to inject SQL
- Breaks with special characters (quotes, backslashes)
- Impossible to audit

**What to do instead**:
```rust
// ✅ SAFE - parameterized queries
fn execute_query(sql: &str, param: String) -> Result<QueryResult> {
    let mut query = sqlx::query(sql);  // Has $1, $2, etc
    query = query.bind(param);          // Binds safely
    query.fetch_all(pool).await         // Parameter separate from SQL
}
```

### 3. **ANTIPATTERN: Bypassing the Pool**

```rust
// ❌ WRONG - creates connections outside pool
#[pymethods]
impl DatabasePool {
    fn get_raw_connection(&self) -> PyResult<PgConnection> {
        // Someone might use this to execute queries directly
        // Breaks pooling, connection limits, caching
        PgConnection::connect(&self.url).await
    }
}
```

**Why it's wrong**:
- Defeats connection pooling benefits
- Can exhaust connections
- Circumvents all the safety mechanisms
- Makes debugging hard

**What to do instead**:
```rust
// ✅ CORRECT - all queries go through pool
#[pymethods]
impl DatabasePool {
    fn execute_query(
        &self,
        sql: &str,
        params: Vec<QueryParam>,
    ) -> PyResult<PyObject> {
        // This is the ONLY way to query
        // All safety happens here
    }
}
```

### 4. **ANTIPATTERN: Leaking Implementation Details**

```rust
// ❌ EXPOSES INTERNALS - API changes break users
pub struct QueryResult {
    pub inner: sqlx::QueryBuilder,  // sqlx is an implementation detail
    pub raw_sql: String,             // SQL shouldn't be user-visible
}
```

**Why it's wrong**:
- Users might rely on `sqlx` directly
- Upgrading sqlx becomes a breaking change
- Hides the abstraction layer

**What to do instead**:
```rust
// ✅ STABLE API - implementation can change
pub struct QueryResult {
    pub rows: Vec<Row>,              // Only expose what's needed
    pub columns: Vec<String>,
    pub execution_time_ms: u64,
    // sqlx, deadpool are private implementation details
}
```

### 5. **ANTIPATTERN: Mixing Sync and Async**

```rust
// ❌ WRONG - blocking the async runtime
async fn execute_query(pool: &PgPool, sql: &str) -> Result<Vec<Row>> {
    let result = std::thread::block_in_place(|| {
        // Blocking query execution
        blocking_sqlx_query(sql)
    });
    Ok(result)
}
```

**Why it's wrong**:
- Defeats async benefits
- Can deadlock with many queries
- Wastes thread pool
- Makes timeouts unreliable

**What to do instead**:
```rust
// ✅ CORRECT - fully async
async fn execute_query(pool: &PgPool, sql: &str) -> Result<Vec<Row>> {
    let rows = sqlx::query(sql)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}
```

### 6. **ANTIPATTERN: Silent Error Swallowing**

```rust
// ❌ HIDES PROBLEMS
pub async fn execute_query(sql: &str) -> QueryResult {
    match pool.execute(sql).await {
        Ok(rows) => rows,
        Err(_) => {
            // Silently return empty results
            QueryResult { rows: vec![] }
        }
    }
}
```

**Why it's wrong**:
- Debugging becomes impossible
- Silent failures are the worst kind
- Caching might hide the error
- Users don't know their query failed

**What to do instead**:
```rust
// ✅ CORRECT - propagate errors with context
pub async fn execute_query(sql: &str) -> Result<QueryResult, PoolError> {
    pool.execute(sql).await.map_err(|e| {
        PoolError::QueryFailed(format!(
            "Failed to execute query: {} [sql: {}]",
            e, sql
        ))
    })
}
```

### 7. **ANTIPATTERN: Type Confusion on Parameters**

```rust
// ❌ WRONG - loses type information
pub fn bind_param(query: Query, value: serde_json::Value) -> Query {
    // Is this a string? Number? Array? Can't tell
    match value.as_str() {
        Some(s) => query.bind(s),  // Assumes it's always a string
        None => query.bind(value), // Falls back to binding JSON
    }
}
```

**Why it's wrong**:
- Can't distinguish `null` from `"null"` string
- Numeric precision lost (JSON number vs i64)
- Arrays become ambiguous
- Type mismatches cause runtime errors

**What to do instead**:
```rust
// ✅ CORRECT - explicit enum
pub fn bind_param(query: Query, param: QueryParam) -> Result<Query> {
    match param {
        QueryParam::Null => Ok(query.bind::<Option<String>>(None)),
        QueryParam::String(s) => Ok(query.bind(s)),
        QueryParam::Integer(i) => Ok(query.bind(i)),
        QueryParam::Float(f) => Ok(query.bind(f)),
        _ => Err(PoolError::InvalidParameter { /* ... */ }),
    }
}
```

### 8. **ANTIPATTERN: Unbounded Caching**

```rust
// ❌ WRONG - cache grows forever
pub async fn execute_query(sql: &str) -> Result<QueryResult> {
    static CACHE: Lazy<Mutex<HashMap<String, QueryResult>>> =
        Lazy::new(|| Mutex::new(HashMap::new()));

    let mut cache = CACHE.lock();
    if let Some(cached) = cache.get(sql) {
        return Ok(cached.clone());
    }

    let result = pool.execute(sql).await?;
    cache.insert(sql.to_string(), result.clone());  // Never evicted!
    Ok(result)
}
```

**Why it's wrong**:
- Memory leak
- Stale data forever
- No TTL means data consistency issues
- Unbounded growth destroys performance

**What to do instead**:
```rust
// ✅ CORRECT - bounded cache with TTL
pub async fn execute_query(sql: &str) -> Result<QueryResult> {
    // Use Arc<DashMap> with TTL
    // Cache entry expires after 1 hour
    // Max 10,000 entries (evict oldest if exceeded)

    let cache_key = format!("query:{}", sql);
    if let Ok(cached) = self.cache.get(&cache_key).await {
        return Ok(cached);
    }

    let result = self.pool.execute_query(sql, params).await?;

    // Set with 1-hour TTL
    self.cache.set(&cache_key, result.clone(), 3600).await.ok();

    Ok(result)
}
```

### 9. **ANTIPATTERN: Ignoring Connection Exhaustion**

```rust
// ❌ WRONG - no backpressure
pub async fn execute_many_queries(queries: Vec<String>) {
    let mut handles = vec![];

    for query in queries {
        // Spawn a task for each query without limit
        let handle = tokio::spawn(async move {
            pool.execute(&query).await
        });
        handles.push(handle);
    }

    // If queries > pool size, connections are exhausted
    // Later queries timeout or fail
    futures::future::join_all(handles).await;
}
```

**Why it's wrong**:
- Exceeds pool size = timeout or failure
- No graceful degradation
- Unpredictable behavior under load

**What to do instead**:
```rust
// ✅ CORRECT - bounded concurrency
use tokio::sync::Semaphore;

pub async fn execute_many_queries(queries: Vec<String>) {
    let semaphore = Arc::new(Semaphore::new(pool.max_size));
    let mut handles = vec![];

    for query in queries {
        let permit = semaphore.acquire().await.unwrap();

        let handle = tokio::spawn(async move {
            let _guard = permit;  // Hold until query finishes
            pool.execute(&query).await
        });
        handles.push(handle);
    }

    futures::future::join_all(handles).await;
}
```

### 10. **ANTIPATTERN: No Metrics or Observability**

```rust
// ❌ WRONG - can't debug performance
pub async fn execute_query(sql: &str) -> Result<QueryResult> {
    pool.execute(sql).await
}
```

**Why it's wrong**:
- Can't find slow queries
- Can't detect resource leaks
- Can't alert on errors
- Impossible to optimize

**What to do instead**:
```rust
// ✅ CORRECT - instrumented
pub async fn execute_query(&self, sql: &str) -> Result<QueryResult> {
    let start = std::time::Instant::now();

    let result = self.pool.execute(sql).await;

    let elapsed = start.elapsed();

    // Record metrics
    self.metrics.record_query(sql, elapsed, result.is_ok());

    // Log slow queries
    if elapsed.as_millis() > 100 {
        warn!("Slow query detected: {} ({}ms)", sql, elapsed.as_millis());
    }

    result
}
```

---

## Integration Points: Python ↔ Rust

### Current Integration (Phase 3.1)

```python
# Python code
from fraiseql.db import DatabasePool

pool = DatabasePool(
    database="fraiseql",
    username="postgres",
    password="secret"
)
```

### Phase 3.2 Integration (Query Execution)

```python
# Python code should look like this:

from fraiseql.db import DatabasePool, QueryParam

pool = DatabasePool(url="postgresql://...")

# Execute a SELECT query
result = await pool.execute_query(
    sql="SELECT * FROM users WHERE id = $1",
    params=[QueryParam.integer(123)]
)

# Result is JSON (safe type)
rows = result.rows  # List of dicts
columns = result.columns  # List of strings
execution_time = result.execution_time_ms  # int
```

**Key Points**:
- Python passes SQL to Rust (already validated by Planner)
- Parameters are `QueryParam` enums (type-safe)
- Results are JSON (no type confusion)
- Python never directly executes queries

### Error Handling Pattern

```python
# Python layer
try:
    result = await pool.execute_query(sql, params)
except PoolError as e:
    # PoolError is Rust exception mapped to Python
    if e.kind == "ConnectionTimeout":
        # Handle timeout
        retry()
    elif e.kind == "QueryFailed":
        # Handle query failure
        log_error(e)
    else:
        raise
```

---

## Dependency and Version Alignment

### Current Dependencies (Phase 3.1)

From `Cargo.toml`:
- `deadpool-postgres` - Connection pooling ✅
- `tokio` - Async runtime ✅
- `serde_json` - JSON handling ✅
- `sqlx` - (Phase 3.2 will add)

### Phase 3.2 Dependencies to Add

```toml
[dependencies]
sqlx = { version = "0.8", features = [
    "postgres",        # PostgreSQL driver
    "runtime-tokio",   # tokio integration
    "json",           # JSON type support
    "uuid",           # UUID type support
    "chrono",         # DateTime support
] }

# For metrics (optional but recommended)
prometheus = "0.13"

# Already present
tokio = { version = "1", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

---

## Performance Considerations

### Parameter Binding Performance

```rust
// ✅ Optimal - compile-time SQL preparation
let query = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1");
let user = query.fetch_one(pool, id).await?;

// ⚠️ Acceptable - runtime SQL construction
let query = sqlx::query(sql);
let query = query.bind(id);
let rows = query.fetch_all(pool).await?;

// ❌ Poor - multiple roundtrips
for param in params {
    query = query.bind(param);  // Each bind might be a separate operation
}
```

### Result Transformation Performance

```rust
// ✅ Optimal - streaming results
let rows = sqlx::query("SELECT * FROM users")
    .fetch(pool)
    .try_fold(Vec::new(), |mut acc, row| async move {
        acc.push(row_to_json(row));
        Ok(acc)
    })
    .await?;

// ⚠️ Acceptable - collect all rows
let rows = sqlx::query("SELECT * FROM users")
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(row_to_json)
    .collect();
```

---

## Testing Strategy for Phase 3.2

### Unit Tests (No Database Required)

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_sql_injection_prevention() {
        // Parameter binding prevents injection by construction
        let param = QueryParam::String("'; DROP TABLE users; --");
        // Should NOT execute the injection
    }

    #[test]
    fn test_null_handling() {
        let param = QueryParam::Null;
        // Should bind as SQL NULL, not string "null"
    }

    #[test]
    fn test_json_roundtrip() {
        let json = serde_json::json!({"key": "value"});
        let param = QueryParam::Json(json.clone());
        // After binding and retrieval, should be identical
    }
}
```

### Integration Tests (Database Required, Ignored by Default)

```rust
#[cfg(test)]
mod integration_tests {
    #[tokio::test]
    #[ignore]  // Run with: cargo test -- --ignored
    async fn test_execute_select_query() {
        // Setup: Create test database
        // Execute: SELECT * FROM users
        // Verify: Rows returned correctly
    }

    #[tokio::test]
    #[ignore]
    async fn test_parameter_binding_accuracy() {
        // Insert with parameter binding
        // Verify: Value stored correctly
    }

    #[tokio::test]
    #[ignore]
    async fn test_transaction_isolation() {
        // Test ACID properties
    }
}
```

### Running Tests

```bash
# Run unit tests (fast, no database)
cargo test --lib

# Run all tests including integration (requires database)
cargo test -- --ignored

# Run specific test
cargo test test_execute_select_query -- --ignored
```

---

## Documentation for Phase 3.2

### For Rust Developers

**What should be documented**:
1. Parameter binding API (what types are supported)
2. Result transformation (how rows become JSON)
3. Error types and recovery strategies
4. Transaction semantics (isolation levels, rollback)
5. Performance characteristics (query execution time)
6. Configuration (pool size, timeouts, SSL)

**Where to document**:
- `fraiseql_rs/src/db/mod.rs` - Module overview
- `fraiseql_rs/src/db/query_builder.rs` - Parameter binding API
- `fraiseql_rs/src/db/errors.rs` - Error reference
- `docs/ARCHITECTURE.md` - System design

### For Python Users

**What should be documented**:
1. How to execute queries (with examples)
2. Parameter safety (what's protected against injection)
3. Result structure (how to access rows/columns)
4. Error handling patterns
5. Transaction usage
6. Connection pool configuration

**Where to document**:
- `docs/guides/database-queries.md` - Query execution guide
- `src/fraiseql/db/pool.py` - Python API docstrings
- `examples/query-execution.py` - Working examples

---

## Summary: What Phase 3.2 Must Deliver

### ✅ Correct Implementation Pattern

1. **Extend `PoolBackend` trait** with query execution methods
2. **Define type-safe parameter binding** using `QueryParam` enum
3. **Implement query execution in `ProductionPool`** using sqlx
4. **Handle transactions** with proper BEGIN/COMMIT/ROLLBACK
5. **Create comprehensive error types** with context
6. **Add metrics and observability** (execution time, error rates)
7. **Write integration tests** (marked as `#[ignore]`)
8. **Document thoroughly** (API and usage guides)

### ❌ Antipatterns to Avoid

1. Don't let Python query database directly
2. Don't use string interpolation for parameters
3. Don't bypass the pool
4. Don't leak implementation details
5. Don't mix sync and async
6. Don't swallow errors silently
7. Don't confuse types in parameters
8. Don't cache without TTL/limits
9. Don't ignore connection exhaustion
10. Don't skip metrics and observability

### 🎯 Integration Points

- Python calls Rust query methods (already exists via PyO3)
- Parameters are type-safe enums
- Results are JSON (no type confusion)
- Errors map from Rust exceptions to Python
- No direct database access from Python

---

## Next Steps

1. **Review Phase 3.1 completion** - Verify pool works
2. **Design parameter binding API** - What types are supported?
3. **Implement query execution in Rust** - Use sqlx
4. **Add integration tests** - Verify with real database
5. **Document thoroughly** - API and patterns
6. **Verify no regressions** - All Phase 3.1 tests still pass
7. **Performance benchmark** - Measure query execution time

---

## References

- Phase 3.1 Implementation: `PHASE_3_FOUNDATION_COMPLETE.md`
- Pool Implementation: `fraiseql_rs/src/db/pool.rs`
- FraiseQL CLAUDE.md: Type annotation and architecture guidelines
- sqlx Documentation: Parameter binding and type safety
- PostgreSQL Error Codes: For proper error handling

---

**Status**: Ready for Phase 3.2 Architecture Review ✅
