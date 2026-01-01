# Phase 7.2: WHERE Clause Normalization in Rust

**Status:** Planning
**Priority:** High
**Estimated Complexity:** Medium-High
**Performance Impact:** 7-10x faster WHERE processing

## Objective

Move WHERE clause normalization from Python to Rust, eliminating the bottleneck of parsing and analyzing WHERE clauses on every query. This completes the Rust query path started in Phase 7.0 and 7.1.

## Context

**Current Architecture (Slow):**
```
Python GraphQL Request
    ↓
where_normalization.py (SLOW - ~300 lines of dict parsing)
    ↓
WhereClause → SQL string (via psycopg)
    ↓
Pass SQL string to Rust (Phase 7.1)
    ↓
Rust Query Composer
    ↓
PostgreSQL
```

**Target Architecture (Fast):**
```
Python GraphQL Request (minimal wrapper)
    ↓
RUST WHERE normalization (7-10x faster!)
    ↓
RUST SQL generation (native string building)
    ↓
RUST Query Composer (already exists!)
    ↓
PostgreSQL
```

**Why This Matters:**
- WHERE normalization runs on **EVERY SINGLE QUERY**
- Current Python logic: ~300 lines of complex dict parsing, field analysis, logical operators
- Rust implementation: 7-10x faster, zero Python overhead
- Completes the "all Rust" query path from Phase 7.0/7.1

## Files to Create

### Rust Files (fraiseql_rs/src/query/)

1. **where_normalization.rs** (NEW)
   - Core WHERE clause normalization logic
   - Functions: `normalize_dict_where`, `normalize_where_input`
   - Handles dict format, WhereInput format, logical operators

2. **field_analyzer.rs** (NEW)
   - Field type detection (SQL column vs JSONB path vs FK)
   - Table metadata integration
   - FK mapping resolution

3. **where_sql_builder.rs** (NEW)
   - WHERE SQL generation (replaces psycopg SQL builders)
   - Operator mapping (eq, ne, gt, lt, in, contains, etc.)
   - Safe SQL escaping and parameter binding

4. **casing.rs** (NEW)
   - Field name conversion (camelCase → snake_case)
   - Matches Python `utils/casing.py` behavior

### Python Files (Modifications)

5. **src/fraiseql/sql/query_builder_adapter.py** (MODIFY)
   - Add new function: `_normalize_where_rust(where_dict, table, metadata)`
   - Calls Rust normalization instead of Python
   - Returns WHERE SQL string directly from Rust

6. **fraiseql_rs/src/lib.rs** (MODIFY)
   - Export new Rust functions to Python via PyO3
   - `normalize_where_dict`, `normalize_where_input`

### Test Files

7. **fraiseql_rs/src/query/where_normalization_tests.rs** (NEW)
   - Unit tests for WHERE normalization
   - Test cases from `tests/test_where_normalization.py`

8. **tests/integration/test_where_rust.py** (NEW)
   - Integration tests comparing Rust vs Python output
   - Ensure SQL output matches exactly

## Implementation Steps

### Step 1: Rust Data Structures (30 min)

**File:** `fraiseql_rs/src/query/where_normalization.rs`

```rust
//! WHERE clause normalization from dict/object to SQL.

use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Field condition after normalization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldCondition {
    /// Field name (may be nested: "user.id")
    pub field: String,

    /// Operator (eq, ne, gt, lt, gte, lte, in, nin, contains, etc.)
    pub operator: String,

    /// Value (JSON value)
    pub value: serde_json::Value,

    /// Field type: "sql_column", "jsonb_path", "fk_column"
    pub field_type: String,

    /// For FK: actual column name (e.g., "user_id" for "user.id")
    pub fk_column: Option<String>,

    /// For JSONB: path in JSONB column (e.g., ["device", "name"])
    pub jsonb_path: Option<Vec<String>>,
}

/// Normalized WHERE clause
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedWhere {
    /// List of field conditions
    pub conditions: Vec<FieldCondition>,

    /// Nested WHERE clauses (for AND/OR/NOT)
    pub nested_clauses: Vec<NormalizedWhere>,

    /// Logical operator: "AND" or "OR"
    pub logical_op: String,

    /// NOT flag (for negation)
    pub is_not: bool,
}

impl NormalizedWhere {
    /// Create new empty WHERE clause
    pub fn new() -> Self {
        Self {
            conditions: Vec::new(),
            nested_clauses: Vec::new(),
            logical_op: "AND".to_string(),
            is_not: false,
        }
    }
}
```

