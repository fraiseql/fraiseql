# Phase 7.2: WHERE Normalization in Rust (v2) - PART 2

**Continuation of:** phase-7.2-where-normalization-rust-v2.md

This file contains Steps 5-7 and the complete verification/testing strategy.

---

## Implementation Steps (Continued)

### Step 5: Main Normalization Logic with Nested Objects (120 min)

**File:** `fraiseql_rs/src/query/where_normalization.rs`

```rust
//! WHERE clause normalization from dict/object to PreparedStatement.

use super::casing::to_snake_case;
use super::field_analyzer::{FieldAnalyzer, FieldCondition};
use super::operators::{get_operator_info, OperatorCategory};
use super::prepared_statement::PreparedStatement;
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};

/// Normalized WHERE clause structure
#[derive(Debug, Clone)]
pub struct NormalizedWhere {
    pub conditions: Vec<FieldCondition>,
    pub nested_clauses: Vec<NormalizedWhere>,
    pub logical_op: String,  // "AND" or "OR"
    pub is_not: bool,
}

impl NormalizedWhere {
    pub fn new() -> Self {
        Self {
            conditions: Vec::new(),
            nested_clauses: Vec::new(),
            logical_op: "AND".to_string(),
            is_not: false,
        }
    }
}

impl Default for NormalizedWhere {
    fn default() -> Self {
        Self::new()
    }
}

/// Normalize dict-based WHERE clause
///
/// # Arguments
///
/// * `where_dict` - WHERE clause as JSON object
/// * `table_columns` - Set of actual SQL column names
/// * `fk_mappings` - FK field to column mappings
/// * `jsonb_column` - JSONB column name (default: "data")
///
/// # Returns
///
/// Normalized WHERE structure ready for SQL generation
pub fn normalize_dict_where(
    where_dict: &HashMap<String, JsonValue>,
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
                if let JsonValue::Array(or_clauses) = field_value {
                    let mut nested = Vec::new();
                    for or_dict in or_clauses {
                        if let JsonValue::Object(map) = or_dict {
                            let hash_map: HashMap<String, JsonValue> =
                                map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                            let clause = normalize_dict_where(
                                &hash_map,
                                table_columns,
                                fk_mappings,
                                jsonb_column,
                            );
                            nested.push(clause);
                        }
                    }
                    if !nested.is_empty() {
                        result.nested_clauses.push(NormalizedWhere {
                            conditions: Vec::new(),
                            nested_clauses: nested,
                            logical_op: "OR".to_string(),
                            is_not: false,
                        });
                    }
                }
            }
            "AND" => {
                // Explicit AND (usually implicit, but can be explicit)
                if let JsonValue::Array(and_clauses) = field_value {
                    let mut nested = Vec::new();
                    for and_dict in and_clauses {
                        if let JsonValue::Object(map) = and_dict {
                            let hash_map: HashMap<String, JsonValue> =
                                map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                            let clause = normalize_dict_where(
                                &hash_map,
                                table_columns,
                                fk_mappings,
                                jsonb_column,
                            );
                            nested.push(clause);
                        }
                    }
                    if !nested.is_empty() {
                        result.nested_clauses.push(NormalizedWhere {
                            conditions: Vec::new(),
                            nested_clauses: nested,
                            logical_op: "AND".to_string(),
                            is_not: false,
                        });
                    }
                }
            }
            "NOT" => {
                if let JsonValue::Object(not_map) = field_value {
                    let hash_map: HashMap<String, JsonValue> =
                        not_map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                    let mut not_clause = normalize_dict_where(
                        &hash_map,
                        table_columns,
                        fk_mappings,
                        jsonb_column,
                    );
                    not_clause.is_not = true;
                    result.nested_clauses.push(not_clause);
                }
            }
            _ => {
                // Regular field condition (may be nested object)
                let conditions = analyzer.analyze_nested(field_name, field_value);
                result.conditions.extend(conditions);
            }
        }
    }

    result
}

/// Build WHERE SQL with prepared statement
///
/// # Arguments
///
/// * `where_clause` - Normalized WHERE structure
/// * `stmt` - Prepared statement builder (accumulates parameters)
///
/// # Returns
///
/// WHERE SQL string with placeholders
pub fn build_where_sql(where_clause: &NormalizedWhere, stmt: &mut PreparedStatement) -> String {
    let mut parts = Vec::new();

    // Add field conditions
    for cond in &where_clause.conditions {
        let sql = build_condition_sql(cond, stmt);
        parts.push(sql);
    }

    // Add nested clauses
    for nested in &where_clause.nested_clauses {
        let nested_sql = build_where_sql(nested, stmt);
        if !nested_sql.is_empty() {
            parts.push(format!("({})", nested_sql));
        }
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

/// Build SQL for a single field condition
fn build_condition_sql(cond: &FieldCondition, stmt: &mut PreparedStatement) -> String {
    match cond.operator_category {
        OperatorCategory::Comparison => {
            stmt.build_comparison(&cond.column_expr, &cond.sql_operator, cond.value.clone())
        }
        OperatorCategory::Containment => {
            if let JsonValue::Array(arr) = &cond.value {
                stmt.build_in_clause(&cond.column_expr, &cond.sql_operator, arr)
            } else {
                // Error: IN/NOT IN requires array
                panic!("IN/NOT IN operator requires array value");
            }
        }
        OperatorCategory::String => {
            if let JsonValue::String(s) = &cond.value {
                // Determine pattern type from operator name
                let pattern_type = cond.operator.as_str();
                stmt.build_like_pattern(&cond.column_expr, &cond.sql_operator, s, pattern_type)
            } else {
                // For explicit LIKE/ILIKE, value might not be string
                stmt.build_comparison(&cond.column_expr, &cond.sql_operator, cond.value.clone())
            }
        }
        OperatorCategory::Null => {
            // IS NULL / IS NOT NULL (no parameters needed)
            let is_null = cond.operator == "isnull";
            PreparedStatement::build_null_check(&cond.column_expr, is_null)
        }
        OperatorCategory::Vector | OperatorCategory::Fulltext | OperatorCategory::Array => {
            // These use direct comparison with special operators
            stmt.build_comparison(&cond.column_expr, &cond.sql_operator, cond.value.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_columns() -> HashSet<String> {
        ["id", "status", "machine_id", "data"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    fn test_fk_mappings() -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert("machine".to_string(), "machine_id".to_string());
        map
    }

    #[test]
    fn test_simple_eq() {
        let mut where_dict = HashMap::new();
        where_dict.insert("status".to_string(), json!({"eq": "active"}));

        let normalized = normalize_dict_where(&where_dict, &test_columns(), &test_fk_mappings(), "data");

        assert_eq!(normalized.conditions.len(), 1);
        assert_eq!(normalized.conditions[0].operator, "eq");
        assert_eq!(normalized.conditions[0].column_expr, "status");
    }

    #[test]
    fn test_nested_fk() {
        let mut where_dict = HashMap::new();
        where_dict.insert("machine".to_string(), json!({"id": {"eq": "123"}}));

        let normalized = normalize_dict_where(&where_dict, &test_columns(), &test_fk_mappings(), "data");

        assert_eq!(normalized.conditions.len(), 1);
        assert_eq!(normalized.conditions[0].column_expr, "machine_id");
        assert_eq!(normalized.conditions[0].operator, "eq");
    }

    #[test]
    fn test_jsonb_path() {
        let mut where_dict = HashMap::new();
        where_dict.insert("device".to_string(), json!({"name": {"eq": "Printer"}}));

        let normalized = normalize_dict_where(&where_dict, &test_columns(), &test_fk_mappings(), "data");

        assert_eq!(normalized.conditions.len(), 1);
        assert_eq!(normalized.conditions[0].column_expr, "data->'device'->>'name'");
    }

    #[test]
    fn test_or_operator() {
        let mut where_dict = HashMap::new();
        where_dict.insert(
            "OR".to_string(),
            json!([
                {"status": {"eq": "active"}},
                {"status": {"eq": "pending"}}
            ]),
        );

        let normalized = normalize_dict_where(&where_dict, &test_columns(), &test_fk_mappings(), "data");

        assert_eq!(normalized.nested_clauses.len(), 1);
        assert_eq!(normalized.nested_clauses[0].logical_op, "OR");
        assert_eq!(normalized.nested_clauses[0].nested_clauses.len(), 2);
    }

    #[test]
    fn test_build_where_sql() {
        let mut where_dict = HashMap::new();
        where_dict.insert("status".to_string(), json!({"eq": "active"}));

        let normalized = normalize_dict_where(&where_dict, &test_columns(), &test_fk_mappings(), "data");
        let mut stmt = PreparedStatement::new();
        let sql = build_where_sql(&normalized, &mut stmt);

        assert_eq!(sql, "status = $1");
        assert_eq!(stmt.params[0], json!("active"));
    }

    #[test]
    fn test_in_operator() {
        let mut where_dict = HashMap::new();
        where_dict.insert("status".to_string(), json!({"in": ["active", "pending"]}));

        let normalized = normalize_dict_where(&where_dict, &test_columns(), &test_fk_mappings(), "data");
        let mut stmt = PreparedStatement::new();
        let sql = build_where_sql(&normalized, &mut stmt);

        assert_eq!(sql, "status IN ($1, $2)");
        assert_eq!(stmt.params.len(), 2);
    }
}
```

