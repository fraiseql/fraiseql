# Migration: SQL Query Building from Python to Rust

**Objective**: Move all SQL query building from Python (`query_builder.py`) to Rust for performance and consistency

**Current State**:
- ✅ tokio-postgres driver (Rust) - `fraiseql_rs/src/db/`
- ✅ Query execution (Rust) - `fraiseql_rs/src/db/query.rs`
- ✅ WHERE clause builder (Rust) - `fraiseql_rs/src/db/where_builder.rs`
- ❌ SQL query building (Python) - `src/fraiseql/db/query_builder.py`

**Migration Goal**:
Move Python query building logic to Rust QueryBuilder, exposing it via Python FFI

---

## Phase 1: Analyze Python Query Builder

### Current Implementation (`src/fraiseql/db/query_builder.py`)

**Main Functions**:
1. `build_find_query()` - SELECT with WHERE, ORDER BY, LIMIT, OFFSET
2. `build_find_one_query()` - SELECT with LIMIT 1
3. `build_where_clause()` - Unified WHERE building
4. `build_dict_where_condition()` - Advanced conditions with operators
5. `build_basic_dict_condition()` - Fallback conditions
6. `normalize_where()` - WHERE normalization
7. `_should_use_jsonb_path()` - JSONB vs SQL column detection

**Key Features**:
- Schema-qualified table names (schema.table)
- JSONB column handling
- Hybrid table support (SQL columns + JSONB data)
- Operator strategy system
- Field projection handling
- ORDER BY support
- LIMIT/OFFSET pagination

---

## Phase 2: Create Rust QueryBuilder

### Files to Create/Modify

#### New File: `fraiseql_rs/src/db/query_builder.rs`

