//! Design Quality Analysis Engine
//!
//! Provides linting and quality enforcement for GraphQL schema architecture.
//! Detects anti-patterns and provides actionable recommendations aligned with
//! FraiseQL's compilation model.

pub mod authorization;
pub mod cache;
pub mod compilation;
pub mod cost;
pub mod federation;
pub mod schema_patterns;

use serde::{Deserialize, Serialize};

/// Severity level for design issues
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueSeverity {
    /// Critical issues that may cause performance problems or bugs
    #[serde(rename = "critical")]
    Critical,
    /// Warning issues that should be addressed
    #[serde(rename = "warning")]
    Warning,
    /// Informational suggestions for improvement
    #[serde(rename = "info")]
    Info,
}

impl IssueSeverity {
    /// Get numeric weight for scoring (critical=3, warning=2, info=1)
    pub fn weight(&self) -> u32 {
        match self {
            IssueSeverity::Critical => 3,
            IssueSeverity::Warning => 2,
            IssueSeverity::Info => 1,
        }
    }
}

/// Federation-related design issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationIssue {
    /// Severity level of the issue
    pub severity:   IssueSeverity,
    /// Clear message describing the issue
    pub message:    String,
    /// Actionable suggestion for fixing the issue
    pub suggestion: String,
    /// Affected entity or component (if applicable)
    pub entity:     Option<String>,
}

/// Cost analysis warning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostWarning {
    /// Severity level of the warning
    pub severity:              IssueSeverity,
    /// Clear message describing the issue
    pub message:               String,
    /// Actionable suggestion for fixing the issue
    pub suggestion:            String,
    /// Worst-case complexity score if applicable
    pub worst_case_complexity: Option<u32>,
}

/// Cache coherency issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheIssue {
    /// Severity level of the issue
    pub severity:   IssueSeverity,
    /// Clear message describing the issue
    pub message:    String,
    /// Actionable suggestion for fixing the issue
    pub suggestion: String,
    /// Affected entity or field (if applicable)
    pub affected:   Option<String>,
}

/// Authorization boundary issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthIssue {
    /// Severity level of the issue
    pub severity:       IssueSeverity,
    /// Clear message describing the issue
    pub message:        String,
    /// Actionable suggestion for fixing the issue
    pub suggestion:     String,
    /// Affected field or scope (if applicable)
    pub affected_field: Option<String>,
}

/// Schema design issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaIssue {
    /// Severity level of the issue
    pub severity:      IssueSeverity,
    /// Clear message describing the issue
    pub message:       String,
    /// Actionable suggestion for fixing the issue
    pub suggestion:    String,
    /// Affected type or pattern (if applicable)
    pub affected_type: Option<String>,
}

/// Complete design quality audit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignAudit {
    /// Federation-related issues
    pub federation_issues: Vec<FederationIssue>,
    /// Cost analysis warnings
    pub cost_warnings:     Vec<CostWarning>,
    /// Cache coherency issues
    pub cache_issues:      Vec<CacheIssue>,
    /// Authorization boundary issues
    pub auth_issues:       Vec<AuthIssue>,
    /// Schema design issues
    pub schema_issues:     Vec<SchemaIssue>,
}

impl DesignAudit {
    /// Create a new empty audit
    pub fn new() -> Self {
        Self {
            federation_issues: Vec::new(),
            cost_warnings:     Vec::new(),
            cache_issues:      Vec::new(),
            auth_issues:       Vec::new(),
            schema_issues:     Vec::new(),
        }
    }

    /// Analyze a schema from JSON string
    pub fn from_schema_json(json: &str) -> Result<Self, serde_json::Error> {
        // Parse the schema JSON
        let schema: serde_json::Value = serde_json::from_str(json)?;

        let mut audit = Self::new();

        // Run all analysis engines
        federation::analyze(&schema, &mut audit);
        cost::analyze(&schema, &mut audit);
        cache::analyze(&schema, &mut audit);
        authorization::analyze(&schema, &mut audit);
        schema_patterns::analyze(&schema, &mut audit);
        compilation::analyze(&schema, &mut audit);

        Ok(audit)
    }