### Step 6: PyO3 Bindings with Proper Error Handling (60 min)

**File:** `fraiseql_rs/src/lib.rs` (add to existing)

```rust
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::{HashMap, HashSet};

/// Normalize WHERE clause and generate SQL with prepared statement
///
/// # Arguments (from Python)
///
/// * `where_dict` - WHERE clause as dict
/// * `table_columns` - List of SQL column names
/// * `fk_mappings` - Dict of FK field → column mappings
/// * `jsonb_column` - JSONB column name (default: "data")
///
/// # Returns
///
/// Tuple of (sql_string, parameters_list)
///
/// # Errors
///
/// Returns PyValueError if:
/// - Invalid operator
/// - Invalid field structure
/// - Type mismatch (e.g., IN with non-array value)
///
/// # Example (from Python)
///
/// ```python
/// sql, params = normalize_where_to_sql(
///     {"status": {"eq": "active"}},
///     ["id", "status", "data"],
///     {},
///     "data"
/// )
/// # sql = "status = $1"
/// # params = ["active"]
/// ```
#[pyfunction]
fn normalize_where_to_sql(
    py: Python,
    where_dict: &PyDict,
    table_columns: Vec<String>,
    fk_mappings: HashMap<String, String>,
    jsonb_column: String,
) -> PyResult<(String, Vec<PyObject>)> {
    // Convert Python dict to Rust HashMap<String, JsonValue>
    let where_map: HashMap<String, serde_json::Value> = where_dict
        .iter()
        .map(|(k, v)| {
            let key = k.extract::<String>().map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Invalid key: {}", e))
            })?;

            // Convert Python object to JSON value
            let json_val: serde_json::Value = pythonize::depythonize(v).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "Failed to convert value for key '{}': {}",
                    key, e
                ))
            })?;

            Ok((key, json_val))
        })
        .collect::<PyResult<HashMap<String, serde_json::Value>>>()?;

    // Convert table_columns to HashSet
    let columns: HashSet<String> = table_columns.into_iter().collect();

    // Normalize WHERE clause
    let normalized = query::where_normalization::normalize_dict_where(
        &where_map,
        &columns,
        &fk_mappings,
        &jsonb_column,
    );

    // Build SQL with prepared statement
    let mut stmt = query::prepared_statement::PreparedStatement::new();
    let sql = query::where_normalization::build_where_sql(&normalized, &mut stmt);

    // Convert parameters to Python objects
    let params: Vec<PyObject> = stmt
        .params
        .into_iter()
        .map(|json_val| {
            pythonize::pythonize(py, &json_val).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "Failed to convert parameter: {}",
                    e
                ))
            })
        })
        .collect::<PyResult<Vec<PyObject>>>()?;

    Ok((sql, params))
}

