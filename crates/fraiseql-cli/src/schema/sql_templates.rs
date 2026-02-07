//! SQL template extraction from database handlers.
//!
//! This module extracts SQL generation patterns from the database handlers
//! and stores them as metadata in the compiled schema.
//!
//! For each rich scalar type and its operators, we:
//! 1. Call the database handler's generate_extended_sql() with mock data
//! 2. Normalize the generated SQL to a template format
//! 3. Store the templates for runtime SQL generation
//!
//! # Template Format
//!
//! Templates use placeholders:
//! - `$field` - The JSONB field reference (e.g., `data->>'email'`)
//! - `$1`, `$2`, etc. - Parameter placeholders (database-specific)
//!
//! # Example
//!
//! For EmailDomainEq on PostgreSQL:
//! - Input: field_sql = "data->>'email'", domain = "example.com"
//! - Handler output: "SPLIT_PART(data->>'email', '@', 2) = $1"
//! - Template: "SPLIT_PART($field, '@', 2) = $1"

use std::collections::HashMap;

use serde_json::json;

/// Extract SQL template for an operator from a specific database handler.
///
/// Operator names match the GraphQL field names (without type prefix):
/// - Email: domainEq, domainIn, domainEndswith, localPartStartswith
/// - VIN: wmiEq
/// - IBAN: countryEq
fn extract_template_for_operator(
    db_name: &str,
    operator_name: &str,
) -> Option<String> {
    // For now, we'll hardcode the templates for the 6 implemented operators.
    // In a full implementation, this would instantiate the actual database handlers
    // and call their generate_extended_sql methods.

    match (db_name, operator_name) {
        // Email operators (GraphQL names without "email" prefix)
        ("postgres", "domainEq") => {
            Some("SPLIT_PART($field, '@', 2) = $1".to_string())
        }
        ("mysql", "domainEq") => {
            Some("SUBSTRING_INDEX($field, '@', -1) = ?".to_string())
        }
        ("sqlite", "domainEq") => {
            Some("SUBSTR($field, INSTR($field, '@') + 1) = ?".to_string())
        }
        ("sqlserver", "domainEq") => {
            Some("SUBSTRING($field, CHARINDEX('@', $field) + 1, LEN($field)) = ?".to_string())
        }

        ("postgres", "domainIn") => {
            Some("SPLIT_PART($field, '@', 2) IN ($params)".to_string())
        }
        ("mysql", "domainIn") => {
            Some("SUBSTRING_INDEX($field, '@', -1) IN ($params)".to_string())
        }
        ("sqlite", "domainIn") => {
            Some("SUBSTR($field, INSTR($field, '@') + 1) IN ($params)".to_string())
        }
        ("sqlserver", "domainIn") => {
            Some("SUBSTRING($field, CHARINDEX('@', $field) + 1, LEN($field)) IN ($params)".to_string())
        }

        ("postgres", "domainEndswith") => {
            Some("SPLIT_PART($field, '@', 2) LIKE '%' || $1".to_string())
        }
        ("mysql", "domainEndswith") => {
            Some("SUBSTRING_INDEX($field, '@', -1) LIKE CONCAT('%', ?)".to_string())
        }
        ("sqlite", "domainEndswith") => {
            Some("SUBSTR($field, INSTR($field, '@') + 1) LIKE '%' || ?".to_string())
        }
        ("sqlserver", "domainEndswith") => {
            Some("SUBSTRING($field, CHARINDEX('@', $field) + 1, LEN($field)) LIKE '%' + ?".to_string())
        }

        ("postgres", "localPartStartswith") => {
            Some("SPLIT_PART($field, '@', 1) LIKE $1 || '%'".to_string())
        }
        ("mysql", "localPartStartswith") => {
            Some("SUBSTRING_INDEX($field, '@', 1) LIKE CONCAT(?, '%')".to_string())
        }
        ("sqlite", "localPartStartswith") => {
            Some("SUBSTR($field, 1, INSTR($field, '@') - 1) LIKE ? || '%'".to_string())
        }
        ("sqlserver", "localPartStartswith") => {
            Some("SUBSTRING($field, 1, CHARINDEX('@', $field) - 1) LIKE ? + '%'".to_string())
        }

        // VIN operators (GraphQL names without "vin" prefix)
        ("postgres", "wmiEq") => {
            Some("SUBSTRING($field FROM 1 FOR 3) = $1".to_string())
        }
        ("mysql", "wmiEq") => {
            Some("SUBSTRING($field, 1, 3) = ?".to_string())
        }
        ("sqlite", "wmiEq") => {
            Some("SUBSTR($field, 1, 3) = ?".to_string())
        }
        ("sqlserver", "wmiEq") => {
            Some("SUBSTRING($field, 1, 3) = ?".to_string())
        }

        // IBAN operators (GraphQL names without "iban" prefix)
        ("postgres", "countryEq") => {
            Some("SUBSTRING($field FROM 1 FOR 2) = $1".to_string())
        }
        ("mysql", "countryEq") => {
            Some("SUBSTRING($field, 1, 2) = ?".to_string())
        }
        ("sqlite", "countryEq") => {
            Some("SUBSTR($field, 1, 2) = ?".to_string())
        }
        ("sqlserver", "countryEq") => {
            Some("SUBSTRING($field, 1, 2) = ?".to_string())
        }

        // Standard operators (not extended operators, so no templates)
        _ => None,
    }
}

