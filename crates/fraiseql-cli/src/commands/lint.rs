//! Lint command - Design quality analysis for schemas
//!
//! Usage: fraiseql lint schema.json [--federation] [--cost] [--cache] [--auth] [--compilation]
//!        fraiseql lint schema.json --format=json
//!        fraiseql lint schema.json --fail-on-critical
//!        fraiseql lint schema.json --verbose --fail-on-warning

use anyhow::Result;
use serde::Serialize;
use std::fs;
use std::path::Path;

use fraiseql_core::design::DesignAudit;
use crate::output::CommandResult;

/// Lint command options
#[derive(Debug, Clone)]
pub struct LintOptions {
    /// Only show federation audit
    pub federation: bool,
    /// Only show cost audit
    pub cost: bool,
    /// Only show cache audit
    pub cache: bool,
    /// Only show auth audit
    pub auth: bool,
    /// Only show compilation audit
    pub compilation: bool,
    /// Exit with error if any critical issues found
    pub fail_on_critical: bool,
    /// Exit with error if any warning or critical issues found
    pub fail_on_warning: bool,
    /// Show detailed issue descriptions
    pub verbose: bool,
}

/// Lint output response
#[derive(Debug, Serialize)]
pub struct LintResponse {
    /// Overall design score (0-100)
    pub overall_score: u8,
    /// Severity counts
    pub severity_counts: SeverityCounts,
    /// Category scores
    pub categories: CategoryScores,
}

/// Severity counts in audit
#[derive(Debug, Serialize)]
pub struct SeverityCounts {
    /// Critical issues
    pub critical: usize,
    /// Warning issues
    pub warning: usize,
    /// Info issues
    pub info: usize,
}

/// Category scores
#[derive(Debug, Serialize)]
pub struct CategoryScores {
    /// Federation audit score
    pub federation: u8,
    /// Cost audit score
    pub cost: u8,
    /// Cache audit score
    pub cache: u8,
    /// Authorization audit score
    pub authorization: u8,
    /// Compilation audit score
    pub compilation: u8,
}

/// Run lint command on a schema
pub fn run(schema_path: &str, opts: LintOptions) -> Result<CommandResult> {
    // Check if file exists
    if !Path::new(schema_path).exists() {
        return Ok(CommandResult::error(
            "lint",
            &format!("Schema file not found: {schema_path}"),
            "FILE_NOT_FOUND",
        ));
    }

    // Read schema file
    let schema_json = fs::read_to_string(schema_path)?;

    // Parse as JSON to validate it
    let _schema: serde_json::Value = serde_json::from_str(&schema_json)?;

    // Run design audit
    let audit = DesignAudit::from_schema_json(&schema_json)?;

    // Check for fail conditions if enabled
    if opts.fail_on_critical && audit.severity_count(fraiseql_core::design::IssueSeverity::Critical) > 0 {
        return Ok(CommandResult::error(
            "lint",
            "Design audit failed: critical issues found",
            "DESIGN_AUDIT_FAILED",
        ));
    }

    if opts.fail_on_warning && audit.severity_count(fraiseql_core::design::IssueSeverity::Warning) > 0 {
        return Ok(CommandResult::error(
            "lint",
            "Design audit failed: warning issues found",
            "DESIGN_AUDIT_FAILED",
        ));
    }

    // Calculate category scores
    let fed_score = if audit.federation_issues.is_empty() {
        100
    } else {
        let count = u32::try_from(audit.federation_issues.len()).unwrap_or(u32::MAX);
        (100u32 - (count * 10)).clamp(0, 100) as u8
    };

    let cost_score = if audit.cost_warnings.is_empty() {
        100
    } else {
        let count = u32::try_from(audit.cost_warnings.len()).unwrap_or(u32::MAX);
        (100u32 - (count * 8)).clamp(0, 100) as u8
    };

    let cache_score = if audit.cache_issues.is_empty() {
        100
    } else {
        let count = u32::try_from(audit.cache_issues.len()).unwrap_or(u32::MAX);
        (100u32 - (count * 6)).clamp(0, 100) as u8
    };

    let auth_score = if audit.auth_issues.is_empty() {
        100
    } else {
        let count = u32::try_from(audit.auth_issues.len()).unwrap_or(u32::MAX);
        (100u32 - (count * 12)).clamp(0, 100) as u8
    };

    let comp_score = if audit.schema_issues.is_empty() {
        100
    } else {
        let count = u32::try_from(audit.schema_issues.len()).unwrap_or(u32::MAX);
        (100u32 - (count * 10)).clamp(0, 100) as u8
    };

    let severity_counts = SeverityCounts {
        critical: audit.severity_count(fraiseql_core::design::IssueSeverity::Critical),
        warning: audit.severity_count(fraiseql_core::design::IssueSeverity::Warning),
        info: audit.severity_count(fraiseql_core::design::IssueSeverity::Info),
    };

    let response = LintResponse {
        overall_score: audit.score(),
        severity_counts,
        categories: CategoryScores {
            federation: fed_score,
            cost: cost_score,
            cache: cache_score,
            authorization: auth_score,
            compilation: comp_score,
        },
    };

    Ok(CommandResult::success("lint", serde_json::to_value(&response)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn default_opts() -> LintOptions {
        LintOptions {
            federation: false,
            cost: false,
            cache: false,
            auth: false,
            compilation: false,
            fail_on_critical: false,
            fail_on_warning: false,
            verbose: false,
        }
    }

    #[test]
    fn test_lint_valid_schema() {
        let schema_json = r#"{
            "types": [
                {
                    "name": "Query",
                    "fields": [
                        {"name": "users", "type": "[User!]"}
                    ]
                },
                {
                    "name": "User",
                    "fields": [
                        {"name": "id", "type": "ID", "isPrimaryKey": true},
                        {"name": "name", "type": "String"}
                    ]
                }
            ]
        }"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(schema_json.as_bytes()).unwrap();
        let path = file.path().to_str().unwrap();

        let result = run(path, default_opts());
        assert!(result.is_ok());

        let cmd_result = result.unwrap();
        assert_eq!(cmd_result.status, "success");
        assert_eq!(cmd_result.command, "lint");
        assert!(cmd_result.data.is_some());
    }

    #[test]
    fn test_lint_file_not_found() {
        let result = run("nonexistent_schema.json", default_opts());
        assert!(result.is_ok());

        let cmd_result = result.unwrap();
        assert_eq!(cmd_result.status, "error");
        assert_eq!(cmd_result.code, Some("FILE_NOT_FOUND".to_string()));
    }

    #[test]
    fn test_lint_returns_score() {
        let schema_json = r#"{"types": []}"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(schema_json.as_bytes()).unwrap();
        let path = file.path().to_str().unwrap();

        let result = run(path, default_opts());
        assert!(result.is_ok());

        let cmd_result = result.unwrap();
        if let Some(data) = &cmd_result.data {
            assert!(data.get("overall_score").is_some());
            assert!(data.get("severity_counts").is_some());
            assert!(data.get("categories").is_some());
        }
    }
}
