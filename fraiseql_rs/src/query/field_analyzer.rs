//! Field type analysis for WHERE clause normalization.
//!
//! This module determines whether a field is a SQL column, JSONB path,
//! or FK column, and handles nested object structures.

use super::casing::to_snake_case;
use super::operators::{get_operator_info, is_operator};
use super::prepared_statement::PreparedStatement;
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// Type of field in the database
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    /// Regular SQL column
    SqlColumn,
    /// Path within JSONB column
    JsonbPath,
    /// Foreign key column
    ForeignKey,
}

/// A single condition in a WHERE clause
#[derive(Debug, Clone)]
pub struct FieldCondition {
    /// The SQL expression for this condition
    pub sql: String,
    /// Whether this is a NOT condition
    pub is_not: bool,
    /// Field type
    pub field_type: FieldType,
}

/// Analyzer for determining field types and building conditions
#[derive(Debug)]
pub struct FieldAnalyzer<'a> {
    /// Set of SQL column names for this table
    table_columns: &'a std::collections::HashSet<String>,
    /// Map of FK field names to their SQL column names (e.g., "machine" -> "machine_id")
    fk_mappings: &'a HashMap<String, String>,
    /// Name of the JSONB column (usually "data")
    jsonb_column: &'a str,
}

impl<'a> FieldAnalyzer<'a> {
    /// Create a new field analyzer.
    ///
    /// # Arguments
    ///
    /// * `table_columns` - Set of SQL column names
    /// * `fk_mappings` - Map of FK field names to column names
    /// * `jsonb_column` - Name of the JSONB column
    #[must_use]
    pub fn new(
        table_columns: &'a std::collections::HashSet<String>,
        fk_mappings: &'a HashMap<String, String>,
        jsonb_column: &'a str,
    ) -> Self {
        Self {
            table_columns,
            fk_mappings,
            jsonb_column,
        }
    }

    /// Analyze a nested field and return conditions.
    ///
    /// This handles both flat and nested formats:
    /// - Flat: `{"status": {"eq": "active"}}` → status = 'active'
    /// - Nested: `{"machine": {"id": {"eq": "123"}}}` → machine_id = '123'
    ///
    /// # Arguments
    ///
    /// * `field_name` - The field name (may be camelCase)
    /// * `field_value` - The value (either operators or nested object)
    /// * `stmt` - The prepared statement builder
    pub fn analyze_nested(
        &self,
        field_name: &str,
        field_value: &JsonValue,
        stmt: &mut PreparedStatement,
    ) -> Vec<FieldCondition> {
        let snake_field = to_snake_case(field_name);

        match field_value {
            JsonValue::Object(inner_map) => {
                // Check if all keys are operators
                let all_operators = inner_map.keys().all(|k| is_operator(k));

                if all_operators {
                    // Flat format: {"status": {"eq": "active"}}
                    self.analyze_flat_field(&snake_field, inner_map, stmt)
                } else {
                    // Nested format: {"machine": {"id": {"eq": "123"}}}
                    self.analyze_nested_object(&snake_field, inner_map, stmt)
                }
            }
            _ => {
                // Unexpected format - treat as flat with implicit "eq"
                vec![]
            }
        }
    }

    /// Analyze a flat field with operators: {"status": {"eq": "active"}}.
    fn analyze_flat_field(
        &self,
        field_name: &str,
        operators: &serde_json::Map<String, JsonValue>,
        stmt: &mut PreparedStatement,
    ) -> Vec<FieldCondition> {
        let mut conditions = Vec::new();

        // Determine field type
        let field_type = self.determine_field_type(field_name);

        for (op_name, op_value) in operators {
            let op_info = match get_operator_info(op_name) {
                Some(info) => info,
                None => continue, // Skip unknown operators
            };

            let sql = match field_type {
                FieldType::SqlColumn => {
                    self.build_sql_column_condition(field_name, op_info, op_value, stmt)
                }
                FieldType::ForeignKey => {
                    // Use the FK column name
                    let fk_col = self.fk_mappings.get(field_name).map_or(field_name, String::as_str);
                    self.build_sql_column_condition(fk_col, op_info, op_value, stmt)
                }
                FieldType::JsonbPath => {
                    self.build_jsonb_condition(field_name, op_info, op_value, stmt)
                }
            };

            if let Some(sql) = sql {
                conditions.push(FieldCondition {
                    sql,
                    is_not: false,
                    field_type,
                });
            }
        }

        conditions
    }