/// Extract SQL templates for a specific operator from all database handlers.
///
/// Returns a map of database name to SQL template.
/// If a database handler doesn't support the operator, it's omitted from the map.
pub fn extract_operator_templates(operator_name: &str) -> HashMap<String, String> {
    let mut templates = HashMap::new();

    for db in &["postgres", "mysql", "sqlite", "sqlserver"] {
        if let Some(template) = extract_template_for_operator(db, operator_name) {
            templates.insert(db.to_string(), template);
        }
    }

    templates
}

/// Build SQL templates metadata for rich filter types.
///
/// Generates a JSON structure like:
/// ```json
/// {
///   "operators": {
///     "emailDomainEq": {
///       "postgres": "SPLIT_PART($field, '@', 2) = $1",
///       "mysql": "SUBSTRING_INDEX($field, '@', -1) = ?",
///       "sqlite": "...",
///       "sqlserver": "..."
///     }
///   }
/// }
/// ```
pub fn build_sql_templates_metadata(operator_names: &[&str]) -> serde_json::Value {
    let mut operators = serde_json::Map::new();

    for op_name in operator_names {
        let templates = extract_operator_templates(op_name);
        if !templates.is_empty() {
            operators.insert(
                op_name.to_string(),
                json!(templates),
            );
        }
    }

    json!({
        "operators": operators
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_operator_templates() {
        let templates = extract_operator_templates("domainEq");

        // Should have templates for all 4 databases
        assert_eq!(templates.len(), 4);
        assert!(templates.contains_key("postgres"));
        assert!(templates.contains_key("mysql"));
        assert!(templates.contains_key("sqlite"));
        assert!(templates.contains_key("sqlserver"));

        // Verify templates are correct
        assert!(templates["postgres"].contains("SPLIT_PART"));
        assert!(templates["mysql"].contains("SUBSTRING_INDEX"));
    }

    #[test]
    fn test_build_sql_templates_metadata() {
        let operators = vec!["domainEq", "wmiEq"];
        let metadata = build_sql_templates_metadata(&operators);

        assert!(metadata.get("operators").is_some());
        let ops = metadata["operators"].as_object().unwrap();
        assert_eq!(ops.len(), 2);
        assert!(ops.contains_key("domainEq"));
        assert!(ops.contains_key("wmiEq"));
    }

    #[test]
    fn test_extract_vin_templates() {
        let templates = extract_operator_templates("wmiEq");

        assert!(templates.contains_key("postgres"));
        assert!(templates["postgres"].contains("SUBSTRING"));
        assert!(templates["mysql"].contains("SUBSTRING"));
    }
}