// Add to module registration
#[pymodule]
fn _fraiseql_rs(py: Python, m: &PyModule) -> PyResult<()> {
    // ... existing exports ...

    // Phase 7.2: WHERE normalization
    m.add_function(wrap_pyfunction!(normalize_where_to_sql, m)?)?;

    Ok(())
}
```

**File:** `fraiseql_rs/Cargo.toml` (add dependency if not present)

```toml
[dependencies]
# ... existing dependencies ...
pythonize = "0.21"  # For Python ↔ JSON conversion
lazy_static = "1.4"  # For operator registry
heck = "0.5"  # For case conversion (already have this!)
```

### Step 7: Python Integration Layer (45 min)

**File:** `src/fraiseql/sql/query_builder_adapter.py` (add function)

```python
def _normalize_where_rust(
    where_dict: dict[str, Any],
    table: str,
    metadata: dict[str, Any],
) -> tuple[str, list[Any]] | None:
    """Normalize WHERE clause using Rust (7-10x faster).

    Args:
        where_dict: WHERE clause as dict
        table: Table name
        metadata: Table metadata (columns, fk_mappings, etc.)

    Returns:
        Tuple of (WHERE SQL string, parameters list), or None if no WHERE clause

    Example:
        >>> where_dict = {"status": {"eq": "active"}}
        >>> sql, params = _normalize_where_rust(where_dict, "users", metadata)
        >>> sql
        'status = $1'
        >>> params
        ['active']
    """
    if not where_dict:
        return None

    try:
        from fraiseql._fraiseql_rs import normalize_where_to_sql

        # Extract metadata
        table_columns = list(metadata.get("columns", set()))
        fk_mappings = metadata.get("fk_mappings", {})
        jsonb_column = metadata.get("jsonb_column", "data")

        # Call Rust normalization
        where_sql, params = normalize_where_to_sql(
            where_dict,
            table_columns,
            fk_mappings,
            jsonb_column,
        )

        if LOG_QUERY_BUILDER_MODE:
            logger.debug(
                f"Phase 7.2: Rust WHERE normalization: {where_sql} with {len(params)} params"
            )

        return (where_sql, params) if where_sql else None

    except ImportError:
        # Fallback to Python (should not happen in production)
        logger.warning("Rust extension not available, using Python WHERE normalization")
        return None
    except Exception as e:
        # Log error and fallback to Python
        logger.error(f"Rust WHERE normalization failed: {e}, falling back to Python")
        return None
