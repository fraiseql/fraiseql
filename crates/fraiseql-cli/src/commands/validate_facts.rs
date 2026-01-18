//! Validate that declared fact tables match database schema.
//!
//! This command checks:
//! - Declared fact tables exist in database
//! - Metadata structure matches actual database schema
//! - Warns about undeclared tf_* tables
//!
//! **Purpose**: CI/CD validation step to catch schema drift.

use std::{collections::HashSet, fs, path::Path};

use anyhow::Result;
use deadpool_postgres::{Config, ManagerConfig, RecyclingMethod, Runtime};
use fraiseql_core::{
    compiler::{
        fact_table::{DatabaseIntrospector, FactTableDetector, FactTableMetadata},
        ir::AuthoringIR,
        parser::SchemaParser,
    },
    db::PostgresIntrospector,
};
use tokio_postgres::NoTls;

/// Validation error type.
#[derive(Debug)]
pub struct ValidationIssue {
    /// Issue type (error or warning)
    pub severity:   IssueSeverity,
    /// Fact table name
    pub table_name: String,
    /// Issue description
    pub message:    String,
}

/// Issue severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueSeverity {
    /// Critical error - validation fails
    Error,
    /// Warning - validation passes with warnings
    Warning,
}

impl ValidationIssue {
    /// Create a new error issue.
    pub const fn error(table_name: String, message: String) -> Self {
        Self {
            severity: IssueSeverity::Error,
            table_name,
            message,
        }
    }

    /// Create a new warning issue.
    pub const fn warning(table_name: String, message: String) -> Self {
        Self {
            severity: IssueSeverity::Warning,
            table_name,
            message,
        }
    }
}

/// Create a PostgreSQL introspector from a database URL
async fn create_introspector(database_url: &str) -> Result<PostgresIntrospector> {
    let mut cfg = Config::new();
    cfg.url = Some(database_url.to_string());
    cfg.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    });
    cfg.pool = Some(deadpool_postgres::PoolConfig::new(2));

    let pool = cfg
        .create_pool(Some(Runtime::Tokio1), NoTls)
        .map_err(|e| anyhow::anyhow!("Failed to create database pool: {e}"))?;

    // Test connection
    let _client = pool
        .get()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to database: {e}"))?;

    Ok(PostgresIntrospector::new(pool))
}

/// Validate that declared fact tables match database schema.
///
/// # Arguments
///
/// * `schema_path` - Path to schema.json file
/// * `database_url` - Database connection string
///
/// # Returns
///
/// Success if all validations pass, error otherwise
///
/// # Example
///
/// ```bash
/// fraiseql validate facts --schema schema.json --database postgresql://localhost/mydb
/// ```
pub async fn run(schema_path: &Path, database_url: &str) -> Result<()> {
    eprintln!("🔍 Validating fact tables...");
    eprintln!("   Schema: {}", schema_path.display());
    eprintln!("   Database: {database_url}");
    eprintln!();

    // 1. Load and parse schema
    let schema_str = fs::read_to_string(schema_path)?;

    let parser = SchemaParser::new();
    let ir: AuthoringIR = parser.parse(&schema_str)?;

    let declared_tables: HashSet<String> = ir.fact_tables.keys().cloned().collect();

    eprintln!("📋 Found {} declared fact table(s) in schema", declared_tables.len());

    if declared_tables.is_empty() {
        eprintln!("   No fact tables declared - nothing to validate");
        eprintln!();
        eprintln!("💡 Tip: Use 'fraiseql introspect facts' to discover fact tables");
        return Ok(());
    }

    for table_name in &declared_tables {
        eprintln!("   - {table_name}");
    }
    eprintln!();

    // 2. Connect to database and list actual fact tables
    let introspector = create_introspector(database_url).await?;

    let actual_tables: HashSet<String> = introspector
        .list_fact_tables()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to list fact tables: {e}"))?
        .into_iter()
        .collect();

    eprintln!("📊 Found {} fact table(s) in database", actual_tables.len());
    eprintln!();

    // 3. Validate each declared table
    let mut issues: Vec<ValidationIssue> = Vec::new();
    let mut validated_count = 0;

    for table_name in &declared_tables {
        eprintln!("   Validating {table_name}...");

        // Check if table exists in database
        if !actual_tables.contains(table_name) {
            issues.push(ValidationIssue::error(
                table_name.clone(),
                "Table does not exist in database".to_string(),
            ));
            continue;
        }

        // Introspect actual table structure
        match FactTableDetector::introspect(&introspector, table_name).await {
            Ok(actual_metadata) => {
                // Get declared metadata
                if let Some(declared_json) = ir.fact_tables.get(table_name) {
                    // Compare structures
                    let comparison_issues =
                        compare_metadata(table_name, declared_json, &actual_metadata);
                    issues.extend(comparison_issues);
                }
                validated_count += 1;
            },
            Err(e) => {
                issues.push(ValidationIssue::error(
                    table_name.clone(),
                    format!("Failed to introspect: {e}"),
                ));
            },
        }
    }

    // 4. Check for undeclared tables in database
    for table_name in &actual_tables {
        if !declared_tables.contains(table_name) {
            issues.push(ValidationIssue::warning(
                table_name.clone(),
                "Table exists in database but not declared in schema".to_string(),
            ));
        }
    }

    // 5. Report results
    eprintln!();
    let errors: Vec<&ValidationIssue> =
        issues.iter().filter(|i| i.severity == IssueSeverity::Error).collect();
    let warnings: Vec<&ValidationIssue> =
        issues.iter().filter(|i| i.severity == IssueSeverity::Warning).collect();

    if !errors.is_empty() {
        eprintln!("❌ Errors ({}):", errors.len());
        for issue in &errors {
            eprintln!("   {} - {}", issue.table_name, issue.message);
        }
        eprintln!();
    }

    if !warnings.is_empty() {
        eprintln!("⚠️  Warnings ({}):", warnings.len());
        for issue in &warnings {
            eprintln!("   {} - {}", issue.table_name, issue.message);
        }
        eprintln!();
    }

    if errors.is_empty() {
        eprintln!("✅ Validation passed");
        eprintln!("   {validated_count} table(s) validated successfully");
        if !warnings.is_empty() {
            eprintln!("   {} warning(s)", warnings.len());
        }
        Ok(())
    } else {
        Err(anyhow::anyhow!("Validation failed with {} error(s)", errors.len()))
    }
}