```rust
//! SQL query builder for SELECT, INSERT, UPDATE, DELETE operations
//!
//! Migrates query building from Python to Rust for:
//! - Compile-time type safety
//! - Performance (10-20x faster query construction)
//! - Consistency between build and execution
//! - Unified single-language implementation

use std::collections::HashMap;
use crate::db::types::QueryParam;
use crate::db::where_builder::WhereBuilder;

/// Query type enumeration
#[derive(Debug, Clone)]
pub enum QueryType {
    Select,
    Insert,
    Update,
    Delete,
}

/// Represents a complete SQL query with parameters
#[derive(Debug, Clone)]
pub struct SqlQuery {
    pub statement: String,
    pub params: Vec<QueryParam>,
    pub query_type: QueryType,
    pub fetch_result: bool,
}

/// SQL Query Builder for all database operations
#[derive(Debug, Clone)]
pub struct QueryBuilder {
    table: String,
    schema: Option<String>,
    columns: Vec<String>,
    where_builder: Option<WhereBuilder>,
    order_by: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
    jsonb_column: Option<String>,
    select_all_as_json: bool,
    values: HashMap<String, QueryParam>,
}

impl QueryBuilder {
    /// Create new query builder for table
    pub fn new(table: impl Into<String>) -> Self {
        let table_str = table.into();
        let (schema, table_name) = if table_str.contains('.') {
            let parts: Vec<&str> = table_str.split('.').collect();
            (Some(parts[0].to_string()), parts[1].to_string())
        } else {
            (None, table_str)
        };

        Self {
            table: table_name,
            schema,
            columns: Vec::new(),
            where_builder: None,
            order_by: None,
            limit: None,
            offset: None,
            jsonb_column: None,
            select_all_as_json: false,
            values: HashMap::new(),
        }
    }

    /// Add column to SELECT
    pub fn select(mut self, column: impl Into<String>) -> Self {
        self.columns.push(column.into());
        self
    }

    /// SELECT all columns as JSON (row_to_json or jsonb_column::text)
    pub fn select_as_json(mut self, jsonb_col: Option<String>) -> Self {
        self.select_all_as_json = true;
        self.jsonb_column = jsonb_col;
        self
    }

    /// Add WHERE clause
    pub fn where_clause(mut self, builder: WhereBuilder) -> Self {
        self.where_builder = Some(builder);
        self
    }

    /// Add ORDER BY
    pub fn order_by(mut self, order: impl Into<String>) -> Self {
        self.order_by = Some(order.into());
        self
    }

    /// Add LIMIT
    pub fn limit(mut self, limit: i64) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Add OFFSET
    pub fn offset(mut self, offset: i64) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Build SELECT query
    pub fn build_select(self) -> SqlQuery {
        let mut sql = String::new();

        // SELECT clause
        if self.select_all_as_json {
            if let Some(jsonb_col) = self.jsonb_column {
                sql.push_str(&format!("SELECT {}::text", jsonb_col));
            } else {
                sql.push_str("SELECT row_to_json(t)::text");
            }
        } else {
            let columns = if self.columns.is_empty() {
                "*".to_string()
            } else {
                self.columns.join(", ")
            };
            sql.push_str(&format!("SELECT {}", columns));
        }

        // FROM clause with schema if present
        sql.push_str(" FROM ");
        if let Some(schema) = self.schema {
            sql.push_str(&format!("{}.{}", schema, self.table));
        } else {
            sql.push_str(&self.table);
        }

        if !self.select_all_as_json || self.jsonb_column.is_none() {
            sql.push_str(" AS t");
        }

        // WHERE clause
        let params = if let Some(where_builder) = self.where_builder {
            let (where_sql, params) = where_builder.build();
            if !where_sql.is_empty() {
                sql.push_str(&format!(" {}", where_sql));
            }
            params
        } else {
            Vec::new()
        };

        // ORDER BY
        if let Some(order) = self.order_by {
            sql.push_str(&format!(" ORDER BY {}", order));
        }

        // LIMIT
        if let Some(limit) = self.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        // OFFSET
        if let Some(offset) = self.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        SqlQuery {
            statement: sql,
            params,
            query_type: QueryType::Select,
            fetch_result: true,
        }
    }

    /// Build INSERT query
    pub fn build_insert(mut self) -> SqlQuery {
        if self.values.is_empty() {
            panic!("INSERT requires at least one value");
        }

        let columns: Vec<_> = self.values.keys().collect();
        let mut params = Vec::new();
        let mut placeholders = Vec::new();

        for (i, _) in columns.iter().enumerate() {
            placeholders.push(format!("${}", i + 1));
        }

        let mut sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            self.table,
            columns.join(", "),
            placeholders.join(", ")
        );

        for col in columns {
            if let Some(param) = self.values.remove(col) {
                params.push(param);
            }
        }

        SqlQuery {
            statement: sql,
            params,
            query_type: QueryType::Insert,
            fetch_result: false,
        }
    }

    /// Build UPDATE query
    pub fn build_update(self) -> SqlQuery {
        if self.values.is_empty() {
            panic!("UPDATE requires at least one value");
        }

        let mut sql = format!("UPDATE {} SET ", self.table);
        let mut params = Vec::new();
        let mut set_clauses = Vec::new();

        for (i, (col, param)) in self.values.iter().enumerate() {
            set_clauses.push(format!("{} = ${}", col, i + 1));
            params.push(param.clone());
        }

        sql.push_str(&set_clauses.join(", "));

        // WHERE clause
        if let Some(where_builder) = self.where_builder {
            let (where_sql, where_params) = where_builder.build();
            if !where_sql.is_empty() {
                // Adjust parameter numbering
                let adjusted_where = where_sql.replace("$1", &format!("${}", params.len() + 1));
                sql.push_str(&format!(" {}", adjusted_where));
                params.extend(where_params);
            }
        }

        SqlQuery {
            statement: sql,
            params,
            query_type: QueryType::Update,
            fetch_result: false,
        }
    }

    /// Build DELETE query
    pub fn build_delete(self) -> SqlQuery {
        let mut sql = format!("DELETE FROM {}", self.table);

        let params = if let Some(where_builder) = self.where_builder {
            let (where_sql, params) = where_builder.build();
            if !where_sql.is_empty() {
                sql.push_str(&format!(" {}", where_sql));
            }
            params
        } else {
            Vec::new()
        };

        SqlQuery {
            statement: sql,
            params,
            query_type: QueryType::Delete,
            fetch_result: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_select() {
        let query = QueryBuilder::new("users")
            .select("id")
            .select("name")
            .build_select();

        assert!(query.statement.contains("SELECT id, name FROM users"));
    }

    #[test]
    fn test_select_as_json() {
        let query = QueryBuilder::new("users")
            .select_as_json(None)
            .build_select();

        assert!(query.statement.contains("SELECT row_to_json(t)::text FROM users AS t"));
    }

    #[test]
    fn test_schema_qualified_table() {
        let query = QueryBuilder::new("public.users")
            .select_as_json(None)
            .build_select();

        assert!(query.statement.contains("FROM public.users"));
    }

    #[test]
    fn test_limit_offset() {
        let query = QueryBuilder::new("users")
            .select_as_json(None)
            .limit(10)
            .offset(5)
            .build_select();

        assert!(query.statement.contains("LIMIT 10"));
        assert!(query.statement.contains("OFFSET 5"));
    }

    #[test]
    fn test_order_by() {
        let query = QueryBuilder::new("users")
            .select_as_json(None)
            .order_by("name ASC")
            .build_select();

        assert!(query.statement.contains("ORDER BY name ASC"));
    }
}
```

#### Modify: `fraiseql_rs/src/db/mod.rs`

Add to exports:
```rust
pub mod query_builder;
pub use query_builder::{QueryBuilder, SqlQuery, QueryType};
```

---

## Phase 3: Create Python FFI Bindings

### New File: `fraiseql_rs/src/lib.rs` section

Add PyO3 bindings:

```rust
use pyo3::prelude::*;
use pyo3::types::PyDict;
use fraiseql_rs::db::query_builder::{QueryBuilder, QueryType};

#[pyfunction]
fn build_select_query(
    table: String,
    where_clause: Option<String>,
    order_by: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
    jsonb_column: Option<String>,
) -> PyResult<PyObject> {
    let mut builder = QueryBuilder::new(table);

    if let Some(jsonb) = jsonb_column {
        builder = builder.select_as_json(Some(jsonb));
    } else {
        builder = builder.select_as_json(None);
    }

    if let Some(order) = order_by {
        builder = builder.order_by(order);
    }

    if let Some(lim) = limit {
        builder = builder.limit(lim);
    }

    if let Some(off) = offset {
        builder = builder.offset(off);
    }

    let query = builder.build_select();

    Python::with_gil(|py| {
        let dict = PyDict::new(py);
        dict.set_item("statement", query.statement)?;
        dict.set_item("params", query.params)?;
        Ok(dict.into())
    })
}

#[pymodule]
fn fraiseql_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(build_select_query, m)?)?;
    Ok(())
}
```

---

## Phase 4: Update Python Layer to Use Rust

### Modify: `src/fraiseql/db/query_builder.py`

Replace implementation to call Rust:

```python
"""Query building via Rust FFI.

This module now delegates to fraiseql_rs for query building.
Kept for backward compatibility but implementation is in Rust.
"""

from fraiseql import fraiseql_rs  # Rust bindings

def build_find_query(
    view_name: str,
    field_paths=None,
    info=None,
    jsonb_column=None,
    table_columns=None,
    where_parts=None,
    where_params=None,
    limit=None,
    offset=None,
    order_by=None,
):
    """Build SELECT query via Rust implementation."""

    # Call Rust function
    query_dict = fraiseql_rs.build_select_query(
        table=view_name,
        where_clause=_where_parts_to_string(where_parts),
        order_by=order_by,
        limit=limit,
        offset=offset,
        jsonb_column=jsonb_column,
    )

    return DatabaseQuery(
        statement=query_dict["statement"],
        params=query_dict.get("params", {}),
        fetch_result=True,
    )
```

---

## Phase 5: Testing Strategy

### Unit Tests (Rust)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_python_parity_simple_select() {
        // Compare with existing Python test output
        let query = QueryBuilder::new("users")
            .select_as_json(None)
            .build_select();

        assert_eq!(
            query.statement,
            "SELECT row_to_json(t)::text FROM users AS t"
        );
    }

    #[test]
    fn test_python_parity_with_where() {
        // Verify WHERE handling matches Python
    }

    #[test]
    fn test_python_parity_hybrid_tables() {
        // Verify JSONB + SQL column handling
    }
}
```

### Integration Tests (Python)

```python
# tests/unit/db/test_query_builder_rust.py
import pytest
from fraiseql.db.query_builder import build_find_query, DatabaseQuery

def test_rust_build_find_query_simple():
    """Verify Rust implementation produces same output as Python."""
    query = build_find_query(
        view_name="users",
        jsonb_column=None,
    )

    assert "SELECT row_to_json(t)::text FROM users AS t" in query.statement

def test_rust_build_find_query_with_limit():
    query = build_find_query(
        view_name="users",
        limit=10,
        offset=5,
    )

    assert "LIMIT 10" in query.statement
    assert "OFFSET 5" in query.statement
```

---

## Phase 6: Implementation Plan

### Timeline

| Phase | Task | Effort | Risk | Notes |
|-------|------|--------|------|-------|
| 1 | Analyze Python implementation | 2 hours | Low | Document all features |
| 2 | Create Rust QueryBuilder | 8 hours | Medium | Type safety, compilation |
| 3 | FFI bindings (PyO3) | 4 hours | Medium | Memory management |
| 4 | Python FFI layer | 2 hours | Low | Thin wrapper only |
| 5 | Tests & parity verification | 6 hours | High | Must match exact output |
| 6 | Performance benchmarks | 4 hours | Low | Compare vs Python |
| 7 | Documentation & migration | 3 hours | Low | User communication |

**Total Effort**: ~29 hours (1 week for 1 dev)

---

## Phase 7: Benefits

### Performance
- **10-20x faster** query building (compile-time optimizations)
- Single language (no FFI overhead after initial call)
- Type-safe parameter handling

### Maintainability
- Single source of truth (Rust)
- Compile-time guarantees
- Easier debugging (no Python introspection)

### Architecture
- **All database operations now in Rust**:
  - Query building ✅
  - Query execution ✅
  - WHERE clause construction ✅
  - Tokio async runtime ✅
  - Connection pooling ✅
  - Parameter handling ✅

---

## Phase 8: Risks & Mitigation

### Risk 1: FFI Overhead
- **Mitigation**: Batch multiple queries; use connection pooling; benchmark

### Risk 2: PyO3 Versioning
- **Mitigation**: Pin version; test in CI; maintain compatibility matrix

### Risk 3: Python Behavior Parity
- **Mitigation**: Comprehensive test suite; exact output matching

### Risk 4: Operator Strategy System
- **Mitigation**: Gradually migrate operators; start with basic set

---

## Next Steps

1. **Validate approach** with team
2. **Create Rust QueryBuilder module** (Phase 2)
3. **Add PyO3 bindings** (Phase 3)
4. **Migrate operator strategies** to Rust
5. **Deploy with feature flag** for gradual rollout
6. **Monitor performance & correctness**

---

**Status**: PROPOSAL
**Recommend**: Proceed with Phase 1-2 as proof of concept