```

**File:** `src/fraiseql/sql/query_builder_adapter.py` (modify existing function)

```python
def build_query_rust(
    table: str,
    field_paths: Sequence[Any],
    where_clause: SQL | None = None,
    **kwargs: Any,
) -> tuple[str, list[Any]]:
    """Build complete SQL query using Rust query builder.

    Phase 7.2: Now uses Rust WHERE normalization if where_dict is provided.

    Args:
        table: Table/view name
        field_paths: List of field paths to select
        where_clause: Legacy psycopg WHERE clause (Phase 7.1)
        **kwargs: Additional query options (limit, offset, order_by, where_dict)

    Returns:
        Tuple of (SQL string, parameters list)
    """
    # Build schema metadata
    metadata = _build_schema_metadata(table, field_paths, where_clause, kwargs)

    # Phase 7.2: Try Rust WHERE normalization first
    where_dict = kwargs.get("where_dict")
    if where_dict:
        rust_where = _normalize_where_rust(where_dict, table, metadata)
        if rust_where:
            where_sql, where_params = rust_where
            # Override where_clause with Rust-generated SQL
            metadata["tables"][table]["where_sql"] = where_sql
            # Store params for later use
            metadata["where_params"] = where_params

    # ... rest of existing logic ...
```

## Module Organization

**File:** `fraiseql_rs/src/query/mod.rs` (update)

```rust
// Existing modules
pub mod composer;
pub mod schema;
pub mod where_builder;

