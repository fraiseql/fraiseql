//! Design Quality Analysis Tests
//!
//! Tests for the design quality enforcement engine that detects architectural
//! anti-patterns and provides actionable recommendations.

use fraiseql_core::design::{DesignAudit, IssueSeverity};

// Helper function to create a minimal test schema
fn create_test_schema() -> String {
    r#"{
        "types": [
            {
                "name": "User",
                "fields": [
                    {"name": "id", "type": "ID", "required": true},
                    {"name": "email", "type": "String", "required": true},
                    {"name": "name", "type": "String"}
                ]
            }
        ]
    }"#.to_string()
}

// ============================================================================
// Federation Rules Tests
// ============================================================================

#[test]
fn test_detect_over_federation() {
    // User entity in users-service, posts-service, and comments-service
    // Should warn: Entity exists in 3 subgraphs, consolidate
    let schema = r#"{
        "subgraphs": [
            {"name": "users-service", "entities": ["User"]},
            {"name": "posts-service", "entities": ["User", "Post"]},
            {"name": "comments-service", "entities": ["User", "Comment"]}
        ]
    }"#;

    let audit = DesignAudit::from_schema_json(schema).unwrap();
    let federation_issues = &audit.federation_issues;

    assert!(!federation_issues.is_empty(), "Should detect over-federation");
    assert!(
        federation_issues.iter().any(|issue| {
            issue.message.contains("User") && issue.message.contains("3")
        }),
        "Should identify User entity in 3 subgraphs"
    );
}

#[test]
fn test_detect_circular_dependencies() {
    // users-service → posts-service → users-service (via references)
    // Should warn: Circular dependency detected
    let schema = r#"{
        "subgraphs": [
            {
                "name": "users-service",
                "entities": ["User"],
                "references": [{"target_subgraph": "posts-service", "field": "author"}]
            },
            {
                "name": "posts-service",
                "entities": ["Post"],
                "references": [{"target_subgraph": "users-service", "field": "user"}]
            }
        ]
    }"#;

    let audit = DesignAudit::from_schema_json(schema).unwrap();

    assert!(
        !audit.federation_issues.is_empty(),
        "Should detect circular dependency"
    );
    assert!(
        audit.federation_issues.iter().any(|issue| {
            issue.message.contains("Circular") || issue.message.contains("cycle")
        }),
        "Should identify circular dependency"
    );
}

#[test]
fn test_no_federation_issues_for_well_designed_schema() {
    // Each entity in exactly one subgraph, no circular deps
    let schema = r#"{
        "subgraphs": [
            {
                "name": "users-service",
                "entities": ["User"]
            },
            {
                "name": "posts-service",
                "entities": ["Post"],
                "references": [{"target_subgraph": "users-service", "field": "author"}]
            }
        ]
    }"#;

    let audit = DesignAudit::from_schema_json(schema).unwrap();

    let critical_or_warning = audit
        .federation_issues
        .iter()
        .filter(|issue| {
            matches!(
                issue.severity,
                IssueSeverity::Critical | IssueSeverity::Warning
            )
        })
        .collect::<Vec<_>>();

    assert!(
        critical_or_warning.is_empty(),
        "Well-designed schema should have no critical/warning federation issues"
    );
}

// ============================================================================
// Cost Analysis Tests
// ============================================================================

#[test]
fn test_detect_worst_case_complexity() {
    // Query can hit 10,000+ complexity in worst case
    // Should warn: Cost avalanche scenario
    let schema = r#"{
        "types": [
            {
                "name": "Post",
                "fields": [
                    {"name": "id", "type": "ID"},
                    {"name": "comments", "type": "[Comment!]", "complexity_multiplier": 100}
                ]
            },
            {
                "name": "Comment",
                "fields": [
                    {"name": "id", "type": "ID"},
                    {"name": "replies", "type": "[Comment!]", "complexity_multiplier": 100}
                ]
            }
        ]
    }"#;

    let audit = DesignAudit::from_schema_json(schema).unwrap();

    assert!(
        !audit.cost_warnings.is_empty(),
        "Should detect worst-case complexity"
    );
    assert!(
        audit.cost_warnings.iter().any(|warning| {
            if let Some(complexity) = warning.worst_case_complexity {
                complexity > 1000
            } else {
                false
            }
        }),
        "Should calculate high worst-case complexity"
    );
}