    /// Calculate overall design quality score (0-100)
    pub fn score(&self) -> u8 {
        // Base score
        let mut score: f64 = 100.0;

        // Deduct points for each issue based on severity
        // Critical issues are heavily penalized
        for issue in &self.federation_issues {
            let penalty = match issue.severity {
                IssueSeverity::Critical => 25.0,
                IssueSeverity::Warning => 15.0,
                IssueSeverity::Info => 3.0,
            };
            score -= penalty;
        }
        for warning in &self.cost_warnings {
            let penalty = match warning.severity {
                IssueSeverity::Critical => 20.0,
                IssueSeverity::Warning => 8.0,
                IssueSeverity::Info => 2.0,
            };
            score -= penalty;
        }
        for issue in &self.cache_issues {
            let penalty = match issue.severity {
                IssueSeverity::Critical => 15.0,
                IssueSeverity::Warning => 6.0,
                IssueSeverity::Info => 1.0,
            };
            score -= penalty;
        }
        for issue in &self.auth_issues {
            let penalty = match issue.severity {
                IssueSeverity::Critical => 25.0,
                IssueSeverity::Warning => 12.0,
                IssueSeverity::Info => 2.0,
            };
            score -= penalty;
        }
        for issue in &self.schema_issues {
            let penalty = match issue.severity {
                IssueSeverity::Critical => 15.0,
                IssueSeverity::Warning => 5.0,
                IssueSeverity::Info => 1.0,
            };
            score -= penalty;
        }

        // Clamp to 0-100
        let score = score.clamp(0.0, 100.0);
        score as u8
    }

    /// Count issues by severity level
    pub fn severity_count(&self, severity: IssueSeverity) -> usize {
        let fed_count = self.federation_issues.iter().filter(|i| i.severity == severity).count();
        let cost_count = self.cost_warnings.iter().filter(|w| w.severity == severity).count();
        let cache_count = self.cache_issues.iter().filter(|i| i.severity == severity).count();
        let auth_count = self.auth_issues.iter().filter(|i| i.severity == severity).count();
        let schema_count = self.schema_issues.iter().filter(|i| i.severity == severity).count();

        fed_count + cost_count + cache_count + auth_count + schema_count
    }

    /// Get all issues as a flat list
    pub fn all_issues(&self) -> Vec<String> {
        let mut issues = Vec::new();

        for issue in &self.federation_issues {
            issues.push(format!("{:?}: {}", issue.severity, issue.message));
        }
        for warning in &self.cost_warnings {
            issues.push(format!("{:?}: {}", warning.severity, warning.message));
        }
        for issue in &self.cache_issues {
            issues.push(format!("{:?}: {}", issue.severity, issue.message));
        }
        for issue in &self.auth_issues {
            issues.push(format!("{:?}: {}", issue.severity, issue.message));
        }
        for issue in &self.schema_issues {
            issues.push(format!("{:?}: {}", issue.severity, issue.message));
        }

        issues
    }
}

impl Default for DesignAudit {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_issue_severity_weight() {
        assert_eq!(IssueSeverity::Critical.weight(), 3);
        assert_eq!(IssueSeverity::Warning.weight(), 2);
        assert_eq!(IssueSeverity::Info.weight(), 1);
    }

    #[test]
    fn test_empty_audit_score() {
        let audit = DesignAudit::new();
        assert_eq!(audit.score(), 100);
    }

    #[test]
    fn test_severity_count_empty() {
        let audit = DesignAudit::new();
        assert_eq!(audit.severity_count(IssueSeverity::Critical), 0);
        assert_eq!(audit.severity_count(IssueSeverity::Warning), 0);
        assert_eq!(audit.severity_count(IssueSeverity::Info), 0);
    }
}
