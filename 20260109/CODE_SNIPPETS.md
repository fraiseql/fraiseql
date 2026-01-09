# Code Snippets - Task 4: ProductionPool Query Execution

## Template for query() Implementation

**Location**: `fraiseql_rs/src/db/pool_production.rs`

### Basic Structure

```rust
#[async_trait]
impl PoolBackend for ProductionPool {
    async fn query(&self, sql: &str) -> PoolResult<Vec<serde_json::Value>> {
        let start = Instant::now();

        // 1. Get connection from pool
        let conn = self.pool
            .get()
            .await
            .map_err(|e| PoolError::ConnectionAcquisition(e.to_string()))?;

        // 2. Execute query
        let rows = conn
            .query(sql, &[])
            .await
            .map_err(|e| PoolError::QueryExecution(format!("Query failed: {}", e)))?;

        // 3. Extract JSONB from column 0
        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            let jsonb_value: serde_json::Value = row
                .try_get(0)
                .map_err(|e| PoolError::QueryExecution(
                    format!("Failed to extract JSONB from column 0: {}", e)
                ))?;
            results.push(jsonb_value);
        }

        // 4. Update metrics
        let elapsed = start.elapsed();
        self.metrics.record_query_time(elapsed);

        Ok(results)
    }
}
```

## With Parameter Binding (Future)

```rust
// This will come later, but shows the pattern:
pub async fn query_with_params(
    &self,
    sql: &str,
    params: Vec<QueryParam>,
) -> PoolResult<Vec<serde_json::Value>> {
    // 1. Validate parameters FIRST
    validate_parameter_count(sql, &params)?;
    prepare_parameters(&params)?;

    // 2. Get connection
    let conn = self.pool.get().await?;

    // 3. Build parameter array for PostgreSQL
    let pg_params: Vec<&(dyn ToSql + Sync)> = params
        .iter()
        .map(|p| p as &(dyn ToSql + Sync))
        .collect();

    // 4. Execute with parameters
    let rows = conn.query(sql, &pg_params).await?;

    // 5. Extract JSONB from column 0 (same as above)
    // ...
}
```

## Test Template