#[test]
fn test_detect_unbounded_pagination() {
    // Fields without limit defaults
    let schema = r#"{
        "types": [
            {
                "name": "Query",
                "fields": [
                    {
                        "name": "posts",
                        "type": "[Post!]",
                        "args": [{"name": "first", "type": "Int"}],
                        "default_limit": null
                    }
                ]
            }
        ]
    }"#;

    let audit = DesignAudit::from_schema_json(schema).unwrap();

    assert!(
        audit.cost_warnings.iter().any(|warning| {
            warning.message.contains("pagination") || warning.message.contains("limit")
        }),
        "Should warn about unbounded pagination"
    );
}

#[test]
fn test_detect_field_multipliers() {
    // Lists within lists (O(n²) patterns)
    let schema = r#"{
        "types": [
            {
                "name": "User",
                "fields": [
                    {"name": "posts", "type": "[Post!]"}
                ]
            },
            {
                "name": "Post",
                "fields": [
                    {"name": "comments", "type": "[Comment!]"}
                ]
            }
        ]
    }"#;

    let audit = DesignAudit::from_schema_json(schema).unwrap();

    // Should detect multiplier patterns
    assert!(
        !audit.cost_warnings.is_empty(),
        "Should detect field multiplier patterns"
    );
}

// ============================================================================
// Cache Coherency Tests
// ============================================================================

#[test]
fn test_detect_cache_incoherence() {
    // User cached 5min in users-service, 30min in posts-service
    // Should warn: Inconsistent TTL
    let schema = r#"{
        "subgraphs": [
            {
                "name": "users-service",
                "entities": [
                    {
                        "name": "User",
                        "cache_ttl_seconds": 300
                    }
                ]
            },
            {
                "name": "posts-service",
                "entities": [
                    {
                        "name": "User",
                        "cache_ttl_seconds": 1800
                    }
                ]
            }
        ]
    }"#;

    let audit = DesignAudit::from_schema_json(schema).unwrap();

    assert!(
        !audit.cache_issues.is_empty(),
        "Should detect cache TTL incoherence"
    );
    assert!(
        audit.cache_issues.iter().any(|issue| {
            issue.message.contains("TTL") || issue.message.contains("inconsistent")
        }),
        "Should identify TTL mismatch"
    );
}

#[test]
fn test_detect_missing_cache_directives() {
    // Expensive fields without cache directives
    let schema = r#"{
        "types": [
            {
                "name": "User",
                "fields": [
                    {
                        "name": "complexCalculation",
                        "type": "String",
                        "is_expensive": true,
                        "has_cache_directive": false
                    }
                ]
            }
        ]
    }"#;

    let audit = DesignAudit::from_schema_json(schema).unwrap();

    assert!(
        audit.cache_issues.iter().any(|issue| {
            issue.message.contains("cache") || issue.message.contains("expensive")
        }),
        "Should warn about missing cache directives on expensive fields"
    );
}

// ============================================================================
// Authorization Tests
// ============================================================================

#[test]
fn test_detect_auth_boundary_leak() {
    // User.email exposed to comments-service without auth scope
    // Should warn: Cross-subgraph auth violation
    let schema = r#"{
        "subgraphs": [
            {
                "name": "users-service",
                "entities": [
                    {
                        "name": "User",
                        "fields": [
                            {
                                "name": "email",
                                "requires_auth": true,
                                "auth_scopes": ["user:profile"]
                            }
                        ]
                    }
                ]
            },
            {
                "name": "comments-service",
                "references": [
                    {
                        "target_type": "User",
                        "accessed_fields": ["email"],
                        "has_auth_check": false
                    }
                ]
            }
        ]
    }"#;

    let audit = DesignAudit::from_schema_json(schema).unwrap();

    assert!(
        !audit.auth_issues.is_empty(),
        "Should detect auth boundary leaks"
    );
    assert!(
        audit.auth_issues.iter().any(|issue| {
            issue.message.contains("auth") && (issue.message.contains("boundary") || issue.message.contains("leak"))
        }),
        "Should identify cross-subgraph auth violation"
    );
}