/// Compare declared metadata with actual database metadata
fn compare_metadata(
    table_name: &str,
    declared: &serde_json::Value,
    actual: &FactTableMetadata,
) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    // Extract declared measures
    if let Some(declared_measures) = declared.get("measures").and_then(|m| m.as_array()) {
        let declared_measure_names: HashSet<String> = declared_measures
            .iter()
            .filter_map(|m| m.get("name").and_then(|n| n.as_str()))
            .map(String::from)
            .collect();

        let actual_measure_names: HashSet<String> =
            actual.measures.iter().map(|m| m.name.clone()).collect();

        // Check for missing measures in actual
        for name in &declared_measure_names {
            if !actual_measure_names.contains(name) {
                issues.push(ValidationIssue::error(
                    table_name.to_string(),
                    format!("Declared measure '{name}' not found in database"),
                ));
            }
        }

        // Check for extra measures in actual (warning)
        for name in &actual_measure_names {
            if !declared_measure_names.contains(name) {
                issues.push(ValidationIssue::warning(
                    table_name.to_string(),
                    format!("Database has measure '{name}' not declared in schema"),
                ));
            }
        }

        // Validate measure types
        for declared_measure in declared_measures {
            if let (Some(name), Some(declared_type)) = (
                declared_measure.get("name").and_then(|n| n.as_str()),
                declared_measure.get("sql_type").and_then(|t| t.as_str()),
            ) {
                if let Some(actual_measure) = actual.measures.iter().find(|m| m.name == name) {
                    let actual_type_str = format!("{:?}", actual_measure.sql_type);
                    if !types_compatible(declared_type, &actual_type_str) {
                        issues.push(ValidationIssue::warning(
                            table_name.to_string(),
                            format!(
                                "Measure '{name}' type mismatch: declared '{declared_type}', actual '{actual_type_str}'"
                            ),
                        ));
                    }
                }
            }
        }
    }

    // Validate dimensions column
    if let Some(declared_dims) = declared.get("dimensions") {
        if let Some(declared_name) = declared_dims.get("name").and_then(|n| n.as_str()) {
            if declared_name != actual.dimensions.name {
                issues.push(ValidationIssue::error(
                    table_name.to_string(),
                    format!(
                        "Dimensions column mismatch: declared '{}', actual '{}'",
                        declared_name, actual.dimensions.name
                    ),
                ));
            }
        }
    }

    // Validate denormalized filters
    if let Some(declared_filters) = declared.get("denormalized_filters").and_then(|f| f.as_array())
    {
        let declared_filter_names: HashSet<String> = declared_filters
            .iter()
            .filter_map(|f| f.get("name").and_then(|n| n.as_str()))
            .map(String::from)
            .collect();

        let actual_filter_names: HashSet<String> =
            actual.denormalized_filters.iter().map(|f| f.name.clone()).collect();

        for name in &declared_filter_names {
            if !actual_filter_names.contains(name) {
                issues.push(ValidationIssue::warning(
                    table_name.to_string(),
                    format!("Declared filter '{name}' not found in database"),
                ));
            }
        }
    }

    issues
}

/// Check if two SQL types are compatible
fn types_compatible(declared: &str, actual: &str) -> bool {
    let declared_lower = declared.to_lowercase();
    let actual_lower = actual.to_lowercase();

    // Exact match
    if declared_lower == actual_lower {
        return true;
    }

    // Common aliases
    let aliases: &[(&[&str], &[&str])] = &[
        (&["int", "integer", "int4"], &["int", "integer", "int4"]),
        (&["bigint", "int8"], &["bigint", "int8"]),
        (&["decimal", "numeric", "money"], &["decimal", "numeric", "money"]),
        (&["float", "double", "real", "float8"], &["float", "double", "real", "float8"]),
        (&["text", "varchar", "string"], &["text", "varchar", "string"]),
        (&["uuid"], &["uuid"]),
        (
            &["timestamp", "timestamptz", "datetime"],
            &["timestamp", "timestamptz", "datetime"],
        ),
        (&["json", "jsonb"], &["json", "jsonb"]),
        (&["bool", "boolean"], &["bool", "boolean"]),
    ];

    for (group1, group2) in aliases {
        let in_group1 = group1.iter().any(|t| declared_lower.contains(t));
        let in_group2 = group2.iter().any(|t| actual_lower.contains(t));
        if in_group1 && in_group2 {
            return true;
        }
    }

    false
}