**Location**: `tests/integration/rust/` (create new file)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use fraiseql_rs::db::{DatabaseConfig, ProductionPool, PoolBackend};

    #[tokio::test]
    async fn test_query_returns_jsonb() {
        // Setup: Create test pool with real PostgreSQL
        let config = DatabaseConfig::from_url(
            "postgresql://postgres:postgres@localhost/fraiseql_test"
        ).expect("Invalid URL");

        let pool = ProductionPool::new(config)
            .expect("Failed to create pool");

        // Execute: Query a test view with JSONB data
        let results = pool
            .query("SELECT data FROM tv_user LIMIT 1")
            .await
            .expect("Query failed");

        // Assert: Results are Vec<serde_json::Value>
        assert!(!results.is_empty());
        assert!(results[0].is_object());
    }

    #[tokio::test]
    async fn test_query_empty_result() {
        // Setup
        let config = DatabaseConfig::from_url(
            "postgresql://postgres:postgres@localhost/fraiseql_test"
        ).expect("Invalid URL");

        let pool = ProductionPool::new(config)
            .expect("Failed to create pool");

        // Execute: Query with no results
        let results = pool
            .query("SELECT data FROM tv_user WHERE id = -999")
            .await
            .expect("Query failed");

        // Assert: Empty vec
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_query_invalid_sql() {
        // Setup
        let config = DatabaseConfig::from_url(
            "postgresql://postgres:postgres@localhost/fraiseql_test"
        ).expect("Invalid URL");

        let pool = ProductionPool::new(config)
            .expect("Failed to create pool");

        // Execute: Invalid SQL
        let result = pool
            .query("INVALID SQL HERE")
            .await;

        // Assert: Returns error
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_query_column_0_extraction() {
        // Setup
        let config = DatabaseConfig::from_url(
            "postgresql://postgres:postgres@localhost/fraiseql_test"
        ).expect("Invalid URL");

        let pool = ProductionPool::new(config)
            .expect("Failed to create pool");

        // Execute: Query that returns specific JSONB structure
        let results = pool
            .query("SELECT '{\"id\": 1, \"name\": \"test\"}'::jsonb as data")
            .await
            .expect("Query failed");

        // Assert: Correct structure extracted
        assert_eq!(results.len(), 1);
        let obj = results[0].as_object().expect("Not an object");
        assert_eq!(obj.get("id"), Some(&json!(1)));
        assert_eq!(obj.get("name"), Some(&json!("test")));
    }
}
```

## Error Handling Pattern

```rust
// Pattern 1: Map PostgreSQL errors to PoolError
.map_err(|e| PoolError::QueryExecution(format!("Query failed: {}", e)))?

// Pattern 2: Connection acquisition errors
.map_err(|e| PoolError::ConnectionAcquisition(e.to_string()))?

// Pattern 3: Type conversion errors
.try_get(0)
.map_err(|e| PoolError::QueryExecution(format!("Column 0 not JSONB: {}", e)))?

// Pattern 4: Configuration errors
if self.pool.state().connections == 0 {
    return Err(PoolError::Configuration("No connections in pool".to_string()));
}
```

## Integration with Existing Code

### Import Statements

```rust
// Add to pool_production.rs top
use crate::db::pool::traits::{PoolBackend, PoolResult, PoolError};
use async_trait::async_trait;
use std::time::Instant;
```

### Use Parameter Binding Module

```rust
// Add when implementing parameters (later)
use crate::db::parameter_binding::{
    prepare_parameters,
    validate_parameter_count,
    count_placeholders,
};
```

### Metrics Recording

```rust
// Update metrics after query
self.metrics.record_query_time(elapsed);
self.metrics.increment_query_count();

// Or get current metrics
let stats = self.metrics.snapshot();
println!("Queries executed: {}", stats.query_count);
```

## Deadpool-Postgres API Reference

```rust
// Getting a connection
let conn = self.pool.get().await?;

// Executing a query (no parameters)
let rows = conn.query(sql, &[]).await?;

// Iterating over rows
for row in rows {
    let value: serde_json::Value = row.get(0);
}

// Getting a value from row
let value: serde_json::Value = row.try_get(0)?;

// Getting pool state
let state = self.pool.state();
println!("Connections: {}", state.connections);
println!("Available: {}", state.available_size);
```

## Testing Against Real PostgreSQL

### Test Setup

```bash
# Ensure PostgreSQL is running locally
psql -U postgres -h localhost -c "CREATE DATABASE fraiseql_test;"

# Create test view
psql -U postgres -h localhost -d fraiseql_test << EOF
CREATE TABLE test_user (
    id BIGINT PRIMARY KEY,
    data JSONB NOT NULL
);

CREATE VIEW tv_user AS
SELECT data FROM test_user;

INSERT INTO test_user VALUES
    (1, '{"id": 1, "name": "Alice"}'),
    (2, '{"id": 2, "name": "Bob"}');
EOF

# Run tests
cargo test --lib pool_production
```

## Common Debugging Patterns

```rust
// Debug: Print SQL before execution
eprintln!("Executing SQL: {}", sql);

// Debug: Print row count
eprintln!("Got {} rows from query", rows.len());

// Debug: Print extracted JSONB
eprintln!("Extracted JSONB: {}", serde_json::to_string_pretty(&results).unwrap());

// Debug: Check pool state
let state = self.pool.state();
eprintln!("Pool state - connections: {}, available: {}",
    state.connections, state.available_size);

// Debug: Time tracking
let start = Instant::now();
// ... do work ...
let elapsed = start.elapsed();
eprintln!("Query took {:?}", elapsed);
```

## Compile-Time Checks

```bash
# Check for type errors
cargo check --lib

# Full compilation
cargo build --lib

# With strict warnings
cargo build --lib --all-targets

# Run clippy
cargo clippy --lib -- -D warnings
```

---

**Note**: These snippets are templates. Adjust based on actual deadpool-postgres API and your implementation approach.