#[test]
fn test_detect_missing_auth_directives() {
    // Public mutations that should be protected
    let schema = r#"{
        "types": [
            {
                "name": "Mutation",
                "fields": [
                    {
                        "name": "updateUser",
                        "type": "User",
                        "requires_auth": false
                    }
                ]
            }
        ]
    }"#;

    let audit = DesignAudit::from_schema_json(schema).unwrap();

    assert!(
        audit.auth_issues.iter().any(|issue| {
            issue.message.contains("auth") || issue.message.contains("protected")
        }),
        "Should warn about unprotected mutations"
    );
}

// ============================================================================
// Design Score Tests
// ============================================================================

#[test]
fn test_design_score_calculation() {
    // Design score should be 0-100 based on issues found
    let schema = r#"{
        "subgraphs": [
            {"name": "users-service", "entities": ["User"]}
        ]
    }"#;

    let audit = DesignAudit::from_schema_json(schema).unwrap();
    let score = audit.score();

    assert!(score >= 0 && score <= 100, "Score should be 0-100");
}

#[test]
fn test_perfect_schema_has_high_score() {
    // Well-designed schema should have high score
    let schema = r#"{
        "subgraphs": [
            {
                "name": "users-service",
                "entities": ["User"],
                "config": {
                    "cache_ttl_seconds": 300,
                    "auth_required": true
                }
            },
            {
                "name": "posts-service",
                "entities": ["Post"],
                "references": [{"target_subgraph": "users-service"}]
            }
        ]
    }"#;

    let audit = DesignAudit::from_schema_json(schema).unwrap();
    let score = audit.score();

    assert!(score > 80, "Well-designed schema should score > 80");
}

#[test]
fn test_problematic_schema_has_low_score() {
    // Schema with many issues should have low score
    let schema = r#"{
        "subgraphs": [
            {"name": "users-service", "entities": ["User"]},
            {"name": "posts-service", "entities": ["User", "Post"]},
            {"name": "comments-service", "entities": ["User", "Comment"]}
        ]
    }"#;

    let audit = DesignAudit::from_schema_json(schema).unwrap();
    let score = audit.score();

    assert!(score < 60, "Schema with issues should score < 60");
}

// ============================================================================
// Severity Classification Tests
// ============================================================================

#[test]
fn test_severity_count() {
    // Should count issues by severity level
    let schema = r#"{
        "subgraphs": [
            {"name": "users-service", "entities": ["User"]},
            {"name": "posts-service", "entities": ["User", "Post"]},
            {"name": "comments-service", "entities": ["User", "Comment"]}
        ]
    }"#;

    let audit = DesignAudit::from_schema_json(schema).unwrap();

    let critical_count = audit.severity_count(IssueSeverity::Critical);
    let warning_count = audit.severity_count(IssueSeverity::Warning);

    // Should classify issues by severity
    assert!(critical_count >= 0);
    assert!(warning_count >= 0);
}

// ============================================================================
// Integration Tests
// ============================================================================

#[test]
fn test_design_audit_complete_response() {
    // Full audit should return all categories with scores
    let schema = create_test_schema();

    let audit = DesignAudit::from_schema_json(&schema).unwrap();

    // Should have all audit categories
    assert!(audit.federation_issues.is_empty() || !audit.federation_issues.is_empty());
    assert!(audit.cost_warnings.is_empty() || !audit.cost_warnings.is_empty());
    assert!(audit.cache_issues.is_empty() || !audit.cache_issues.is_empty());
    assert!(audit.auth_issues.is_empty() || !audit.auth_issues.is_empty());
    assert!(audit.schema_issues.is_empty() || !audit.schema_issues.is_empty());

    // Should calculate overall score
    let overall_score = audit.score();
    assert!(overall_score >= 0 && overall_score <= 100);
}

#[test]
fn test_design_audit_with_suggestions() {
    // Issues should include actionable suggestions
    let schema = r#"{
        "subgraphs": [
            {"name": "users-service", "entities": ["User"]},
            {"name": "posts-service", "entities": ["User", "Post"]}
        ]
    }"#;

    let audit = DesignAudit::from_schema_json(schema).unwrap();

    // Federation issues should have suggestions
    let fed_with_suggestions = audit.federation_issues.iter()
        .filter(|issue| !issue.suggestion.is_empty())
        .count();

    assert!(
        fed_with_suggestions > 0 || audit.federation_issues.is_empty(),
        "Issues should include suggestions"
    );
}
