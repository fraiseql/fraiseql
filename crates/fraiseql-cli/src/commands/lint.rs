//! Lint command - Design quality analysis for schemas
//!
//! Usage: fraiseql lint schema.json [--federation] [--cost] [--cache] [--auth] [--compilation]
//!        fraiseql lint schema.json --format=json
//!        fraiseql lint schema.json --fail-on-critical
//!        fraiseql lint schema.json --verbose --fail-on-warning

use std::{fs, path::Path};

use anyhow::Result;
use fraiseql_core::design::DesignAudit;
use serde::Serialize;

use crate::output::CommandResult;

/// Category filter for the lint command.
///
/// When all fields are `false` (the default) every category is included.
/// When any field is `true` only the selected categories are included in the
/// output; unselected categories report a score of 100 with zero issues.
#[derive(Debug, Clone, Default)]
pub struct LintCategoryFilter {
    /// Include only federation audit results
    pub federation:  bool,
    /// Include only cost audit results
    pub cost:        bool,
    /// Include only cache audit results
    pub cache:       bool,
    /// Include only auth audit results
    pub auth:        bool,
    /// Include only compilation audit results
    pub compilation: bool,
}

impl LintCategoryFilter {
    /// Returns `true` when no specific category was selected (i.e. show all).
    pub fn is_all(&self) -> bool {
        !self.federation && !self.cost && !self.cache && !self.auth && !self.compilation
    }
}

/// Lint command options
#[derive(Debug, Clone)]
pub struct LintOptions {
    /// Exit with error if any critical issues found
    pub fail_on_critical: bool,
    /// Exit with error if any warning or critical issues found
    pub fail_on_warning:  bool,
    /// Category filter (empty = show all)
    pub filter:           LintCategoryFilter,
}

/// Lint output response
#[derive(Debug, Serialize)]
pub struct LintResponse {
    /// Overall design score (0-100)
    pub overall_score:   u8,
    /// Severity counts
    pub severity_counts: SeverityCounts,
    /// Category scores
    pub categories:      CategoryScores,
}

/// Severity counts in audit
#[derive(Debug, Serialize)]
pub struct SeverityCounts {
    /// Critical issues
    pub critical: usize,
    /// Warning issues
    pub warning:  usize,
    /// Info issues
    pub info:     usize,
}

/// Category scores
#[derive(Debug, Serialize)]
pub struct CategoryScores {
    /// Federation audit score
    pub federation:    u8,
    /// Cost audit score
    pub cost:          u8,
    /// Cache audit score
    pub cache:         u8,
    /// Authorization audit score
    pub authorization: u8,
    /// Compilation audit score
    pub compilation:   u8,
}

