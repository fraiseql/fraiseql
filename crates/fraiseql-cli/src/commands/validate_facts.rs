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
                // Compare structures against declared metadata
                if let Some(declared) = ir.fact_tables.get(table_name) {
                    let comparison_issues =
                        compare_metadata(table_name, declared, &actual_metadata);
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
    declared: &FactTableMetadata,
    actual: &FactTableMetadata,
) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    let declared_measure_names: HashSet<&str> =
        declared.measures.iter().map(|m| m.name.as_str()).collect();
    let actual_measure_names: HashSet<&str> =
        actual.measures.iter().map(|m| m.name.as_str()).collect();

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
    for declared_measure in &declared.measures {
        if let Some(actual_measure) =
            actual.measures.iter().find(|m| m.name == declared_measure.name)
        {
            let declared_type = format!("{:?}", declared_measure.sql_type);
            let actual_type = format!("{:?}", actual_measure.sql_type);
            if declared_type != actual_type {
                issues.push(ValidationIssue::warning(
                    table_name.to_string(),
                    format!(
                        "Measure '{}' type mismatch: declared '{declared_type}', actual \
                         '{actual_type}'",
                        declared_measure.name
                    ),
                ));
            }
        }
    }

    // Validate dimensions column
    if declared.dimensions.name != actual.dimensions.name {
        issues.push(ValidationIssue::error(
            table_name.to_string(),
            format!(
                "Dimensions column mismatch: declared '{}', actual '{}'",
                declared.dimensions.name, actual.dimensions.name
            ),
        ));
    }

    // Validate denormalized filters
    let declared_filter_names: HashSet<&str> =
        declared.denormalized_filters.iter().map(|f| f.name.as_str()).collect();
    let actual_filter_names: HashSet<&str> =
        actual.denormalized_filters.iter().map(|f| f.name.as_str()).collect();

    for name in &declared_filter_names {
        if !actual_filter_names.contains(name) {
            issues.push(ValidationIssue::warning(
                table_name.to_string(),
                format!("Declared filter '{name}' not found in database"),
            ));
        }
    }

    issues
}


#[cfg(test)]
mod tests {
    use fraiseql_core::compiler::fact_table::{
        DimensionColumn, FactTableMetadata, FilterColumn, MeasureColumn, SqlType,
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

    fn make_metadata(
        measures: Vec<MeasureColumn>,
        dim_name: &str,
        filters: Vec<FilterColumn>,
    ) -> FactTableMetadata {
        FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures,
            dimensions:           DimensionColumn { name: dim_name.to_string(), paths: vec![] },
            denormalized_filters: filters,
            calendar_dimensions:  vec![],
        }
    }

    #[test]
    fn test_compare_metadata_matching() {
        let declared = make_metadata(
            vec![
                MeasureColumn { name: "revenue".to_string(), sql_type: SqlType::Decimal, nullable: false },
                MeasureColumn { name: "quantity".to_string(), sql_type: SqlType::Int, nullable: false },
            ],
            "data",
            vec![FilterColumn { name: "customer_id".to_string(), sql_type: SqlType::Uuid, indexed: true }],
        );
        let actual = declared.clone();

        let issues = compare_metadata("tf_sales", &declared, &actual);
        let errors: Vec<_> = issues.iter().filter(|i| i.severity == IssueSeverity::Error).collect();
        assert!(errors.is_empty(), "Unexpected errors: {errors:?}");
    }

    #[test]
    fn test_compare_metadata_missing_measure() {
        let declared = make_metadata(
            vec![
                MeasureColumn { name: "revenue".to_string(), sql_type: SqlType::Decimal, nullable: false },
                MeasureColumn { name: "profit".to_string(), sql_type: SqlType::Decimal, nullable: false },
            ],
            "data",
            vec![],
        );
        let actual = make_metadata(
            vec![MeasureColumn { name: "revenue".to_string(), sql_type: SqlType::Decimal, nullable: false }],
            "data",
            vec![],
        );

        let issues = compare_metadata("tf_sales", &declared, &actual);
        let errors: Vec<_> = issues.iter().filter(|i| i.severity == IssueSeverity::Error).collect();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("profit"));
    }
}
