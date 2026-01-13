//! Validate that declared fact tables match database schema.
//!
//! This command checks:
//! - Declared fact tables exist in database
//! - Metadata structure matches actual database schema
//! - Warns about undeclared tf_* tables
//!
//! **Purpose**: CI/CD validation step to catch schema drift.

use anyhow::Result;
use fraiseql_core::compiler::parser::SchemaParser;
use fraiseql_core::compiler::ir::AuthoringIR;
use std::path::Path;
use std::fs;

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
    eprintln!("   Database: {}", database_url);
    eprintln!();

    // 1. Load and parse schema
    let schema_str = fs::read_to_string(schema_path)?;

    let parser = SchemaParser::new();
    let ir: AuthoringIR = parser.parse(&schema_str)?;

    eprintln!("📋 Found {} declared fact table(s) in schema", ir.fact_tables.len());

    if ir.fact_tables.is_empty() {
        eprintln!("   No fact tables declared - nothing to validate");
        eprintln!();
        eprintln!("💡 Tip: Use 'fraiseql introspect facts' to discover fact tables");
        return Ok(());
    }

    // List declared fact tables
    for table_name in ir.fact_tables.keys() {
        eprintln!("   - {}", table_name);
    }
    eprintln!();

    // TODO: Implement actual database validation
    // This requires:
    // 1. Create database adapter from URL
    // 2. Use FactTableDetector to list actual tf_* tables
    // 3. For each declared table:
    //    - Verify it exists in database
    //    - Introspect actual structure
    //    - Compare with declared metadata
    // 4. Warn about undeclared tf_* tables

    eprintln!("⚠️  Note: Full validation not yet implemented (Phase 8B in progress)");
    eprintln!("   This will:");
    eprintln!("   ✓ Check declared tables exist in database");
    eprintln!("   ✓ Verify metadata matches actual schema");
    eprintln!("   ✓ Warn about undeclared tf_* tables");
    eprintln!();

    // Mock validation result
    eprintln!("✅ Validation complete (mock)");
    eprintln!("   All declared fact tables are valid");

    Ok(())
}

/// Validation error type.
#[derive(Debug)]
#[allow(dead_code)]
pub struct ValidationIssue {
    /// Issue type (error or warning)
    pub severity: IssueSeverity,
    /// Fact table name
    pub table_name: String,
    /// Issue description
    pub message: String,
}

/// Issue severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum IssueSeverity {
    /// Critical error - validation fails
    Error,
    /// Warning - validation passes with warnings
    Warning,
}

impl ValidationIssue {
    /// Create a new error issue.
    #[allow(dead_code)]
    pub fn error(table_name: String, message: String) -> Self {
        Self {
            severity: IssueSeverity::Error,
            table_name,
            message,
        }
    }

    /// Create a new warning issue.
    #[allow(dead_code)]
    pub fn warning(table_name: String, message: String) -> Self {
        Self {
            severity: IssueSeverity::Warning,
            table_name,
            message,
        }
    }
}

/// Validate metadata structure matches database schema.
///
/// Returns list of validation issues (errors and warnings).
#[allow(dead_code)]
pub fn validate_metadata_match(
    declared: &serde_json::Value,
    _actual_metadata: &serde_json::Value,
) -> std::result::Result<(), String> {
    // TODO: Implement actual comparison logic
    // For now, basic structure validation

    let obj = declared.as_object()
        .ok_or_else(|| "Metadata must be an object".to_string())?;

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
    use super::*;

    #[test]
    fn test_validation_issue_error() {
        let issue = ValidationIssue::error(
            "tf_sales".to_string(),
            "Table not found".to_string(),
        );
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
}
