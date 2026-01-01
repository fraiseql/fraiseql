//! WHERE clause normalization - main logic.
//!
//! This module provides the main entry point for normalizing GraphQL WHERE
//! clauses into SQL, handling AND/OR/NOT logic, nested objects, and all operators.

use super::field_analyzer::FieldAnalyzer;
use super::prepared_statement::PreparedStatement;
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// Result of WHERE clause normalization
#[derive(Debug, Clone)]
pub struct NormalizedWhere {
    /// SQL WHERE clause (without the "WHERE" keyword)
    pub sql: String,
    /// Parameters for the prepared statement
    pub params: Vec<JsonValue>,
}

impl NormalizedWhere {
    /// Create a new empty normalized WHERE clause.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            sql: String::new(),
            params: Vec::new(),
        }
    }

    /// Check if the normalized WHERE is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.sql.is_empty()
    }
}

impl Default for NormalizedWhere {
    fn default() -> Self {
        Self::new()
    }
}

/// Normalize a GraphQL WHERE clause dictionary into SQL.
///
/// # Arguments
///
/// * `where_dict` - The WHERE clause as a `HashMap`
/// * `table_columns` - Set of SQL column names
/// * `fk_mappings` - Map of FK field names to SQL columns
/// * `jsonb_column` - Name of the JSONB column
///
/// # Returns
///
/// A `NormalizedWhere` with SQL and parameters, or empty if no valid conditions.
///
/// # Example
///
/// ```
/// use fraiseql_rs::query::where_normalization::normalize_dict_where;
/// use std::collections::{HashMap, HashSet};
/// use serde_json::json;
///
/// let mut where_dict = HashMap::new();
/// where_dict.insert("status".to_string(), json!({"eq": "active"}));
///
/// let mut columns = HashSet::new();
/// columns.insert("status".to_string());
///
/// let result = normalize_dict_where(
///     &where_dict,
///     &columns,
///     &HashMap::new(),
///     "data"
/// );
///
/// assert!(!result.is_empty());
/// assert!(result.sql.contains("status = $1"));
/// ```
#[must_use]
#[allow(clippy::implicit_hasher)]
pub fn normalize_dict_where(
    where_dict: &HashMap<String, JsonValue>,
    table_columns: &std::collections::HashSet<String>,
    fk_mappings: &HashMap<String, String>,
    jsonb_column: &str,
) -> NormalizedWhere {
    let analyzer = FieldAnalyzer::new(table_columns, fk_mappings, jsonb_column);
    let mut stmt = PreparedStatement::new();

    let sql_parts = process_where_dict(where_dict, &analyzer, &mut stmt, false);

    if sql_parts.is_empty() {
        return NormalizedWhere::new();
    }

    NormalizedWhere {
        sql: sql_parts.join(" AND "),
        params: stmt.params,
    }
}

/// Process a WHERE dictionary recursively, handling AND/OR/NOT logic.
fn process_where_dict(
    where_dict: &HashMap<String, JsonValue>,
    analyzer: &FieldAnalyzer,
    stmt: &mut PreparedStatement,
    is_not: bool,
) -> Vec<String> {
    let mut sql_parts = Vec::new();

    for (field_name, field_value) in where_dict {
        match field_name.as_str() {
            "OR" => {
                // Handle OR: {"OR": [{"status": {"eq": "a"}}, {"status": {"eq": "b"}}]}
                if let Some(or_sql) = process_or_clause(field_value, analyzer, stmt) {
                    if is_not {
                        sql_parts.push(format!("NOT ({or_sql})"));
                    } else {
                        sql_parts.push(format!("({or_sql})"));
                    }
                }
            }
            "NOT" => {
                // Handle NOT: {"NOT": {"status": {"eq": "deleted"}}}
                if let JsonValue::Object(not_map) = field_value {
                    let not_dict: HashMap<String, JsonValue> = not_map
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    let not_parts = process_where_dict(&not_dict, analyzer, stmt, true);
                    if !not_parts.is_empty() {
                        sql_parts.push(format!("NOT ({})", not_parts.join(" AND ")));
                    }
                }
            }
            _ => {
                // Regular field
                let mut conditions = analyzer.analyze_nested(field_name, field_value, stmt);

                // Apply NOT flag if needed
                if is_not {
                    for cond in &mut conditions {
                        cond.is_not = !cond.is_not;
                    }
                }

                // Add all conditions
                for cond in conditions {
                    if cond.is_not {
                        sql_parts.push(format!("NOT ({})", cond.sql));
                    } else {
                        sql_parts.push(cond.sql);
                    }
                }
            }
        }
    }

    sql_parts
}