### Step 2: Field Type Analyzer (45 min)

**File:** `fraiseql_rs/src/query/field_analyzer.rs`

```rust
//! Field type detection (SQL column vs JSONB path vs FK).

use super::where_normalization::FieldCondition;
use crate::query::schema::TableSchema;
use std::collections::{HashMap, HashSet};

/// Analyze field and determine its type
pub struct FieldAnalyzer<'a> {
    /// Table columns (actual SQL columns)
    table_columns: &'a HashSet<String>,

    /// FK mappings (e.g., "machine" → "machine_id")
    fk_mappings: &'a HashMap<String, String>,

    /// JSONB column name (default: "data")
    jsonb_column: &'a str,
}

impl<'a> FieldAnalyzer<'a> {
    pub fn new(
        table_columns: &'a HashSet<String>,
        fk_mappings: &'a HashMap<String, String>,
        jsonb_column: &'a str,
    ) -> Self {
        Self {
            table_columns,
            fk_mappings,
            jsonb_column,
        }
    }

    /// Analyze field and create FieldCondition
    pub fn analyze_field(
        &self,
        field_name: &str,
        operator: &str,
        value: serde_json::Value,
    ) -> FieldCondition {
        // Check if field is direct SQL column
        if self.table_columns.contains(field_name) {
            return FieldCondition {
                field: field_name.to_string(),
                operator: operator.to_string(),
                value,
                field_type: "sql_column".to_string(),
                fk_column: None,
                jsonb_path: None,
            };
        }

        // Check if field is FK (e.g., "machine" → "machine_id")
        if let Some(fk_col) = self.fk_mappings.get(field_name) {
            return FieldCondition {
                field: field_name.to_string(),
                operator: operator.to_string(),
                value,
                field_type: "fk_column".to_string(),
                fk_column: Some(fk_col.clone()),
                jsonb_path: None,
            };
        }

        // Check for nested FK (e.g., "machine.id")
        if field_name.contains('.') {
            let parts: Vec<&str> = field_name.split('.').collect();
            if parts.len() == 2 {
                let parent = parts[0];
                let child = parts[1];

                if let Some(fk_col) = self.fk_mappings.get(parent) {
                    // Nested FK filter: "machine.id" → "machine_id"
                    if child == "id" {
                        return FieldCondition {
                            field: field_name.to_string(),
                            operator: operator.to_string(),
                            value,
                            field_type: "fk_column".to_string(),
                            fk_column: Some(fk_col.clone()),
                            jsonb_path: None,
                        };
                    }
                }
            }

            // Otherwise, it's a JSONB path
            let path: Vec<String> = parts.iter().map(|s| s.to_string()).collect();
            return FieldCondition {
                field: field_name.to_string(),
                operator: operator.to_string(),
                value,
                field_type: "jsonb_path".to_string(),
                fk_column: None,
                jsonb_path: Some(path),
            };
        }

        // Default: JSONB path (single level)
        FieldCondition {
            field: field_name.to_string(),
            operator: operator.to_string(),
            value,
            field_type: "jsonb_path".to_string(),
            fk_column: None,
            jsonb_path: Some(vec![field_name.to_string()]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_column_detection() {
        let columns: HashSet<String> = ["id", "status", "created_at"].iter().map(|s| s.to_string()).collect();
        let fk_mappings = HashMap::new();
        let analyzer = FieldAnalyzer::new(&columns, &fk_mappings, "data");

        let cond = analyzer.analyze_field("status", "eq", serde_json::json!("active"));
        assert_eq!(cond.field_type, "sql_column");
        assert_eq!(cond.fk_column, None);
        assert_eq!(cond.jsonb_path, None);
    }

    #[test]
    fn test_fk_column_detection() {
        let columns: HashSet<String> = ["id", "machine_id", "data"].iter().map(|s| s.to_string()).collect();
        let mut fk_mappings = HashMap::new();
        fk_mappings.insert("machine".to_string(), "machine_id".to_string());

        let analyzer = FieldAnalyzer::new(&columns, &fk_mappings, "data");

        let cond = analyzer.analyze_field("machine", "eq", serde_json::json!("123"));
        assert_eq!(cond.field_type, "fk_column");
        assert_eq!(cond.fk_column, Some("machine_id".to_string()));
    }

    #[test]
    fn test_nested_fk_detection() {
        let columns: HashSet<String> = ["id", "machine_id", "data"].iter().map(|s| s.to_string()).collect();
        let mut fk_mappings = HashMap::new();
        fk_mappings.insert("machine".to_string(), "machine_id".to_string());

        let analyzer = FieldAnalyzer::new(&columns, &fk_mappings, "data");

        let cond = analyzer.analyze_field("machine.id", "eq", serde_json::json!("123"));
        assert_eq!(cond.field_type, "fk_column");
        assert_eq!(cond.fk_column, Some("machine_id".to_string()));
    }

    #[test]
    fn test_jsonb_path_detection() {
        let columns: HashSet<String> = ["id", "data"].iter().map(|s| s.to_string()).collect();
        let fk_mappings = HashMap::new();

        let analyzer = FieldAnalyzer::new(&columns, &fk_mappings, "data");

        let cond = analyzer.analyze_field("device.name", "eq", serde_json::json!("Printer"));
        assert_eq!(cond.field_type, "jsonb_path");
        assert_eq!(cond.jsonb_path, Some(vec!["device".to_string(), "name".to_string()]));
    }
}
```