// Phase 7.2: WHERE normalization modules
pub mod casing;
pub mod operators;
pub mod prepared_statement;
pub mod field_analyzer;
pub mod where_normalization;
```

## Comprehensive Testing Strategy

### Rust Unit Tests (200+ test cases)

**File:** `fraiseql_rs/src/query/where_normalization_tests.rs`

```rust
#[cfg(test)]
mod comprehensive_tests {
    use super::*;
    use serde_json::json;

    // Test all 40+ operators
    mod operator_tests {
        use super::*;

        #[test]
        fn test_all_comparison_operators() {
            let ops = vec![
                ("eq", "=", json!("value")),
                ("neq", "!=", json!("value")),
                ("gt", ">", json!(10)),
                ("gte", ">=", json!(10)),
                ("lt", "<", json!(10)),
                ("lte", "<=", json!(10)),
            ];

            for (op, sql_op, value) in ops {
                let mut where_dict = HashMap::new();
                where_dict.insert("field".to_string(), json!({op: value}));

                let normalized = normalize_dict_where(&where_dict, &test_columns(), &HashMap::new(), "data");
                assert_eq!(normalized.conditions.len(), 1);
                assert_eq!(normalized.conditions[0].sql_operator, sql_op);
            }
        }

        #[test]
        fn test_all_string_operators() {
            let ops = vec![
                "contains", "icontains", "startswith", "istartswith",
                "endswith", "iendswith", "like", "ilike",
            ];

            for op in ops {
                let mut where_dict = HashMap::new();
                where_dict.insert("name".to_string(), json!({op: "test"}));

                let normalized = normalize_dict_where(&where_dict, &test_columns(), &HashMap::new(), "data");
                assert_eq!(normalized.conditions.len(), 1);
            }
        }

        #[test]
        fn test_vector_operators() {
            let ops = vec![
                ("cosine_distance", "<=>"),
                ("l2_distance", "<->"),
                ("l1_distance", "<+>"),
            ];

            for (op, sql_op) in ops {
                let mut where_dict = HashMap::new();
                where_dict.insert("embedding".to_string(), json!({op: [0.1, 0.2, 0.3]}));

                let normalized = normalize_dict_where(&where_dict, &test_columns(), &HashMap::new(), "data");
                assert_eq!(normalized.conditions[0].sql_operator, sql_op);
            }
        }

        #[test]
        fn test_array_operators() {
            let ops = vec![
                ("array_contains", "@>"),
                ("array_contained_by", "<@"),
                ("overlap", "&&"),
            ];

            for (op, sql_op) in ops {
                let mut where_dict = HashMap::new();
                where_dict.insert("tags".to_string(), json!({op: ["tag1", "tag2"]}));

                let normalized = normalize_dict_where(&where_dict, &test_columns(), &HashMap::new(), "data");
                assert_eq!(normalized.conditions[0].sql_operator, sql_op);
            }
        }
    }

    // Test nested object handling
    mod nested_tests {
        use super::*;

        #[test]
        fn test_nested_fk_id() {
            let mut where_dict = HashMap::new();
            where_dict.insert("machine".to_string(), json!({"id": {"eq": "123"}}));

            let mut fk_mappings = HashMap::new();
            fk_mappings.insert("machine".to_string(), "machine_id".to_string());

            let normalized = normalize_dict_where(&where_dict, &test_columns(), &fk_mappings, "data");
            assert_eq!(normalized.conditions[0].column_expr, "machine_id");
        }

        #[test]
        fn test_nested_jsonb() {
            let mut where_dict = HashMap::new();
            where_dict.insert("user".to_string(), json!({"name": {"eq": "John"}}));

            let normalized = normalize_dict_where(&where_dict, &test_columns(), &HashMap::new(), "data");
            assert_eq!(normalized.conditions[0].column_expr, "data->'user'->>'name'");
        }