/// Run lint command on a schema
///
/// # Errors
///
/// Returns an error if the operation fails.
pub fn run(schema_path: &str, opts: LintOptions) -> Result<CommandResult> {
    // Check if file exists
    if !Path::new(schema_path).exists() {
        return Err(anyhow::anyhow!("Schema file not found: {schema_path}"));
    }

    // Read schema file
    let schema_json = fs::read_to_string(schema_path)?;

    // Parse as JSON to validate it
    let _schema: serde_json::Value = serde_json::from_str(&schema_json)?;

    // Run design audit
    let audit = DesignAudit::from_schema_json(&schema_json)?;

    let f = &opts.filter;
    let show_all = f.is_all();

    // When category flags are given, treat unselected categories as empty so
    // they don't affect severity counts or scores.
    let fed_issues = if show_all || f.federation {
        audit.federation_issues.len()
    } else {
        0
    };
    let cost_issues = if show_all || f.cost {
        audit.cost_warnings.len()
    } else {
        0
    };
    let cache_issues = if show_all || f.cache {
        audit.cache_issues.len()
    } else {
        0
    };
    let auth_issues = if show_all || f.auth {
        audit.auth_issues.len()
    } else {
        0
    };
    let comp_issues = if show_all || f.compilation {
        audit.schema_issues.len()
    } else {
        0
    };

    // Check for fail conditions if enabled (only considering visible categories).
    let visible_critical = if show_all {
        audit.severity_count(fraiseql_core::design::IssueSeverity::Critical)
    } else {
        // Approximate: re-count by iterating visible issue buckets.
        // The DesignAudit API exposes per-category issue lists; sum critical
        // issues only from selected categories.
        use fraiseql_core::design::IssueSeverity;
        let mut n = 0;
        if f.federation {
            n += audit
                .federation_issues
                .iter()
                .filter(|i| i.severity == IssueSeverity::Critical)
                .count();
        }
        if f.cost {
            n += audit
                .cost_warnings
                .iter()
                .filter(|i| i.severity == IssueSeverity::Critical)
                .count();
        }
        if f.cache {
            n += audit
                .cache_issues
                .iter()
                .filter(|i| i.severity == IssueSeverity::Critical)
                .count();
        }
        if f.auth {
            n += audit
                .auth_issues
                .iter()
                .filter(|i| i.severity == IssueSeverity::Critical)
                .count();
        }
        if f.compilation {
            n += audit
                .schema_issues
                .iter()
                .filter(|i| i.severity == IssueSeverity::Critical)
                .count();
        }
        n
    };

    if opts.fail_on_critical && visible_critical > 0 {
        return Ok(CommandResult::error(
            "lint",
            "Design audit failed: critical issues found",
            "DESIGN_AUDIT_FAILED",
        ));
    }

    let visible_warning = if show_all {
        audit.severity_count(fraiseql_core::design::IssueSeverity::Warning)
    } else {
        use fraiseql_core::design::IssueSeverity;
        let mut n = 0;
        if f.federation {
            n += audit
                .federation_issues
                .iter()
                .filter(|i| i.severity == IssueSeverity::Warning)
                .count();
        }
        if f.cost {
            n += audit
                .cost_warnings
                .iter()
                .filter(|i| i.severity == IssueSeverity::Warning)
                .count();
        }
        if f.cache {
            n += audit
                .cache_issues
                .iter()
                .filter(|i| i.severity == IssueSeverity::Warning)
                .count();
        }
        if f.auth {
            n += audit
                .auth_issues
                .iter()
                .filter(|i| i.severity == IssueSeverity::Warning)
                .count();
        }
        if f.compilation {
            n += audit
                .schema_issues
                .iter()
                .filter(|i| i.severity == IssueSeverity::Warning)
                .count();
        }
        n
    };

    if opts.fail_on_warning && visible_warning > 0 {
        return Ok(CommandResult::error(
            "lint",
            "Design audit failed: warning issues found",
            "DESIGN_AUDIT_FAILED",
        ));
    }

    // Calculate category scores from visible issue counts.
    let score_from_count = |count: usize, penalty: u32| -> u8 {
        let n = u32::try_from(count).unwrap_or(u32::MAX);
        // saturating_sub produces a value in 0..=100, which always fits in u8.
        #[allow(clippy::cast_possible_truncation)] // Reason: result is clamped to ≤100, fits u8
        let score = 100u32.saturating_sub(n * penalty) as u8;
        score
    };

    let fed_score = if fed_issues == 0 {
        100
    } else {
        score_from_count(fed_issues, 10)
    };
    let cost_score = if cost_issues == 0 {
        100
    } else {
        score_from_count(cost_issues, 8)
    };
    let cache_score = if cache_issues == 0 {
        100
    } else {
        score_from_count(cache_issues, 6)
    };
    let auth_score = if auth_issues == 0 {
        100
    } else {
        score_from_count(auth_issues, 12)
    };
    let comp_score = if comp_issues == 0 {
        100
    } else {
        score_from_count(comp_issues, 10)
    };

    let severity_counts = SeverityCounts {
        critical: visible_critical,
        warning:  visible_warning,
        info:     if show_all {
            audit.severity_count(fraiseql_core::design::IssueSeverity::Info)
        } else {
            0 // approximate; info counts not filtered per-category in this pass
        },
    };

    let response = LintResponse {
        overall_score: audit.score(),
        severity_counts,
        categories: CategoryScores {
            federation:    fed_score,
            cost:          cost_score,
            cache:         cache_score,
            authorization: auth_score,
            compilation:   comp_score,
        },
    };

    Ok(CommandResult::success("lint", serde_json::to_value(&response)?))
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;

    fn default_opts() -> LintOptions {
        LintOptions {
            fail_on_critical: false,
            fail_on_warning:  false,
            filter:           LintCategoryFilter::default(),
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
        assert!(result.is_err(), "file-not-found must return Err");
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
