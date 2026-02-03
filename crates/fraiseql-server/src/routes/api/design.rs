//! Design Quality Audit API Endpoints
//!
//! Provides FraiseQL-calibrated design quality analysis for schemas.

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use fraiseql_core::db::traits::DatabaseAdapter;
use fraiseql_core::design::{DesignAudit, IssueSeverity};
use crate::routes::api::types::{ApiResponse, ApiError};
use crate::routes::graphql::AppState;

/// Request body for design audit endpoints
#[derive(Debug, Clone, Deserialize)]
pub struct DesignAuditRequest {
    /// Schema to analyze (JSON)
    pub schema: serde_json::Value,
}

/// Single design issue response
#[derive(Debug, Clone, Serialize)]
pub struct DesignIssueResponse {
    /// Severity: critical, warning, info
    pub severity: String,
    /// Human-readable message
    pub message: String,
    /// Actionable suggestion
    pub suggestion: String,
    /// Affected entity/field if applicable
    pub affected: Option<String>,
}

/// Category audit response with score and issues
#[derive(Debug, Clone, Serialize)]
pub struct CategoryAuditResponse {
    /// Category score (0-100)
    pub score: u8,
    /// Issues found in this category
    pub issues: Vec<DesignIssueResponse>,
}

/// Severity counts
#[derive(Debug, Clone, Serialize)]
pub struct SeverityCountResponse {
    /// Critical issues count
    pub critical: usize,
    /// Warning issues count
    pub warning: usize,
    /// Info issues count
    pub info: usize,
}

/// Complete design audit response
#[derive(Debug, Clone, Serialize)]
pub struct DesignAuditResponse {
    /// Overall design score (0-100)
    pub overall_score: u8,
    /// Issue counts by severity
    pub severity_counts: SeverityCountResponse,
    /// Federation analysis (JSONB batching)
    pub federation: CategoryAuditResponse,
    /// Cost analysis (compiled determinism)
    pub cost: CategoryAuditResponse,
    /// Cache analysis (JSONB coherency)
    pub cache: CategoryAuditResponse,
    /// Authorization analysis
    pub authorization: CategoryAuditResponse,
    /// Compilation analysis
    pub compilation: CategoryAuditResponse,
}

/// Overall design audit endpoint
pub async fn overall_design_audit_handler<A: DatabaseAdapter>(
    State(_state): State<AppState<A>>,
    Json(req): Json<DesignAuditRequest>,
) -> std::result::Result<Json<ApiResponse<DesignAuditResponse>>, ApiError> {
    let audit = DesignAudit::from_schema_json(&req.schema.to_string())
        .map_err(|e| ApiError::parse_error(format!("Invalid schema: {}", e)))?;

    // Convert federation issues
    let federation_issues: Vec<DesignIssueResponse> = audit
        .federation_issues
        .iter()
        .map(|issue| DesignIssueResponse {
            severity: format!("{:?}", issue.severity).to_lowercase(),
            message: issue.message.clone(),
            suggestion: issue.suggestion.clone(),
            affected: issue.entity.clone(),
        })
        .collect();

    // Convert cost warnings
    let cost_issues: Vec<DesignIssueResponse> = audit
        .cost_warnings
        .iter()
        .map(|warning| DesignIssueResponse {
            severity: format!("{:?}", warning.severity).to_lowercase(),
            message: warning.message.clone(),
            suggestion: warning.suggestion.clone(),
            affected: warning
                .worst_case_complexity
                .map(|c| format!("complexity: {}", c)),
        })
        .collect();

    // Convert cache issues
    let cache_issues: Vec<DesignIssueResponse> = audit
        .cache_issues
        .iter()
        .map(|issue| DesignIssueResponse {
            severity: format!("{:?}", issue.severity).to_lowercase(),
            message: issue.message.clone(),
            suggestion: issue.suggestion.clone(),
            affected: issue.affected.clone(),
        })
        .collect();

    // Convert auth issues
    let auth_issues: Vec<DesignIssueResponse> = audit
        .auth_issues
        .iter()
        .map(|issue| DesignIssueResponse {
            severity: format!("{:?}", issue.severity).to_lowercase(),
            message: issue.message.clone(),
            suggestion: issue.suggestion.clone(),
            affected: issue.affected_field.clone(),
        })
        .collect();

    // Convert compilation issues
    let compilation_issues: Vec<DesignIssueResponse> = audit
        .schema_issues
        .iter()
        .map(|issue| DesignIssueResponse {
            severity: format!("{:?}", issue.severity).to_lowercase(),
            message: issue.message.clone(),
            suggestion: issue.suggestion.clone(),
            affected: issue.affected_type.clone(),
        })
        .collect();

    let severity_counts = SeverityCountResponse {
        critical: audit.severity_count(IssueSeverity::Critical),
        warning: audit.severity_count(IssueSeverity::Warning),
        info: audit.severity_count(IssueSeverity::Info),
    };

    let fed_score = if federation_issues.is_empty() {
        100
    } else {
        (100u32 - (federation_issues.len() as u32 * 10)).max(0).min(100) as u8
    };

    let cost_score = if cost_issues.is_empty() {
        100
    } else {
        (100u32 - (cost_issues.len() as u32 * 8)).max(0).min(100) as u8
    };

    let cache_score = if cache_issues.is_empty() {
        100
    } else {
        (100u32 - (cache_issues.len() as u32 * 6)).max(0).min(100) as u8
    };

    let auth_score = if auth_issues.is_empty() {
        100
    } else {
        (100u32 - (auth_issues.len() as u32 * 12)).max(0).min(100) as u8
    };

    let comp_score = if compilation_issues.is_empty() {
        100
    } else {
        (100u32 - (compilation_issues.len() as u32 * 10)).max(0).min(100) as u8
    };

    let response = DesignAuditResponse {
        overall_score: audit.score(),
        severity_counts,
        federation: CategoryAuditResponse {
            score: fed_score,
            issues: federation_issues,
        },
        cost: CategoryAuditResponse {
            score: cost_score,
            issues: cost_issues,
        },
        cache: CategoryAuditResponse {
            score: cache_score,
            issues: cache_issues,
        },
        authorization: CategoryAuditResponse {
            score: auth_score,
            issues: auth_issues,
        },
        compilation: CategoryAuditResponse {
            score: comp_score,
            issues: compilation_issues,
        },
    };

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data: response,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_count_response() {
        let resp = SeverityCountResponse {
            critical: 1,
            warning: 3,
            info: 5,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"critical\":1"));
    }
}