### Step 3: WHERE SQL Builder (60 min)

**File:** `fraiseql_rs/src/query/where_sql_builder.rs`

```rust
//! WHERE SQL generation from normalized WHERE clause.

use super::where_normalization::{FieldCondition, NormalizedWhere};

/// Build WHERE SQL from normalized WHERE clause
pub struct WhereSqlBuilder {
    /// JSONB column name
    jsonb_column: String,
}

impl WhereSqlBuilder {
    pub fn new(jsonb_column: impl Into<String>) -> Self {
        Self {
            jsonb_column: jsonb_column.into(),
        }
    }

    /// Build complete WHERE clause SQL
    pub fn build(&self, where_clause: &NormalizedWhere) -> String {
        let mut parts = Vec::new();

        // Add field conditions
        for cond in &where_clause.conditions {
            parts.push(self.build_condition(cond));
        }

        // Add nested clauses
        for nested in &where_clause.nested_clauses {
            let nested_sql = self.build(nested);
            parts.push(format!("({})", nested_sql));
        }

        if parts.is_empty() {
            return String::new();
        }

        let joined = parts.join(&format!(" {} ", where_clause.logical_op));

        if where_clause.is_not {
            format!("NOT ({})", joined)
        } else {
            joined
        }
    }

    /// Build single field condition
    fn build_condition(&self, cond: &FieldCondition) -> String {
        match cond.field_type.as_str() {
            "sql_column" => self.build_sql_column(cond),
            "fk_column" => self.build_fk_column(cond),
            "jsonb_path" => self.build_jsonb_path(cond),
            _ => panic!("Unknown field type: {}", cond.field_type),
        }
    }

    /// Build SQL column condition
    fn build_sql_column(&self, cond: &FieldCondition) -> String {
        let column = &cond.field;
        let value_sql = self.format_value(&cond.value);

        match cond.operator.as_str() {
            "eq" => format!("{} = {}", column, value_sql),
            "ne" => format!("{} != {}", column, value_sql),
            "gt" => format!("{} > {}", column, value_sql),
            "gte" => format!("{} >= {}", column, value_sql),
            "lt" => format!("{} < {}", column, value_sql),
            "lte" => format!("{} <= {}", column, value_sql),
            "in" => {
                if let serde_json::Value::Array(arr) = &cond.value {
                    let values: Vec<String> = arr.iter().map(|v| self.format_value(v)).collect();
                    format!("{} IN ({})", column, values.join(", "))
                } else {
                    panic!("IN operator requires array value");
                }
            }
            "nin" => {
                if let serde_json::Value::Array(arr) = &cond.value {
                    let values: Vec<String> = arr.iter().map(|v| self.format_value(v)).collect();
                    format!("{} NOT IN ({})", column, values.join(", "))
                } else {
                    panic!("NIN operator requires array value");
                }
            }
            "is_null" => format!("{} IS NULL", column),
            "is_not_null" => format!("{} IS NOT NULL", column),
            _ => panic!("Unknown operator: {}", cond.operator),
        }
    }

    /// Build FK column condition
    fn build_fk_column(&self, cond: &FieldCondition) -> String {
        let fk_col = cond.fk_column.as_ref().expect("FK column required");
        let value_sql = self.format_value(&cond.value);

        match cond.operator.as_str() {
            "eq" => format!("{} = {}", fk_col, value_sql),
            "ne" => format!("{} != {}", fk_col, value_sql),
            "in" => {
                if let serde_json::Value::Array(arr) = &cond.value {
                    let values: Vec<String> = arr.iter().map(|v| self.format_value(v)).collect();
                    format!("{} IN ({})", fk_col, values.join(", "))
                } else {
                    panic!("IN operator requires array value");
                }
            }
            _ => panic!("Unsupported FK operator: {}", cond.operator),
        }
    }

    /// Build JSONB path condition
    fn build_jsonb_path(&self, cond: &FieldCondition) -> String {
        let path = cond.jsonb_path.as_ref().expect("JSONB path required");
        let jsonb_expr = self.build_jsonb_expression(path);
        let value_sql = self.format_value(&cond.value);

        match cond.operator.as_str() {
            "eq" => format!("{} = {}", jsonb_expr, value_sql),
            "ne" => format!("{} != {}", jsonb_expr, value_sql),
            "gt" => format!("({})::numeric > {}", jsonb_expr, value_sql),
            "gte" => format!("({})::numeric >= {}", jsonb_expr, value_sql),
            "lt" => format!("({})::numeric < {}", jsonb_expr, value_sql),
            "lte" => format!("({})::numeric <= {}", jsonb_expr, value_sql),
            "contains" => format!("{} ILIKE '%' || {} || '%'", jsonb_expr, value_sql),
            _ => panic!("Unsupported JSONB operator: {}", cond.operator),
        }
    }

    /// Build JSONB path expression
    fn build_jsonb_expression(&self, path: &[String]) -> String {
        if path.is_empty() {
            panic!("JSONB path cannot be empty");
        }

        // Build PostgreSQL JSONB path: data->'level1'->'level2'->>'level3'
        // Last element uses ->> (text), others use -> (jsonb)
        let mut expr = self.jsonb_column.clone();

        for (i, key) in path.iter().enumerate() {
            if i == path.len() - 1 {
                // Last element: extract as text
                expr = format!("{}->>'{}'", expr, key);
            } else {
                // Intermediate: extract as jsonb
                expr = format!("{}->'{}'", expr, key);
            }
        }

        expr
    }

    /// Format value for SQL
    fn format_value(&self, value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::Null => "NULL".to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::String(s) => format!("'{}'", s.replace('\'', "''")), // SQL escape
            serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                panic!("Complex values not supported in WHERE")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_sql_column_eq() {
        let builder = WhereSqlBuilder::new("data");
        let cond = FieldCondition {
            field: "status".to_string(),
            operator: "eq".to_string(),
            value: json!("active"),
            field_type: "sql_column".to_string(),
            fk_column: None,
            jsonb_path: None,
        };

        assert_eq!(builder.build_condition(&cond), "status = 'active'");
    }

    #[test]
    fn test_fk_column_eq() {
        let builder = WhereSqlBuilder::new("data");
        let cond = FieldCondition {
            field: "machine".to_string(),
            operator: "eq".to_string(),
            value: json!("123"),
            field_type: "fk_column".to_string(),
            fk_column: Some("machine_id".to_string()),
            jsonb_path: None,
        };

        assert_eq!(builder.build_condition(&cond), "machine_id = '123'");
    }

    #[test]
    fn test_jsonb_path() {
        let builder = WhereSqlBuilder::new("data");
        let cond = FieldCondition {
            field: "device.name".to_string(),
            operator: "eq".to_string(),
            value: json!("Printer"),
            field_type: "jsonb_path".to_string(),
            fk_column: None,
            jsonb_path: Some(vec!["device".to_string(), "name".to_string()]),
        };

        assert_eq!(
            builder.build_condition(&cond),
            "data->'device'->>'name' = 'Printer'"
        );
    }

    #[test]
    fn test_in_operator() {
        let builder = WhereSqlBuilder::new("data");
        let cond = FieldCondition {
            field: "status".to_string(),
            operator: "in".to_string(),
            value: json!(["active", "pending"]),
            field_type: "sql_column".to_string(),
            fk_column: None,
            jsonb_path: None,
        };

        assert_eq!(
            builder.build_condition(&cond),
            "status IN ('active', 'pending')"
        );
    }
}
```