/// Validate metadata structure (basic validation)
#[allow(dead_code)]
pub fn validate_metadata_match(
    declared: &serde_json::Value,
    _actual_metadata: &serde_json::Value,
) -> std::result::Result<(), String> {
    let obj = declared.as_object().ok_or_else(|| "Metadata must be an object".to_string())?;

    // Check required fields exist
    if !obj.contains_key("measures") {
        return Err("Missing 'measures' field".to_string());
    }

    if !obj.contains_key("dimensions") {
        return Err("Missing 'dimensions' field".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use fraiseql_core::compiler::fact_table::{
        DimensionColumn, FilterColumn, MeasureColumn, SqlType,
    };

    use super::*;

    #[test]
    fn test_validation_issue_error() {
        let issue = ValidationIssue::error("tf_sales".to_string(), "Table not found".to_string());
        assert_eq!(issue.severity, IssueSeverity::Error);
        assert_eq!(issue.table_name, "tf_sales");
    }

    #[test]
    fn test_validation_issue_warning() {
        let issue = ValidationIssue::warning(
            "tf_orders".to_string(),
            "Table exists but not declared".to_string(),
        );
        assert_eq!(issue.severity, IssueSeverity::Warning);
    }

    #[test]
    fn test_validate_metadata_match() {
        let metadata = serde_json::json!({
            "measures": [],
            "dimensions": {"name": "data"}
        });

        let result = validate_metadata_match(&metadata, &metadata);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_metadata_match_missing_measures() {
        let metadata = serde_json::json!({
            "dimensions": {"name": "data"}
        });

        let result = validate_metadata_match(&metadata, &metadata);
        assert!(result.is_err());
    }

    #[test]
    fn test_types_compatible() {
        // Exact match
        assert!(types_compatible("Int", "Int"));
        assert!(types_compatible("Decimal", "Decimal"));

        // Aliases
        assert!(types_compatible("integer", "Int"));
        assert!(types_compatible("int4", "Int"));
        assert!(types_compatible("bigint", "BigInt"));
        assert!(types_compatible("numeric", "Decimal"));
        assert!(types_compatible("float", "Float"));
        assert!(types_compatible("double", "Float"));
        assert!(types_compatible("text", "Text"));
        assert!(types_compatible("varchar", "Text"));

        // Incompatible
        assert!(!types_compatible("Int", "Text"));
        assert!(!types_compatible("Decimal", "Boolean"));
    }

    #[test]
    fn test_compare_metadata_matching() {
        let declared = serde_json::json!({
            "measures": [
                {"name": "revenue", "sql_type": "Decimal"},
                {"name": "quantity", "sql_type": "Int"}
            ],
            "dimensions": {"name": "data"},
            "denormalized_filters": [
                {"name": "customer_id"}
            ]
        });

        let actual = FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![
                MeasureColumn {
                    name:     "revenue".to_string(),
                    sql_type: SqlType::Decimal,
                    nullable: false,
                },
                MeasureColumn {
                    name:     "quantity".to_string(),
                    sql_type: SqlType::Int,
                    nullable: false,
                },
            ],
            dimensions:           DimensionColumn {
                name:  "data".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![FilterColumn {
                name:     "customer_id".to_string(),
                sql_type: SqlType::Uuid,
                indexed:  true,
            }],
            calendar_dimensions:  vec![],
        };

        let issues = compare_metadata("tf_sales", &declared, &actual);

        // No errors expected for matching metadata
        let errors: Vec<_> = issues.iter().filter(|i| i.severity == IssueSeverity::Error).collect();
        assert!(errors.is_empty(), "Unexpected errors: {:?}", errors);
    }

    #[test]
    fn test_compare_metadata_missing_measure() {
        let declared = serde_json::json!({
            "measures": [
                {"name": "revenue", "sql_type": "Decimal"},
                {"name": "profit", "sql_type": "Decimal"}  // Not in actual
            ],
            "dimensions": {"name": "data"}
        });

        let actual = FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![MeasureColumn {
                name:     "revenue".to_string(),
                sql_type: SqlType::Decimal,
                nullable: false,
            }],
            dimensions:           DimensionColumn {
                name:  "data".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![],
            calendar_dimensions:  vec![],
        };

        let issues = compare_metadata("tf_sales", &declared, &actual);

        // Should have error for missing 'profit' measure
        let errors: Vec<_> = issues.iter().filter(|i| i.severity == IssueSeverity::Error).collect();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("profit"));
    }
}