    /// Analyze a nested object: {"machine": {"id": {"eq": "123"}}}.
    fn analyze_nested_object(
        &self,
        parent_field: &str,
        nested_map: &serde_json::Map<String, JsonValue>,
        stmt: &mut PreparedStatement,
    ) -> Vec<FieldCondition> {
        let mut conditions = Vec::new();

        // Check if parent is an FK field
        if let Some(fk_column) = self.fk_mappings.get(parent_field) {
            self.analyze_fk_nested(fk_column, nested_map, stmt, &mut conditions);
        } else {
            self.analyze_jsonb_nested(parent_field, nested_map, stmt, &mut conditions);
        }

        conditions
    }

    /// Analyze FK nested access (helper to reduce nesting).
    fn analyze_fk_nested(
        &self,
        fk_column: &str,
        nested_map: &serde_json::Map<String, JsonValue>,
        stmt: &mut PreparedStatement,
        conditions: &mut Vec<FieldCondition>,
    ) {
        for (nested_field, nested_value) in nested_map {
            // Only handle "id" field - other fields require JOINs (Phase 7.3)
            if nested_field != "id" {
                continue;
            }

            if let JsonValue::Object(operators) = nested_value {
                let flat_conditions = self.analyze_flat_field(fk_column, operators, stmt);
                conditions.extend(flat_conditions);
            }
        }
    }

    /// Analyze JSONB nested path (helper to reduce nesting).
    fn analyze_jsonb_nested(
        &self,
        parent_field: &str,
        nested_map: &serde_json::Map<String, JsonValue>,
        stmt: &mut PreparedStatement,
        conditions: &mut Vec<FieldCondition>,
    ) {
        for (nested_field, nested_value) in nested_map {
            let full_path = format!("{}.{}", parent_field, nested_field);
            let nested_snake = to_snake_case(&full_path);

            let JsonValue::Object(operators) = nested_value else {
                continue;
            };

            let all_operators = operators.keys().all(|k| is_operator(k));
            if !all_operators {
                continue;
            }

            // Build JSONB path conditions
            for (op_name, op_value) in operators {
                let Some(op_info) = get_operator_info(op_name) else {
                    continue;
                };

                if let Some(sql) = self.build_jsonb_nested_condition(
                    &nested_snake,
                    op_info,
                    op_value,
                    stmt,
                ) {
                    conditions.push(FieldCondition {
                        sql,
                        is_not: false,
                        field_type: FieldType::JsonbPath,
                    });
                }
            }
        }
    }

    /// Determine the type of a field.
    fn determine_field_type(&self, field_name: &str) -> FieldType {
        if self.table_columns.contains(field_name) {
            FieldType::SqlColumn
        } else if self.fk_mappings.contains_key(field_name) {
            FieldType::ForeignKey
        } else {
            FieldType::JsonbPath
        }
    }