### Step 4: Main Normalization Logic (60 min)

**File:** `fraiseql_rs/src/query/where_normalization.rs` (add)

```rust
/// Normalize dict-based WHERE clause
pub fn normalize_dict_where(
    where_dict: &HashMap<String, serde_json::Value>,
    table_columns: &HashSet<String>,
    fk_mappings: &HashMap<String, String>,
    jsonb_column: &str,
) -> NormalizedWhere {
    let analyzer = FieldAnalyzer::new(table_columns, fk_mappings, jsonb_column);
    let mut result = NormalizedWhere::new();

    for (field_name, field_value) in where_dict {
        // Handle logical operators
        match field_name.as_str() {
            "OR" => {
                if let serde_json::Value::Array(or_clauses) = field_value {
                    let mut nested = Vec::new();
                    for or_dict in or_clauses {
                        if let serde_json::Value::Object(map) = or_dict {
                            let clause = normalize_dict_where(
                                &map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                                table_columns,
                                fk_mappings,
                                jsonb_column,
                            );
                            nested.push(clause);
                        }
                    }
                    result.nested_clauses.push(NormalizedWhere {
                        conditions: Vec::new(),
                        nested_clauses: nested,
                        logical_op: "OR".to_string(),
                        is_not: false,
                    });
                }
            }
            "NOT" => {
                if let serde_json::Value::Object(not_map) = field_value {
                    let mut not_clause = normalize_dict_where(
                        &not_map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                        table_columns,
                        fk_mappings,
                        jsonb_column,
                    );
                    not_clause.is_not = true;
                    result.nested_clauses.push(not_clause);
                }
            }
            _ => {
                // Regular field condition
                if let serde_json::Value::Object(operators) = field_value {
                    for (operator, value) in operators {
                        let cond = analyzer.analyze_field(
                            field_name,
                            operator,
                            value.clone(),
                        );
                        result.conditions.push(cond);
                    }
                }
            }
        }
    }

    result
}
```