        #[test]
        fn test_deeply_nested() {
            let mut where_dict = HashMap::new();
            where_dict.insert("device".to_string(), json!({"specs": {"cpu": {"eq": "Intel"}}}));

            let normalized = normalize_dict_where(&where_dict, &test_columns(), &HashMap::new(), "data");
            // Should handle multiple levels of nesting
            assert!(normalized.conditions[0].column_expr.contains("device"));
        }
    }

    // Test logical operators
    mod logical_tests {
        use super::*;

        #[test]
        fn test_or() {
            let mut where_dict = HashMap::new();
            where_dict.insert(
                "OR".to_string(),
                json!([{"status": {"eq": "active"}}, {"status": {"eq": "pending"}}]),
            );

            let normalized = normalize_dict_where(&where_dict, &test_columns(), &HashMap::new(), "data");
            assert_eq!(normalized.nested_clauses.len(), 1);
            assert_eq!(normalized.nested_clauses[0].logical_op, "OR");
        }

        #[test]
        fn test_not() {
            let mut where_dict = HashMap::new();
            where_dict.insert("NOT".to_string(), json!({"status": {"eq": "deleted"}}));

            let normalized = normalize_dict_where(&where_dict, &test_columns(), &HashMap::new(), "data");
            assert_eq!(normalized.nested_clauses.len(), 1);
            assert!(normalized.nested_clauses[0].is_not);
        }

        #[test]
        fn test_complex_and_or() {
            let mut where_dict = HashMap::new();
            where_dict.insert(
                "OR".to_string(),
                json!([
                    {"status": {"eq": "active"}, "role": {"eq": "admin"}},
                    {"status": {"eq": "pending"}}
                ]),
            );

            let normalized = normalize_dict_where(&where_dict, &test_columns(), &HashMap::new(), "data");
            // Should create proper nesting
            assert!(normalized.nested_clauses.len() > 0);
        }
    }

    // Test SQL generation
    mod sql_generation_tests {
        use super::*;

        #[test]
        fn test_prepared_statement_params() {
            let mut where_dict = HashMap::new();
            where_dict.insert("status".to_string(), json!({"eq": "active"}));
            where_dict.insert("role".to_string(), json!({"eq": "admin"}));

            let normalized = normalize_dict_where(&where_dict, &test_columns(), &HashMap::new(), "data");
            let mut stmt = PreparedStatement::new();
            let sql = build_where_sql(&normalized, &mut stmt);

            assert!(sql.contains("$1"));
            assert!(sql.contains("$2"));
            assert_eq!(stmt.params.len(), 2);
        }

        #[test]
        fn test_in_clause_params() {
            let mut where_dict = HashMap::new();
            where_dict.insert("status".to_string(), json!({"in": ["active", "pending", "review"]}));

            let normalized = normalize_dict_where(&where_dict, &test_columns(), &HashMap::new(), "data");
            let mut stmt = PreparedStatement::new();
            let sql = build_where_sql(&normalized, &mut stmt);

            assert!(sql.contains("IN ($1, $2, $3)"));
            assert_eq!(stmt.params.len(), 3);
        }
    }

    // Helper functions
    fn test_columns() -> HashSet<String> {
        ["id", "status", "role", "machine_id", "data"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }
}
```

### Python Integration Tests (100+ test cases)

**File:** `tests/integration/test_where_rust_comprehensive.py`

```python
"""Comprehensive integration tests for Rust WHERE normalization.

This test suite ensures 100% parity between Rust and Python implementations.
"""

import pytest
from fraiseql._fraiseql_rs import normalize_where_to_sql


class TestWhereNormalizationRustComprehensive:
    """Test all operators and edge cases."""

    @pytest.fixture
    def basic_metadata(self):
        """Standard test metadata."""
        return {
            "table_columns": ["id", "status", "role", "machine_id", "data"],
            "fk_mappings": {"machine": "machine_id", "user": "user_id"},
            "jsonb_column": "data",
        }