    /// Build a SQL column condition.
    fn build_sql_column_condition(
        &self,
        column: &str,
        op_info: &super::operators::OperatorInfo,
        value: &JsonValue,
        stmt: &mut PreparedStatement,
    ) -> Option<String> {
        use super::operators::OperatorCategory;

        match op_info.category {
            OperatorCategory::Comparison => {
                if op_info.requires_array {
                    // IN/NOT IN operators
                    if let JsonValue::Array(values) = value {
                        Some(stmt.build_in_clause(column, op_info.sql_op, values))
                    } else {
                        None
                    }
                } else {
                    Some(stmt.build_comparison(column, op_info.sql_op, value.clone()))
                }
            }
            OperatorCategory::String => {
                Some(stmt.build_like(column, op_info.sql_op, value.clone()))
            }
            OperatorCategory::Null => Some(stmt.build_null_check(column, op_info.sql_op)),
            OperatorCategory::Vector => {
                Some(stmt.build_vector_distance(column, op_info.sql_op, value.clone()))
            }
            OperatorCategory::Array => {
                Some(stmt.build_array_operator(column, op_info.sql_op, value.clone()))
            }
            OperatorCategory::Fulltext => {
                Some(stmt.build_fulltext_search(column, op_info.name, value.clone()))
            }
            OperatorCategory::Containment => None, // JSONB only
        }
    }

    /// Build a JSONB condition for a top-level JSONB field.
    fn build_jsonb_condition(
        &self,
        field_path: &str,
        op_info: &super::operators::OperatorInfo,
        value: &JsonValue,
        stmt: &mut PreparedStatement,
    ) -> Option<String> {
        use super::operators::OperatorCategory;

        // Build JSONB path: data->>'field_name'
        let column_expr = stmt.build_jsonb_path(self.jsonb_column, &[field_path], true);

        match op_info.category {
            OperatorCategory::Comparison => {
                if op_info.requires_array {
                    if let JsonValue::Array(values) = value {
                        Some(stmt.build_in_clause(&column_expr, op_info.sql_op, values))
                    } else {
                        None
                    }
                } else {
                    Some(stmt.build_comparison(&column_expr, op_info.sql_op, value.clone()))
                }
            }
            OperatorCategory::String => {
                Some(stmt.build_like(&column_expr, op_info.sql_op, value.clone()))
            }
            OperatorCategory::Null => Some(stmt.build_null_check(&column_expr, op_info.sql_op)),
            OperatorCategory::Containment => {
                // Use JSONB operator on the path: data->'field' @> value
                let json_path = stmt.build_jsonb_path(self.jsonb_column, &[field_path], false);
                Some(stmt.build_jsonb_operator(&json_path, op_info.sql_op, value.clone()))
            }
            _ => None, // Other categories not supported for JSONB
        }
    }

    /// Build a JSONB condition for a nested JSONB path.
    fn build_jsonb_nested_condition(
        &self,
        field_path: &str,
        op_info: &super::operators::OperatorInfo,
        value: &JsonValue,
        stmt: &mut PreparedStatement,
    ) -> Option<String> {
        // Split the path: "device.sensor.value" → ["device", "sensor", "value"]
        let path_segments: Vec<&str> = field_path.split('.').collect();

        // Build JSONB path: data->'device'->'sensor'->>'value'
        let column_expr = stmt.build_jsonb_path(
            self.jsonb_column,
            &path_segments,
            true,
        );

        use super::operators::OperatorCategory;

        match op_info.category {
            OperatorCategory::Comparison => {
                if op_info.requires_array {
                    if let JsonValue::Array(values) = value {
                        Some(stmt.build_in_clause(&column_expr, op_info.sql_op, values))
                    } else {
                        None
                    }
                } else {
                    Some(stmt.build_comparison(&column_expr, op_info.sql_op, value.clone()))
                }
            }
            OperatorCategory::String => {
                Some(stmt.build_like(&column_expr, op_info.sql_op, value.clone()))
            }
            OperatorCategory::Null => Some(stmt.build_null_check(&column_expr, op_info.sql_op)),
            _ => None,
        }
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

        let mut fk_mappings = HashMap::new();
        fk_mappings.insert("machine".to_string(), "machine_id".to_string());

        (columns, fk_mappings)
    }

    #[test]
    fn test_determine_field_type_sql_column() {
        let (columns, fk_mappings) = setup_test_metadata();
        let analyzer = FieldAnalyzer::new(&columns, &fk_mappings, "data");

        assert_eq!(analyzer.determine_field_type("status"), FieldType::SqlColumn);
        assert_eq!(analyzer.determine_field_type("created_at"), FieldType::SqlColumn);
    }