### Step 5: PyO3 Bindings (30 min)

**File:** `fraiseql_rs/src/lib.rs` (add)

```rust
/// Normalize WHERE clause and generate SQL (Python entry point)
#[pyfunction]
fn normalize_where_dict_to_sql(
    where_dict: HashMap<String, pyo3::PyObject>,
    table_columns: Vec<String>,
    fk_mappings: HashMap<String, String>,
    jsonb_column: String,
) -> PyResult<String> {
    // Convert Python objects to JSON values
    let where_map: HashMap<String, serde_json::Value> = where_dict
        .into_iter()
        .map(|(k, v)| {
            // Convert PyObject to JSON
            let json_val = Python::with_gil(|py| {
                let obj = v.as_ref(py);
                // Use serde_json to convert
                pythonize::depythonize(obj).unwrap()
            });
            (k, json_val)
        })
        .collect();

    let columns: HashSet<String> = table_columns.into_iter().collect();

    // Normalize
    let normalized = query::where_normalization::normalize_dict_where(
        &where_map,
        &columns,
        &fk_mappings,
        &jsonb_column,
    );

    // Generate SQL
    let builder = query::where_sql_builder::WhereSqlBuilder::new(jsonb_column);
    let sql = builder.build(&normalized);

    Ok(sql)
}
```