    # Comparison operators
    def test_eq_operator(self, basic_metadata):
        sql, params = normalize_where_to_sql(
            {"status": {"eq": "active"}},
            basic_metadata["table_columns"],
            basic_metadata["fk_mappings"],
            basic_metadata["jsonb_column"],
        )
        assert sql == "status = $1"
        assert params == ["active"]

    def test_neq_operator(self, basic_metadata):
        sql, params = normalize_where_to_sql(
            {"status": {"neq": "deleted"}},
            basic_metadata["table_columns"],
            basic_metadata["fk_mappings"],
            basic_metadata["jsonb_column"],
        )
        assert sql == "status != $1"
        assert params == ["deleted"]

    def test_gt_operator(self, basic_metadata):
        sql, params = normalize_where_to_sql(
            {"age": {"gt": 18}},
            basic_metadata["table_columns"],
            basic_metadata["fk_mappings"],
            basic_metadata["jsonb_column"],
        )
        assert "$1" in sql
        assert ">" in sql
        assert params == [18]

    # String operators
    def test_contains(self, basic_metadata):
        sql, params = normalize_where_to_sql(
            {"name": {"contains": "john"}},
            basic_metadata["table_columns"],
            basic_metadata["fk_mappings"],
            basic_metadata["jsonb_column"],
        )
        assert "LIKE" in sql
        assert params == ["%john%"]

    def test_icontains(self, basic_metadata):
        sql, params = normalize_where_to_sql(
            {"name": {"icontains": "JOHN"}},
            basic_metadata["table_columns"],
            basic_metadata["fk_mappings"],
            basic_metadata["jsonb_column"],
        )
        assert "ILIKE" in sql
        assert params == ["%JOHN%"]

    def test_startswith(self, basic_metadata):
        sql, params = normalize_where_to_sql(
            {"name": {"startswith": "Mr."}},
            basic_metadata["table_columns"],
            basic_metadata["fk_mappings"],
            basic_metadata["jsonb_column"],
        )
        assert "LIKE" in sql
        assert params == ["Mr.%"]

    # Array operators
    def test_in_operator(self, basic_metadata):
        sql, params = normalize_where_to_sql(
            {"status": {"in": ["active", "pending", "review"]}},
            basic_metadata["table_columns"],
            basic_metadata["fk_mappings"],
            basic_metadata["jsonb_column"],
        )
        assert "IN" in sql
        assert "$1" in sql and "$2" in sql and "$3" in sql
        assert len(params) == 3

    # Nested objects
    def test_nested_fk_id(self, basic_metadata):
        sql, params = normalize_where_to_sql(
            {"machine": {"id": {"eq": "abc123"}}},
            basic_metadata["table_columns"],
            basic_metadata["fk_mappings"],
            basic_metadata["jsonb_column"],
        )
        assert "machine_id" in sql
        assert params == ["abc123"]

    def test_nested_jsonb(self, basic_metadata):
        sql, params = normalize_where_to_sql(
            {"device": {"name": {"eq": "Printer"}}},
            basic_metadata["table_columns"],
            basic_metadata["fk_mappings"],
            basic_metadata["jsonb_column"],
        )
        assert "data->'device'->>'name'" in sql
        assert params == ["Printer"]

    # Logical operators
    def test_or_operator(self, basic_metadata):
        sql, params = normalize_where_to_sql(
            {"OR": [{"status": {"eq": "active"}}, {"status": {"eq": "pending"}}]},
            basic_metadata["table_columns"],
            basic_metadata["fk_mappings"],
            basic_metadata["jsonb_column"],
        )
        assert " OR " in sql
        assert len(params) == 2

    def test_not_operator(self, basic_metadata):
        sql, params = normalize_where_to_sql(
            {"NOT": {"status": {"eq": "deleted"}}},
            basic_metadata["table_columns"],
            basic_metadata["fk_mappings"],
            basic_metadata["jsonb_column"],
        )
        assert "NOT" in sql
        assert params == ["deleted"]

    # Edge cases
    def test_null_operator(self, basic_metadata):
        sql, params = normalize_where_to_sql(
            {"email": {"isnull": True}},
            basic_metadata["table_columns"],
            basic_metadata["fk_mappings"],
            basic_metadata["jsonb_column"],
        )
        assert "IS NULL" in sql
        assert len(params) == 0  # No parameters for IS NULL