/// Process an OR clause: {"OR": [cond1, cond2, ...]}.
fn process_or_clause(
    or_value: &JsonValue,
    analyzer: &FieldAnalyzer,
    stmt: &mut PreparedStatement,
) -> Option<String> {
    let JsonValue::Array(or_array) = or_value else {
        return None;
    };

    let mut or_parts = Vec::new();

    for item in or_array {
        if let JsonValue::Object(item_map) = item {
            let item_dict: HashMap<String, JsonValue> = item_map
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            let item_sql = process_where_dict(&item_dict, analyzer, stmt, false);
            if !item_sql.is_empty() {
                if item_sql.len() == 1 {
                    or_parts.push(item_sql[0].clone());
                } else {
                    or_parts.push(format!("({})", item_sql.join(" AND ")));
                }
            }
        }
    }

    if or_parts.is_empty() {
        None
    } else {
        Some(or_parts.join(" OR "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashSet;

    fn setup_test_metadata() -> (HashSet<String>, HashMap<String, String>) {
        let mut columns = HashSet::new();
        columns.insert("id".to_string());
        columns.insert("status".to_string());
        columns.insert("created_at".to_string());
        columns.insert("machine_id".to_string());
        columns.insert("age".to_string());

        let mut fk_mappings = HashMap::new();
        fk_mappings.insert("machine".to_string(), "machine_id".to_string());

        (columns, fk_mappings)
    }

    #[test]
    fn test_simple_eq_sql_column() {
        let (columns, fk_mappings) = setup_test_metadata();

        let mut where_dict = HashMap::new();
        where_dict.insert("status".to_string(), json!({"eq": "active"}));

        let result = normalize_dict_where(&where_dict, &columns, &fk_mappings, "data");

        assert!(!result.is_empty());
        assert_eq!(result.sql, "status = $1");
        assert_eq!(result.params.len(), 1);
        assert_eq!(result.params[0], json!("active"));
    }

    #[test]
    fn test_simple_eq_jsonb_field() {
        let (columns, fk_mappings) = setup_test_metadata();

        let mut where_dict = HashMap::new();
        where_dict.insert("deviceName".to_string(), json!({"eq": "sensor1"}));

        let result = normalize_dict_where(&where_dict, &columns, &fk_mappings, "data");

        assert!(!result.is_empty());
        assert!(result.sql.contains("data->>'device_name'"));
        assert_eq!(result.params[0], json!("sensor1"));
    }

    #[test]
    fn test_multiple_conditions_and() {
        let (columns, fk_mappings) = setup_test_metadata();

        let mut where_dict = HashMap::new();
        where_dict.insert("status".to_string(), json!({"eq": "active"}));
        where_dict.insert("age".to_string(), json!({"gt": 18}));

        let result = normalize_dict_where(&where_dict, &columns, &fk_mappings, "data");

        assert!(!result.is_empty());
        // Both conditions should be AND-ed together
        assert!(result.sql.contains("status = $1"));
        assert!(result.sql.contains("age > $2"));
        assert!(result.sql.contains(" AND "));
        assert_eq!(result.params.len(), 2);
    }

    #[test]
    fn test_or_clause() {
        let (columns, fk_mappings) = setup_test_metadata();

        let mut where_dict = HashMap::new();
        where_dict.insert(
            "OR".to_string(),
            json!([
                {"status": {"eq": "active"}},
                {"status": {"eq": "pending"}}
            ]),
        );

        let result = normalize_dict_where(&where_dict, &columns, &fk_mappings, "data");

        assert!(!result.is_empty());
        assert!(result.sql.contains("status = $1"));
        assert!(result.sql.contains("status = $2"));
        assert!(result.sql.contains(" OR "));
        assert_eq!(result.params.len(), 2);
    }

    #[test]
    fn test_not_clause() {
        let (columns, fk_mappings) = setup_test_metadata();

        let mut where_dict = HashMap::new();
        where_dict.insert("NOT".to_string(), json!({"status": {"eq": "deleted"}}));

        let result = normalize_dict_where(&where_dict, &columns, &fk_mappings, "data");

        assert!(!result.is_empty());
        assert!(result.sql.contains("NOT ("));
        assert!(result.sql.contains("status = $1"));
        assert_eq!(result.params[0], json!("deleted"));
    }

    #[test]
    fn test_nested_fk_access() {
        let (columns, fk_mappings) = setup_test_metadata();

        let mut where_dict = HashMap::new();
        where_dict.insert("machine".to_string(), json!({"id": {"eq": "machine-123"}}));

        let result = normalize_dict_where(&where_dict, &columns, &fk_mappings, "data");

        assert!(!result.is_empty());
        assert!(result.sql.contains("machine_id = $1"));
        assert_eq!(result.params[0], json!("machine-123"));
    }

    #[test]
    fn test_nested_jsonb_path() {
        let (columns, fk_mappings) = setup_test_metadata();

        let mut where_dict = HashMap::new();
        where_dict.insert(
            "device".to_string(),
            json!({"sensor": {"value": {"gt": 100}}}),
        );

        let result = normalize_dict_where(&where_dict, &columns, &fk_mappings, "data");

        assert!(!result.is_empty());
        assert!(result.sql.contains("data->'device'->'sensor'->>'value'"));
        assert!(result.sql.contains("> $1"));
        assert_eq!(result.params[0], json!(100));
    }

    #[test]
    fn test_in_operator() {
        let (columns, fk_mappings) = setup_test_metadata();

        let mut where_dict = HashMap::new();
        where_dict.insert(
            "status".to_string(),
            json!({"in": ["active", "pending", "review"]}),
        );

        let result = normalize_dict_where(&where_dict, &columns, &fk_mappings, "data");

        assert!(!result.is_empty());
        assert!(result.sql.contains("status IN ($1, $2, $3)"));
        assert_eq!(result.params.len(), 3);
    }

    #[test]
    fn test_like_operator() {
        let (columns, fk_mappings) = setup_test_metadata();

        let mut where_dict = HashMap::new();
        where_dict.insert("status".to_string(), json!({"like": "%active%"}));

        let result = normalize_dict_where(&where_dict, &columns, &fk_mappings, "data");

        assert!(!result.is_empty());
        assert!(result.sql.contains("status LIKE $1"));
        assert_eq!(result.params[0], json!("%active%"));
    }

    #[test]
    fn test_null_check() {
        let (columns, fk_mappings) = setup_test_metadata();

        let mut where_dict = HashMap::new();
        where_dict.insert("created_at".to_string(), json!({"is_not_null": true}));

        let result = normalize_dict_where(&where_dict, &columns, &fk_mappings, "data");

        assert!(!result.is_empty());
        assert!(result.sql.contains("created_at IS NOT NULL"));
        assert_eq!(result.params.len(), 0); // NULL checks don't use params
    }

    #[test]
    fn test_complex_and_or_not() {
        let (columns, fk_mappings) = setup_test_metadata();

        let mut where_dict = HashMap::new();
        where_dict.insert("status".to_string(), json!({"eq": "active"}));
        where_dict.insert(
            "OR".to_string(),
            json!([
                {"age": {"gt": 18}},
                {"age": {"eq": null}}
            ]),
        );
        where_dict.insert("NOT".to_string(), json!({"status": {"eq": "deleted"}}));

        let result = normalize_dict_where(&where_dict, &columns, &fk_mappings, "data");

        assert!(!result.is_empty());
        assert!(result.sql.contains("status = $1"));
        assert!(result.sql.contains(" OR "));
        assert!(result.sql.contains("NOT ("));
    }

    #[test]
    fn test_empty_where() {
        let (columns, fk_mappings) = setup_test_metadata();
        let where_dict = HashMap::new();

        let result = normalize_dict_where(&where_dict, &columns, &fk_mappings, "data");

        assert!(result.is_empty());
        assert_eq!(result.sql, "");
        assert_eq!(result.params.len(), 0);
    }

    #[test]
    fn test_camel_case_conversion() {
        let (columns, fk_mappings) = setup_test_metadata();

        let mut where_dict = HashMap::new();
        where_dict.insert("createdAt".to_string(), json!({"is_not_null": true}));

        let result = normalize_dict_where(&where_dict, &columns, &fk_mappings, "data");

        assert!(!result.is_empty());
        assert!(result.sql.contains("created_at"));
    }
}