### Step 6: Python Integration (30 min)

**File:** `src/fraiseql/sql/query_builder_adapter.py` (add function)

```python
def _normalize_where_rust(
    where_dict: dict[str, Any],
    table: str,
    metadata: dict[str, Any],
) -> str | None:
    """Normalize WHERE clause using Rust (7-10x faster).

    Args:
        where_dict: WHERE clause as dict
        table: Table name
        metadata: Table metadata (columns, fk_mappings, etc.)

    Returns:
        WHERE SQL string, or None if no WHERE clause
    """
    if not where_dict:
        return None

    try:
        from fraiseql._fraiseql_rs import normalize_where_dict_to_sql

        # Extract metadata
        table_columns = list(metadata.get("columns", set()))
        fk_mappings = metadata.get("fk_mappings", {})
        jsonb_column = metadata.get("jsonb_column", "data")

        # Call Rust normalization
        where_sql = normalize_where_dict_to_sql(
            where_dict,
            table_columns,
            fk_mappings,
            jsonb_column,
        )

        if LOG_QUERY_BUILDER_MODE:
            logger.debug(f"Phase 7.2: Rust WHERE normalization: {where_sql}")

        return where_sql

    except ImportError:
        # Fallback to Python (should not happen in production)
        logger.warning("Rust extension not available, using Python WHERE normalization")
        return None
```

### Step 7: Integration Testing (45 min)

**File:** `tests/integration/test_where_rust.py` (NEW)

