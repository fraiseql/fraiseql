# Phase 2: Query Execution - WHERE Clauses & SQL Generation

**Phase**: 2 of 5
**Effort**: 12 hours
**Status**: Blocked until Phase 1 complete
**Prerequisite**: Phase 1 - Foundation complete

---

## Objective

Implement query execution in Rust:
1. Async connection acquisition from pool
2. WHERE clause building (migrate from Python)
3. SQL generation (migrate from Python)
4. Raw query execution with parameter binding
5. Result streaming from database

**Success Criteria**:
- ✅ WHERE clauses build correctly (parity with psycopg)
- ✅ SQL queries execute and return results
- ✅ Query parameters properly bound
- ✅ All 5991+ tests pass with Rust backend
- ✅ Performance: 20-30% faster than psycopg

---

## Architecture Overview

### Data Flow

```
Python Query Definition
  ↓
  GraphQL query parsed → Pydantic validation
  ↓
Python: extract_query_info()
  ├→ table name
  ├→ field selections
  ├→ WHERE filters
  ├→ ORDER BY
  └→ LIMIT/OFFSET
  ↓ (single FFI call)
Rust: execute_query()
  ├→ build_where_clause()
  ├→ build_select_sql()
  ├→ bind_parameters()
  ├→ acquire_connection()
  ├→ conn.query()
  └→ collect_results()
  ↓
Results (JSON rows)
  ↓
Rust JSON Transform Pipeline
  ↓
HTTP Response
```

### WHERE Clause Architecture

**Current Python Implementation** (`sql/graphql_where_generator.py`):
- Recursive WHERE clause building
- Type-aware filtering
- Operator support: `=`, `!=`, `>`, `<`, `>=`, `<=`, `IN`, `LIKE`, etc.

**New Rust Implementation**:
- Direct port of Python logic
- Same performance characteristics
- Type-safe handling

---

## Implementation Steps

### Step 1: Add Dependencies

**File**: `fraiseql_rs/Cargo.toml`

Add if not already present:
```toml
[dependencies]
thiserror = "1.0"      # Better error handling
futures = "0.3"        # For boxed futures
tokio = { version = "1.0", features = ["time"] }
```

### Step 2: Create Transaction Support Module

**File**: `fraiseql_rs/src/db/transaction.rs` (NEW)

```rust
//! Transaction management for mutations.

use tokio_postgres::Client;
use super::types::DatabaseError;

/// Represents an active transaction.
pub struct Transaction<'a> {
    client: &'a mut Client,
    active: bool,
}

impl<'a> Transaction<'a> {
    /// Begin a new transaction.
    pub async fn begin(client: &'a mut Client) -> Result<Self, DatabaseError> {
        client.execute("BEGIN", &[])
            .await
            .map_err(|e| DatabaseError::QueryError(format!("Failed to begin transaction: {}", e)))?;

        Ok(Transaction {
            client,
            active: true,
        })
    }

    /// Commit the transaction.
    pub async fn commit(mut self) -> Result<(), DatabaseError> {
        if self.active {
            self.client.execute("COMMIT", &[])
                .await
                .map_err(|e| DatabaseError::QueryError(format!("Failed to commit: {}", e)))?;
            self.active = false;
        }
        Ok(())
    }

    /// Rollback the transaction.
    pub async fn rollback(mut self) -> Result<(), DatabaseError> {
        if self.active {
            self.client.execute("ROLLBACK", &[])
                .await
                .map_err(|e| DatabaseError::QueryError(format!("Failed to rollback: {}", e)))?;
            self.active = false;
        }
        Ok(())
    }

    /// Create a savepoint for nested transactions.
    pub async fn savepoint(&mut self, name: &str) -> Result<(), DatabaseError> {
        self.client.execute(&format!("SAVEPOINT {}", name), &[])
            .await
            .map_err(|e| DatabaseError::QueryError(format!("Savepoint failed: {}", e)))?;
        Ok(())
    }

    /// Rollback to a savepoint.
    pub async fn rollback_to_savepoint(&mut self, name: &str) -> Result<(), DatabaseError> {
        self.client.execute(&format!("ROLLBACK TO {}", name), &[])
            .await
            .map_err(|e| DatabaseError::QueryError(format!("Rollback to savepoint failed: {}", e)))?;
        Ok(())
    }
}

impl<'a> Drop for Transaction<'a> {
    fn drop(&mut self) {
        // Auto-rollback if not committed
        if self.active {
            // Can't await in drop, so we just log a warning
            eprintln!("Warning: Transaction dropped without explicit commit/rollback");
        }
    }
}
```