    #[test]
    fn test_determine_field_type_fk() {
        let (columns, fk_mappings) = setup_test_metadata();
        let analyzer = FieldAnalyzer::new(&columns, &fk_mappings, "data");

        assert_eq!(analyzer.determine_field_type("machine"), FieldType::ForeignKey);
    }

    #[test]
    fn test_determine_field_type_jsonb() {
        let (columns, fk_mappings) = setup_test_metadata();
        let analyzer = FieldAnalyzer::new(&columns, &fk_mappings, "data");

        assert_eq!(analyzer.determine_field_type("device_name"), FieldType::JsonbPath);
        assert_eq!(analyzer.determine_field_type("sensor_value"), FieldType::JsonbPath);
    }

    #[test]
    fn test_analyze_flat_field_sql_column() {
        let (columns, fk_mappings) = setup_test_metadata();
        let analyzer = FieldAnalyzer::new(&columns, &fk_mappings, "data");
        let mut stmt = PreparedStatement::new();

        let operators = serde_json::from_value::<serde_json::Map<String, JsonValue>>(
            json!({"eq": "active"})
        ).unwrap();

        let conditions = analyzer.analyze_flat_field("status", &operators, &mut stmt);

        assert_eq!(conditions.len(), 1);
        assert_eq!(conditions[0].sql, "status = $1");
        assert_eq!(conditions[0].field_type, FieldType::SqlColumn);
    }

    #[test]
    fn test_analyze_flat_field_jsonb() {
        let (columns, fk_mappings) = setup_test_metadata();
        let analyzer = FieldAnalyzer::new(&columns, &fk_mappings, "data");
        let mut stmt = PreparedStatement::new();

        let operators = serde_json::from_value::<serde_json::Map<String, JsonValue>>(
            json!({"eq": "sensor1"})
        ).unwrap();

        let conditions = analyzer.analyze_flat_field("device_name", &operators, &mut stmt);

        assert_eq!(conditions.len(), 1);
        assert!(conditions[0].sql.contains("data->>'device_name'"));
        assert_eq!(conditions[0].field_type, FieldType::JsonbPath);
    }

    #[test]
    fn test_analyze_nested_fk_id_access() {
        let (columns, fk_mappings) = setup_test_metadata();
        let analyzer = FieldAnalyzer::new(&columns, &fk_mappings, "data");
        let mut stmt = PreparedStatement::new();

        let nested_value = json!({"id": {"eq": "123"}});

        let conditions = analyzer.analyze_nested("machine", &nested_value, &mut stmt);

        assert_eq!(conditions.len(), 1);
        assert_eq!(conditions[0].sql, "machine_id = $1");
    }

    #[test]
    fn test_analyze_nested_jsonb_path() {
        let (columns, fk_mappings) = setup_test_metadata();
        let analyzer = FieldAnalyzer::new(&columns, &fk_mappings, "data");
        let mut stmt = PreparedStatement::new();

        let nested_value = json!({"sensor": {"value": {"gt": 100}}});

        let conditions = analyzer.analyze_nested("device", &nested_value, &mut stmt);

        assert_eq!(conditions.len(), 1);
        assert!(conditions[0].sql.contains("data->'device'->'sensor'->>'value'"));
    }

    #[test]
    fn test_camel_case_conversion() {
        let (columns, fk_mappings) = setup_test_metadata();
        let analyzer = FieldAnalyzer::new(&columns, &fk_mappings, "data");
        let mut stmt = PreparedStatement::new();

        let operators = serde_json::from_value::<serde_json::Map<String, JsonValue>>(
            json!({"eq": "test"})
        ).unwrap();

        // camelCase should be converted to snake_case
        let conditions = analyzer.analyze_flat_field("deviceName", &operators, &mut stmt);

        assert_eq!(conditions.len(), 1);
        assert!(conditions[0].sql.contains("device_name"));
    }
}