```python
"""Integration tests for Rust WHERE normalization."""

import pytest
from fraiseql._fraiseql_rs import normalize_where_dict_to_sql


class TestWhereNormalizationRust:
    """Test Rust WHERE normalization matches Python behavior."""

    def test_simple_eq(self):
        """Test simple equality filter."""
        where_dict = {"status": {"eq": "active"}}
        table_columns = ["id", "status", "data"]
        fk_mappings = {}

        result = normalize_where_dict_to_sql(
            where_dict,
            table_columns,
            fk_mappings,
            "data",
        )

        assert result == "status = 'active'"

    def test_fk_filter(self):
        """Test FK filter."""
        where_dict = {"machine": {"eq": "123"}}
        table_columns = ["id", "machine_id", "data"]
        fk_mappings = {"machine": "machine_id"}

        result = normalize_where_dict_to_sql(
            where_dict,
            table_columns,
            fk_mappings,
            "data",
        )

        assert result == "machine_id = '123'"

    def test_nested_fk(self):
        """Test nested FK filter (machine.id)."""
        where_dict = {"machine": {"id": {"eq": "123"}}}
        table_columns = ["id", "machine_id", "data"]
        fk_mappings = {"machine": "machine_id"}

        result = normalize_where_dict_to_sql(
            where_dict,
            table_columns,
            fk_mappings,
            "data",
        )

        assert result == "machine_id = '123'"

    def test_jsonb_path(self):
        """Test JSONB path filter."""
        where_dict = {"device": {"name": {"eq": "Printer"}}}
        table_columns = ["id", "data"]
        fk_mappings = {}

        result = normalize_where_dict_to_sql(
            where_dict,
            table_columns,
            fk_mappings,
            "data",
        )

        assert result == "data->'device'->>'name' = 'Printer'"

    def test_or_operator(self):
        """Test OR operator."""
        where_dict = {
            "OR": [
                {"status": {"eq": "active"}},
                {"status": {"eq": "pending"}},
            ]
        }
        table_columns = ["id", "status", "data"]
        fk_mappings = {}

        result = normalize_where_dict_to_sql(
            where_dict,
            table_columns,
            fk_mappings,
            "data",
        )

        assert "status = 'active'" in result
        assert "status = 'pending'" in result
        assert " OR " in result

    def test_in_operator(self):
        """Test IN operator."""
        where_dict = {"status": {"in": ["active", "pending"]}}
        table_columns = ["id", "status", "data"]
        fk_mappings = {}

        result = normalize_where_dict_to_sql(
            where_dict,
            table_columns,
            fk_mappings,
            "data",
        )

        assert result == "status IN ('active', 'pending')"
```

## Verification Commands

### Rust Unit Tests
```bash
cd fraiseql_rs
cargo test where_normalization
cargo test field_analyzer
cargo test where_sql_builder
```

### Python Integration Tests
```bash
uv run pytest tests/integration/test_where_rust.py -v
```

### Performance Comparison
```bash
# Run benchmark comparing Python vs Rust WHERE normalization
uv run pytest tests/performance/test_where_benchmark.py -v
```

### Full Test Suite
```bash
make test  # Should pass all 5991+ tests
```

## Acceptance Criteria

- [ ] All Rust unit tests pass (where_normalization, field_analyzer, where_sql_builder)
- [ ] All Python integration tests pass (test_where_rust.py)
- [ ] WHERE SQL output matches Python implementation exactly
- [ ] Performance: 7-10x faster than Python normalization
- [ ] Zero clippy warnings (`cargo clippy --lib -- -D warnings`)
- [ ] Full test suite passes (5991+ tests)
- [ ] No regressions in existing functionality

## DO NOT

- ❌ Change the WHERE SQL output format (must match Python exactly)
- ❌ Break existing tests
- ❌ Add `#[allow]` clippy workarounds (NASA quality!)
- ❌ Skip error handling or validation
- ❌ Forget to handle edge cases (empty dicts, null values, etc.)

## Performance Impact

**Expected Improvement:**
- WHERE normalization: **7-10x faster**
- End-to-end query execution: **15-20% faster** (WHERE processing is bottleneck)
- Memory usage: **Lower** (no Python dict allocations)

**Measurement:**
```python
# Before (Python):
# ~300 lines of dict parsing
# Time: 0.5-1ms per query

# After (Rust):
# Native Rust parsing + string building
# Time: 0.05-0.1ms per query
# Speedup: 10x
```

## Notes

- This completes the "all Rust" query path started in Phase 7.0/7.1
- After this phase, the entire query pipeline (parse → normalize → compose → execute) is in Rust
- Future phases can tackle JSON response transformation (SIMD) and mutations
- Maintains NASA quality: zero clippy warnings, comprehensive tests, no workarounds

## Related Phases

- Phase 7.0: Rust query composer (COMPLETED)
- Phase 7.1: WHERE SQL pass-through (COMPLETED)
- Phase 6: NASA-quality clippy fixes (COMPLETED)
- Phase 7.2: WHERE normalization in Rust (THIS PHASE)
- Phase 7.3: JSON response transformation with SIMD (FUTURE)