### Step 3: Implement Async Pool Functions (COMPLETE)

**File**: `fraiseql_rs/src/db/pool.rs`

Update the acquire_connection implementation to handle connection wrapping:

```rust
/// Connection wrapper that can be passed to Python and used for queries.
#[pyclass]
pub struct Connection {
    conn: Arc<Mutex<tokio_postgres::Client>>,
}

#[pymethods]
impl Connection {
    /// Execute raw SQL (used by query executor).
    async fn execute_raw(
        &self,
        sql: String,
        params: Vec<String>,
    ) -> PyResult<String> {
        // Stub - Phase 2 will implement query execution
        Ok("{}".to_string())
    }
}

#[pymethods]
impl DatabasePool {
    /// Acquire a connection from the pool (ASYNC - returns Python coroutine).
    #[pyo3_asyncio::tokio::main]
    async fn acquire_connection(&self, py: Python) -> PyResult<Py<PyAny>> {
        let pool = self.pool.clone();
        let timeout_ms = self.config.connection_timeout;

        pyo3_asyncio::tokio::future_into_py(py, async move {
            match tokio::time::timeout(
                std::time::Duration::from_millis(timeout_ms),
                pool.get(),
            ).await
            {
                Ok(Ok(client)) => {
                    // Wrap in Connection object for Python
                    let conn = Connection {
                        conn: Arc::new(Mutex::new(client)),
                    };
                    Ok(conn)
                }
                Ok(Err(e)) => {
                    Err(PyErr::new::<pyo3::exceptions::RuntimeError, _>(
                        format!("Failed to acquire connection: {}", e)
                    ))
                }
                Err(_) => {
                    Err(PyErr::new::<pyo3::exceptions::TimeoutError, _>(
                        format!("Connection timeout after {}ms", timeout_ms)
                    ))
                }
            }
        })
    }
}
```

**Verification**:
```bash
cargo test -p fraiseql_rs --lib db::pool::tests
```

### Step 2: Implement WHERE Clause Builder

**File**: `fraiseql_rs/src/db/where_builder.rs`

This is a direct port of `src/fraiseql/sql/graphql_where_generator.py`:

```rust
//! WHERE clause builder for GraphQL queries.
//!
//! Converts GraphQL filter inputs to SQL WHERE clauses with parameter binding.

use super::types::QueryParam;
use serde_json::{json, Value};
use std::collections::HashMap;

/// Build WHERE clause from GraphQL filter dictionary.
///
/// # Example
/// ```rust
/// let filters = HashMap::from([
///     ("user_id".to_string(), json!({"eq": 123})),
///     ("status".to_string(), json!({"in": ["active", "pending"]})),
/// ]);
///
/// let (clause, params) = build_where_clause("users", &filters)?;
/// assert!(clause.contains("user_id = $1"));
/// ```
pub fn build_where_clause(
    table: &str,
    filters: &HashMap<String, Value>,
) -> Result<(String, Vec<QueryParam>), String> {
    let mut conditions = Vec::new();
    let mut params = Vec::new();
    let mut param_counter = 1;

    for (field_name, filter_value) in filters {
        let (condition, new_params) =
            build_field_condition(field_name, filter_value, &mut param_counter)?;
        conditions.push(condition);
        params.extend(new_params);
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    Ok((where_clause, params))
}

/// Build condition for a single field.
fn build_field_condition(
    field_name: &str,
    filter_value: &Value,
    param_counter: &mut usize,
) -> Result<(String, Vec<QueryParam>), String> {
    match filter_value {
        // Simple equality: {"eq": value}
        Value::Object(obj) if obj.contains_key("eq") => {
            let param = parse_param(obj.get("eq").unwrap())?;
            let condition = format!("{} = ${}", field_name, param_counter);
            *param_counter += 1;
            Ok((condition, vec![param]))
        }

        // Not equal: {"ne": value}
        Value::Object(obj) if obj.contains_key("ne") => {
            let param = parse_param(obj.get("ne").unwrap())?;
            let condition = format!("{} != ${}", field_name, param_counter);
            *param_counter += 1;
            Ok((condition, vec![param]))
        }

        // Greater than: {"gt": value}
        Value::Object(obj) if obj.contains_key("gt") => {
            let param = parse_param(obj.get("gt").unwrap())?;
            let condition = format!("{} > ${}", field_name, param_counter);
            *param_counter += 1;
            Ok((condition, vec![param]))
        }

        // Greater or equal: {"gte": value}
        Value::Object(obj) if obj.contains_key("gte") => {
            let param = parse_param(obj.get("gte").unwrap())?;
            let condition = format!("{} >= ${}", field_name, param_counter);
            *param_counter += 1;
            Ok((condition, vec![param]))
        }

        // Less than: {"lt": value}
        Value::Object(obj) if obj.contains_key("lt") => {
            let param = parse_param(obj.get("lt").unwrap())?;
            let condition = format!("{} < ${}", field_name, param_counter);
            *param_counter += 1;
            Ok((condition, vec![param]))
        }

        // Less or equal: {"lte": value}
        Value::Object(obj) if obj.contains_key("lte") => {
            let param = parse_param(obj.get("lte").unwrap())?;
            let condition = format!("{} <= ${}", field_name, param_counter);
            *param_counter += 1;
            Ok((condition, vec![param]))
        }

        // IN: {"in": [values]}
        Value::Object(obj) if obj.contains_key("in") => {
            let in_list = obj
                .get("in")
                .ok_or("Missing 'in' value")?
                .as_array()
                .ok_or("'in' must be an array")?;

            let mut placeholders = Vec::new();
            let mut params = Vec::new();

            for value in in_list {
                let param = parse_param(value)?;
                placeholders.push(format!("${}", param_counter));
                params.push(param);
                *param_counter += 1;
            }

            let condition = format!("{} IN ({})", field_name, placeholders.join(", "));
            Ok((condition, params))
        }

        // LIKE: {"like": "%pattern%"}
        Value::Object(obj) if obj.contains_key("like") => {
            let param = parse_param(obj.get("like").unwrap())?;
            let condition = format!("{} LIKE ${}", field_name, param_counter);
            *param_counter += 1;
            Ok((condition, vec![param]))
        }

        // IS NULL: {"isNull": true}
        Value::Object(obj) if obj.contains_key("isNull") => {
            let is_null = obj
                .get("isNull")
                .ok_or("Missing 'isNull' value")?
                .as_bool()
                .ok_or("'isNull' must be boolean")?;

            let condition = if is_null {
                format!("{} IS NULL", field_name)
            } else {
                format!("{} IS NOT NULL", field_name)
            };
            Ok((condition, vec![]))
        }

        // Nested AND logic: {"and": [{"eq": value1}, {"gt": value2}]}
        Value::Object(obj) if obj.contains_key("and") => {
            let and_conditions = obj
                .get("and")
                .ok_or("Missing 'and' value")?
                .as_array()
                .ok_or("'and' must be an array")?;

            let mut nested_conditions = Vec::new();
            for condition in and_conditions {
                let (cond_str, cond_params) =
                    build_field_condition(field_name, condition, param_counter)?;
                nested_conditions.push(cond_str);
                params.extend(cond_params);
            }

            if nested_conditions.is_empty() {
                Err("'and' array is empty".to_string())
            } else {
                let condition = format!("({})", nested_conditions.join(" AND "));
                Ok((condition, params))
            }
        }

        // Nested OR logic: {"or": [{"eq": value1}, {"gt": value2}]}
        Value::Object(obj) if obj.contains_key("or") => {
            let or_conditions = obj
                .get("or")
                .ok_or("Missing 'or' value")?
                .as_array()
                .ok_or("'or' must be an array")?;

            let mut nested_conditions = Vec::new();
            for condition in or_conditions {
                let (cond_str, cond_params) =
                    build_field_condition(field_name, condition, param_counter)?;
                nested_conditions.push(cond_str);
                params.extend(cond_params);
            }

            if nested_conditions.is_empty() {
                Err("'or' array is empty".to_string())
            } else {
                let condition = format!("({})", nested_conditions.join(" OR "));
                Ok((condition, params))
            }
        }

        // NOT logic: {"not": {"eq": value}}
        Value::Object(obj) if obj.contains_key("not") => {
            let not_filter = obj
                .get("not")
                .ok_or("Missing 'not' value")?;

            let (inner_condition, inner_params) =
                build_field_condition(field_name, not_filter, param_counter)?;

            // For NOT, we need to negate the condition
            let negated = if inner_condition.contains("IS NULL") {
                inner_condition.replace("IS NULL", "IS NOT NULL")
            } else if inner_condition.contains("IS NOT NULL") {
                inner_condition.replace("IS NOT NULL", "IS NULL")
            } else if inner_condition.contains("IN (") {
                inner_condition.replace("IN", "NOT IN")
            } else {
                format!("NOT ({})", inner_condition)
            };

            Ok((negated, inner_params))
        }

        _ => Err(format!("Unsupported filter format for field '{}'", field_name)),
    }
}

/// Parse JSON value to QueryParam.
fn parse_param(value: &Value) -> Result<QueryParam, String> {
    match value {
        Value::String(s) => Ok(QueryParam::String(s.clone())),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(QueryParam::Int(i))
            } else if let Some(f) = n.as_f64() {
                Ok(QueryParam::Float(f))
            } else {
                Err("Invalid number format".to_string())
            }
        }
        Value::Bool(b) => Ok(QueryParam::Bool(*b)),
        Value::Null => Ok(QueryParam::Null),
        Value::Object(_) | Value::Array(_) => {
            Ok(QueryParam::Json(value.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_equality() {
        let mut filters = HashMap::new();
        filters.insert("id".to_string(), json!({"eq": 123}));

        let (clause, params) = build_where_clause("users", &filters).unwrap();
        assert!(clause.contains("id = $1"));
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_in_operator() {
        let mut filters = HashMap::new();
        filters.insert("status".to_string(), json!({"in": ["active", "pending"]}));

        let (clause, params) = build_where_clause("users", &filters).unwrap();
        assert!(clause.contains("status IN"));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_multiple_conditions() {
        let mut filters = HashMap::new();
        filters.insert("id".to_string(), json!({"eq": 123}));
        filters.insert("status".to_string(), json!({"eq": "active"}));

        let (clause, params) = build_where_clause("users", &filters).unwrap();
        assert!(clause.contains("AND"));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_is_null() {
        let mut filters = HashMap::new();
        filters.insert("deleted_at".to_string(), json!({"isNull": true}));

        let (clause, params) = build_where_clause("users", &filters).unwrap();
        assert!(clause.contains("IS NULL"));
        assert_eq!(params.len(), 0);
    }
}
```

**Verification**:
```bash
cargo test -p fraiseql_rs --lib db::where_builder::tests
```

### Step 3: Implement SQL Generator

**File**: `fraiseql_rs/src/sql/mod.rs` (NEW)

```rust
//! SQL generation for GraphQL queries.

pub mod generator;
pub mod select_builder;
pub mod where_clause;

pub use generator::build_select_query;
pub use select_builder::SelectBuilder;
pub use where_clause::build_where_clause;

/// Complete SQL query with parameters.
#[derive(Debug, Clone)]
pub struct SelectQuery {
    pub sql: String,
    pub params: Vec<crate::db::types::QueryParam>,
}
```

**File**: `fraiseql_rs/src/sql/generator.rs`

```rust
//! Main SQL query generator.

use crate::db::types::QueryParam;
use serde_json::Value;
use std::collections::HashMap;

use super::select_builder::SelectBuilder;
use super::where_clause::build_where_clause;

/// Build complete SELECT query from GraphQL query definition.
///
/// # Example
/// ```rust
/// let query_def = QueryDefinition {
///     table: "users".to_string(),
///     columns: vec!["id", "name", "email"],
///     where_filters: HashMap::from([("status", json!({"eq": "active"}))]),
///     limit: 100,
///     offset: 0,
/// };
///
/// let (sql, params) = build_select_query(&query_def)?;
/// ```
pub fn build_select_query(
    table: &str,
    columns: &[&str],
    where_filters: &HashMap<String, Value>,
    limit: Option<i32>,
    offset: Option<i32>,
) -> Result<(String, Vec<QueryParam>), String> {
    let mut builder = SelectBuilder::new(table);

    // Add columns
    for column in columns {
        builder.select(*column);
    }

    // Add WHERE clause
    let (where_clause, mut params) = build_where_clause(table, where_filters)?;
    if !where_clause.is_empty() {
        builder.where_raw(&where_clause);
    }

    // Add LIMIT and OFFSET
    if let Some(limit_val) = limit {
        builder.limit(limit_val);
    }

    if let Some(offset_val) = offset {
        builder.offset(offset_val);
    }

    let sql = builder.build();
    Ok((sql, params))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_select() {
        let (sql, _params) =
            build_select_query("users", &["id", "name"], &HashMap::new(), None, None)
                .unwrap();
        assert!(sql.contains("SELECT"));
        assert!(sql.contains("id"));
    }
}
```

**File**: `fraiseql_rs/src/sql/select_builder.rs`

```rust
//! SELECT query builder.

pub struct SelectBuilder {
    table: String,
    columns: Vec<String>,
    where_clause: Option<String>,
    limit: Option<i32>,
    offset: Option<i32>,
}

impl SelectBuilder {
    pub fn new(table: &str) -> Self {
        SelectBuilder {
            table: table.to_string(),
            columns: Vec::new(),
            where_clause: None,
            limit: None,
            offset: None,
        }
    }

    pub fn select(&mut self, column: &str) {
        self.columns.push(column.to_string());
    }

    pub fn where_raw(&mut self, clause: &str) {
        self.where_clause = Some(clause.to_string());
    }

    pub fn limit(&mut self, limit: i32) {
        self.limit = Some(limit);
    }

    pub fn offset(&mut self, offset: i32) {
        self.offset = Some(offset);
    }

    pub fn build(&self) -> String {
        let mut query = format!("SELECT {} FROM {}", self.columns.join(", "), self.table);

        if let Some(where_clause) = &self.where_clause {
            query.push(' ');
            query.push_str(where_clause);
        }

        if let Some(limit) = self.limit {
            query.push_str(&format!(" LIMIT {}", limit));
        }

        if let Some(offset) = self.offset {
            query.push_str(&format!(" OFFSET {}", offset));
        }

        query
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_select() {
        let mut builder = SelectBuilder::new("users");
        builder.select("id");
        builder.select("name");
        builder.limit(10);

        let sql = builder.build();
        assert_eq!(sql, "SELECT id, name FROM users LIMIT 10");
    }
}
```

**Verification**:
```bash
cargo test -p fraiseql_rs --lib sql
```

### Step 4: Implement Query Executor

**File**: `fraiseql_rs/src/db/query.rs`

Replace stub with real implementation:

```rust
//! Query execution against PostgreSQL.

use super::types::{DatabaseError, QueryParam, QueryResult};
use tokio_postgres::Client;

/// Execute a raw SQL query with parameters.
pub async fn execute_query(
    client: &Client,
    sql: &str,
    params: &[QueryParam],
) -> Result<QueryResult, DatabaseError> {
    // Convert QueryParam to PostgreSQL values
    let pg_params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = params
        .iter()
        .map(|p| match p {
            QueryParam::String(s) => &s as &(dyn tokio_postgres::types::ToSql + Sync),
            QueryParam::Int(i) => &i as &(dyn tokio_postgres::types::ToSql + Sync),
            QueryParam::Float(f) => &f as &(dyn tokio_postgres::types::ToSql + Sync),
            QueryParam::Bool(b) => &b as &(dyn tokio_postgres::types::ToSql + Sync),
            QueryParam::Null => &Option::<i32>::None
                as &(dyn tokio_postgres::types::ToSql + Sync),
            QueryParam::Json(j) => &j as &(dyn tokio_postgres::types::ToSql + Sync),
        })
        .collect();

    // Execute query
    let rows = client.query(sql, &pg_params).await?;

    // Extract column names and values
    let columns: Vec<String> = rows
        .get(0)
        .map(|row| {
            row.columns()
                .iter()
                .map(|col| col.name().to_string())
                .collect()
        })
        .unwrap_or_default();

    let mut result_rows = Vec::new();
    for row in rows {
        let mut row_values = Vec::new();
        for (i, _col) in row.columns().iter().enumerate() {
            let value = row.try_get::<_, String>(i).unwrap_or_default();
            row_values.push(QueryParam::String(value));
        }
        result_rows.push(row_values);
    }

    Ok(QueryResult {
        columns,
        rows: result_rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_param_binding() {
        // Tests in Phase 2
    }
}
```

### Step 5: Expose PyO3 Functions

**File**: `fraiseql_rs/src/lib.rs`

Add these PyO3 function exports:

```rust
/// Execute a query and return JSON results.
///
/// # Arguments
/// * `table` - Table name to query from
/// * `columns` - Columns to select (list of strings)
/// * `where_filters` - WHERE filters as JSON object
/// * `limit` - LIMIT clause (optional)
/// * `offset` - OFFSET clause (optional)
///
/// Returns JSON array of rows
#[pyfunction]
#[pyo3(signature = (table, columns, where_filters=None, limit=None, offset=None))]
fn execute_query_sync(
    table: String,
    columns: Vec<String>,
    where_filters: Option<String>,
    limit: Option<i32>,
    offset: Option<i32>,
) -> PyResult<String> {
    // Async wrapper in Phase 2
    todo!("Implement async query execution wrapper")
}
```

### Step 6: Create Query Tests

**File**: `tests/integration/db/test_rust_queries.py` (NEW)

```python
"""Integration tests for Rust query execution."""

import json
import pytest
from fraiseql.core.database import RustDatabasePool


class TestQueryExecution:
    """Test Rust query execution."""

    @pytest.mark.skipif(True, reason="Requires database connection")
    async def test_simple_select(self, db_pool):
        """Test simple SELECT query."""
        # Test in Phase 2 with database setup
        pass

    @pytest.mark.skipif(True, reason="Requires database connection")
    async def test_select_with_where(self, db_pool):
        """Test SELECT with WHERE clause."""
        pass

    def test_where_clause_generation(self):
        """Test WHERE clause generation."""
        # Can test without database
        from fraiseql_rs import build_where_clause_sql

        where_sql = build_where_clause_sql("users", {"id": {"eq": 123}})
        assert "id = $1" in where_sql


@pytest.fixture
async def db_pool():
    """Database pool fixture."""
    pool = RustDatabasePool("postgres://localhost/fraiseql_test", enabled=False)
    yield pool
```

### Step 7: Verify Parity Tests

**File**: `tests/regression/test_rust_db_parity.py` (NEW)

```python
"""Parity tests: Rust queries vs psycopg queries.

These tests verify that Rust query execution produces identical results
to the existing psycopg implementation.
"""

import pytest


class TestQueryParity:
    """Compare Rust and psycopg query results."""

    @pytest.mark.skipif(True, reason="Phase 2 implementation")
    async def test_simple_select_parity(self, db_pool):
        """Query results should be identical."""
        # SQL: SELECT id, name FROM users LIMIT 10
        # Compare:
        #   - Rust results
        #   - psycopg results
        # Assert: Identical
        pass

    @pytest.mark.skipif(True, reason="Phase 2 implementation")
    async def test_where_clause_parity(self, db_pool):
        """WHERE clauses should filter identically."""
        pass

    @pytest.mark.skipif(True, reason="Phase 2 implementation")
    async def test_limit_offset_parity(self, db_pool):
        """Pagination should work identically."""
        pass
```

---

## Verification Commands

### Unit Tests
```bash
# WHERE clause builder
cargo test -p fraiseql_rs --lib db::where_builder

# SQL generator
cargo test -p fraiseql_rs --lib sql

# Query executor
cargo test -p fraiseql_rs --lib db::query
```

### Integration Tests
```bash
# Query execution (once database is connected)
uv run pytest tests/integration/db/test_rust_queries.py -v

# Parity tests
uv run pytest tests/regression/test_rust_db_parity.py -v
```

### Full Verification
```bash
# Build everything
cargo build -p fraiseql_rs
uv run pip install -e .

# Run all tests
FRAISEQL_DB_BACKEND=rust uv run pytest tests/ -v
```

---

## Acceptance Criteria

### Compile & Build
- [ ] `cargo build -p fraiseql_rs` completes without errors
- [ ] All WHERE clause tests pass
- [ ] All SQL generation tests pass

### Functionality
- [ ] WHERE clauses build correctly (parity with Python)
- [ ] SQL queries execute successfully
- [ ] Query results match psycopg output
- [ ] Parameters properly bound (no SQL injection)

### Performance
- [ ] Query execution is 20-30% faster than psycopg
- [ ] No memory leaks (sustained load testing)
- [ ] Connection pool performs efficiently

### Backward Compatibility
- [ ] All 5991+ existing tests pass
- [ ] No API changes visible to users

---

## Troubleshooting

### Issue: WHERE clause doesn't match Python version

**Check**:
- Compare generated SQL with Python version
- Verify parameter binding order
- Check operator implementations

**Debug**:
```bash
# Print generated SQL
RUST_LOG=debug cargo test -p fraiseql_rs --lib db::where_builder
```

### Issue: Query returns wrong results

**Check**:
- Parameter binding order
- Type conversion (String vs Int vs Json)
- Column ordering

**Debug**:
```rust
eprintln!("SQL: {}", sql);
eprintln!("Params: {:?}", params);
```

---

## Next Phase

After Phase 2 is complete and verified:

👉 Proceed to **Phase 3: Result Streaming**

See: `.phases/rust-postgres-driver/phase-3-result-streaming.md`

---

**Status**: ✅ Blocked until Phase 1 complete
**Duration**: 12 hours
**Branch**: `feature/rust-postgres-driver`