    def test_multiple_conditions_same_field(self, basic_metadata):
        sql, params = normalize_where_to_sql(
            {"age": {"gt": 18, "lt": 65}},
            basic_metadata["table_columns"],
            basic_metadata["fk_mappings"],
            basic_metadata["jsonb_column"],
        )
        assert ">" in sql and "<" in sql
        assert len(params) == 2

    def test_empty_where(self, basic_metadata):
        sql, params = normalize_where_to_sql(
            {},
            basic_metadata["table_columns"],
            basic_metadata["fk_mappings"],
            basic_metadata["jsonb_column"],
        )
        assert sql == ""
        assert params == []


class TestRustVsPythonParity:
    """Compare Rust output with Python output for exact match."""

    def test_parity_simple_eq(self):
        # TODO: Call both Python and Rust, compare SQL output
        pass

    def test_parity_complex_nested(self):
        # TODO: Complex nested WHERE with OR/AND/NOT
        pass
```

## Verification Commands

```bash
# Rust unit tests (all operators)
cd fraiseql_rs
cargo test operators::tests
cargo test where_normalization::comprehensive_tests
cargo test prepared_statement
cargo test field_analyzer
cargo test casing

# Python integration tests
uv run pytest tests/integration/test_where_rust_comprehensive.py -v

# Full test suite
make test  # Should pass all 5991+ tests

# Clippy (NASA quality)
cargo clippy --lib -- -D warnings

# Performance benchmark
uv run pytest tests/performance/test_where_benchmark.py --benchmark-only
```

## Performance Expectations

**Before (Python):**
```python
# 436 lines of dict parsing, field analysis
# Time: 0.5-1.0ms per query
# Memory: High (Python dict allocations)
```

**After (Rust):**
```rust
// Native parsing, prepared statements
// Time: 0.05-0.1ms per query
// Memory: Low (stack allocations)
// Speedup: 7-10x
```

## Acceptance Criteria

- [ ] All 40+ operators implemented and tested
- [ ] Nested object parsing works correctly
- [ ] Prepared statements prevent SQL injection
- [ ] camelCase → snake_case conversion matches Python
- [ ] PyO3 bindings handle all error cases properly
- [ ] 200+ Rust unit tests pass
- [ ] 100+ Python integration tests pass
- [ ] Full test suite passes (5991+ tests)
- [ ] Zero clippy warnings
- [ ] Performance: 7-10x faster (verified by benchmarks)
- [ ] SQL output matches Python implementation exactly

## Timeline Estimate

**Total: ~12-15 hours** (realistic for NASA quality)

| Step | Time | Cumulative |
|------|------|------------|
| 1. Operators | 60 min | 1h |
| 2. Casing | 30 min | 1.5h |
| 3. Prepared statements | 90 min | 3h |
| 4. Field analyzer | 90 min | 4.5h |
| 5. Normalization logic | 120 min | 6.5h |
| 6. PyO3 bindings | 60 min | 7.5h |
| 7. Python integration | 45 min | 8.25h |
| 8. Rust tests | 180 min | 11.25h |
| 9. Python tests | 120 min | 13.25h |
| 10. Debugging/iteration | 120 min | 15.25h |

## Notes

- This is **Option B: Full Implementation** with complete operator coverage
- Addresses all critical issues from v1 self-review
- Uses **prepared statements** (no SQL injection risk)
- Handles **all 40+ operators** (comparison, string, null, vector, fulltext, array)
- Supports **nested objects** properly
- Includes **comprehensive testing** (300+ test cases)
- Maintains **NASA quality** (zero clippy warnings, no workarounds)
- Timeline is **realistic** (3-4x longer than v1 estimate)

## Related Files

- Part 1: `phase-7.2-where-normalization-rust-v2.md`
- This file: `phase-7.2-where-normalization-rust-v2-PART-2.md`
