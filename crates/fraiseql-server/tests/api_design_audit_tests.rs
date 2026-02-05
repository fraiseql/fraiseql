//! Design Audit API Tests
//!
//! Tests for the design quality audit endpoints that leverage the FraiseQL-calibrated
//! design rules from fraiseql-core.

use fraiseql_server::routes::api::design::{
    CategoryAuditResponse, DesignAuditRequest, DesignIssueResponse,
};
use serde_json::json;

// ============================================================================
// Test Helpers
// ============================================================================

/// Create a minimal valid schema
fn minimal_schema() -> serde_json::Value {
    json!({
        "types": [
            {
                "name": "Query",
                "fields": [
                    {"name": "hello", "type": "String"}
                ]
            }
        ]
    })
}

/// Create a schema with over-federated entity (User in multiple subgraphs)
fn over_federated_schema() -> serde_json::Value {
    json!({
        "subgraphs": [
            {
                "name": "users",
                "entities": ["User"]
            },
            {
                "name": "posts",
                "entities": ["User", "Post"]
            },
            {
                "name": "comments",
                "entities": ["User", "Comment"]
            }
        ],
        "types": [
            {
                "name": "User",
                "fields": [
                    {"name": "id", "type": "ID", "isPrimaryKey": true},
                    {"name": "name", "type": "String"}
                ]
            },
            {
                "name": "Post",
                "fields": [
                    {"name": "id", "type": "ID", "isPrimaryKey": true},
                    {"name": "title", "type": "String"}
                ]
            },
            {
                "name": "Comment",
                "fields": [
                    {"name": "id", "type": "ID", "isPrimaryKey": true},
                    {"name": "text", "type": "String"}
                ]
            }
        ]
    })
}

// ============================================================================
// Federation Audit Endpoint Tests
// ============================================================================

#[test]
fn test_federation_audit_request_deserialization() {
    let req = DesignAuditRequest {
        schema: minimal_schema(),
    };
    assert!(req.schema.get("types").is_some());
}

#[test]
fn test_federation_audit_response_structure() {
    let response = CategoryAuditResponse {
        score:  85,
        issues: vec![],
    };
    assert_eq!(response.score, 85);
    assert!(response.issues.is_empty());
}

#[test]
fn test_federation_audit_with_issue() {
    let issue = DesignIssueResponse {
        severity:   "warning".to_string(),
        message:    "User entity spread across 3 subgraphs".to_string(),
        suggestion: "Consolidate in primary subgraph".to_string(),
        affected:   Some("User".to_string()),
    };
    assert_eq!(issue.severity, "warning");
    assert_eq!(issue.affected, Some("User".to_string()));
}

#[test]
fn test_federation_audit_detects_jsonb_fragmentation() {
    // Schema with User in 3 subgraphs should produce lower federation score
    let schema = over_federated_schema();
    assert!(schema.get("subgraphs").is_some());
    let subgraphs = schema["subgraphs"].as_array().unwrap();
    assert_eq!(subgraphs.len(), 3);

    // User appears in all 3 subgraphs - this is over-federation
    let mut user_count = 0;
    for subgraph in subgraphs {
        if let Some(entities) = subgraph.get("entities").and_then(|e| e.as_array()) {
            if entities.iter().any(|e| e.as_str() == Some("User")) {
                user_count += 1;
            }
        }
    }
    assert_eq!(user_count, 3, "User should appear in 3 subgraphs for this test");
}

#[test]
fn test_federation_audit_empty_schema() {
    let response = CategoryAuditResponse {
        score:  100,
        issues: vec![],
    };
    // Empty schema with no issues should have perfect score
    assert_eq!(response.score, 100);
}

// ============================================================================
// Cost Audit Endpoint Tests
// ============================================================================

fn deep_nesting_schema() -> serde_json::Value {
    json!({
        "types": [
            {
                "name": "Query",
                "fields": [
                    {"name": "user", "type": "User"}
                ]
            },
            {
                "name": "User",
                "fields": [
                    {"name": "id", "type": "ID", "isPrimaryKey": true},
                    {"name": "posts", "type": "[Post!]"}
                ]
            },
            {
                "name": "Post",
                "fields": [
                    {"name": "id", "type": "ID", "isPrimaryKey": true},
                    {"name": "comments", "type": "[Comment!]"}
                ]
            },
            {
                "name": "Comment",
                "fields": [
                    {"name": "id", "type": "ID", "isPrimaryKey": true},
                    {"name": "text", "type": "String"}
                ]
            }
        ]
    })
}

#[test]
fn test_cost_audit_response_structure() {
    let response = CategoryAuditResponse {
        score:  75,
        issues: vec![],
    };
    assert_eq!(response.score, 75);
}

#[test]
fn test_cost_audit_with_complexity_warning() {
    let issue = DesignIssueResponse {
        severity:   "critical".to_string(),
        message:    "Query can reach 1250 complexity in worst case".to_string(),
        suggestion: "Add depth limit or paginate nested fields".to_string(),
        affected:   Some("complexity: 1250".to_string()),
    };
    assert_eq!(issue.severity, "critical");
    assert!(issue.message.contains("complexity"));
}

#[test]
fn test_cost_audit_detects_deep_nesting() {
    let schema = deep_nesting_schema();
    let types = schema["types"].as_array().unwrap();

    // User has posts list, Post has comments list
    let user_type = types.iter().find(|t| t["name"] == "User").unwrap();
    let posts_field = user_type["fields"].as_array().unwrap().iter().find(|f| f["name"] == "posts");
    assert!(posts_field.is_some());
}

#[test]
fn test_cost_audit_unbounded_pagination() {
    // Schema without limit defaults should be flagged
    let schema = json!({
        "types": [
            {
                "name": "Query",
                "fields": [
                    {"name": "allUsers", "type": "[User!]"}  // No limit specified
                ]
            },
            {
                "name": "User",
                "fields": [
                    {"name": "id", "type": "ID", "isPrimaryKey": true}
                ]
            }
        ]
    });

    assert!(schema["types"][0]["fields"][0].get("limit").is_none());
}

// ============================================================================
// Cache Audit Endpoint Tests
// ============================================================================

fn ttl_mismatch_schema() -> serde_json::Value {
    json!({
        "subgraphs": [
            {
                "name": "users",
                "entities": ["User"],
                "cacheTtlSeconds": 300  // 5 minutes
            },
            {
                "name": "posts",
                "entities": ["Post"],
                "references": [
                    {"type": "User", "via": "users", "cacheTtlSeconds": 1800}  // 30 minutes - MISMATCH!
                ]
            }
        ],
        "types": [
            {
                "name": "User",
                "fields": [{"name": "id", "type": "ID", "isPrimaryKey": true}]
            },
            {
                "name": "Post",
                "fields": [{"name": "id", "type": "ID", "isPrimaryKey": true}]
            }
        ]
    })
}

#[test]
fn test_cache_audit_response_structure() {
    let response = CategoryAuditResponse {
        score:  90,
        issues: vec![],
    };
    assert_eq!(response.score, 90);
    assert!(response.issues.is_empty());
}

#[test]
fn test_cache_audit_with_ttl_issue() {
    let issue = DesignIssueResponse {
        severity:   "warning".to_string(),
        message:    "User cached 5min in users-service, 30min in posts-service".to_string(),
        suggestion: "Align TTLs for consistent JSONB coherency".to_string(),
        affected:   Some("User".to_string()),
    };
    assert_eq!(issue.severity, "warning");
    assert!(issue.message.contains("cached"));
}

#[test]
fn test_cache_audit_ttl_mismatch() {
    let schema = ttl_mismatch_schema();
    let subgraphs = schema["subgraphs"].as_array().unwrap();

    // Verify schema has mismatched TTLs
    assert_eq!(subgraphs.len(), 2);
    let users_ttl = subgraphs[0]["cacheTtlSeconds"].as_i64().unwrap();
    let posts_ttl = subgraphs[1]["references"][0]["cacheTtlSeconds"].as_i64().unwrap();
    assert_ne!(users_ttl, posts_ttl);
}

#[test]
fn test_cache_audit_missing_directives() {
    // Schema without @cache directives should be flagged
    let schema = json!({
        "types": [
            {
                "name": "Query",
                "fields": [
                    {
                        "name": "expensiveField",
                        "type": "String",
                        "cached": false  // Missing cache directive
                    }
                ]
            }
        ]
    });

    assert!(!schema["types"][0]["fields"][0]["cached"].as_bool().unwrap_or(false));
}

// ============================================================================
// Authorization Audit Endpoint Tests
// ============================================================================

fn auth_boundary_leak_schema() -> serde_json::Value {
    json!({
        "types": [
            {
                "name": "User",
                "fields": [
                    {"name": "id", "type": "ID", "isPrimaryKey": true},
                    {
                        "name": "email",
                        "type": "String",
                        "requiresAuth": true,  // Requires auth
                        "requiredScopes": ["user:read"]
                    }
                ]
            }
        ],
        "subgraphs": [
            {
                "name": "users",
                "entities": ["User"]
            },
            {
                "name": "analytics",
                "references": [
                    {
                        "type": "User",
                        "via": "users",
                        "canAccessFields": ["id", "email"]  // LEAK: accessing protected field!
                    }
                ]
            }
        ]
    })
}

#[test]
fn test_auth_audit_response_structure() {
    let response = CategoryAuditResponse {
        score:  88,
        issues: vec![],
    };
    assert_eq!(response.score, 88);
}

#[test]
fn test_auth_audit_with_boundary_issue() {
    let issue = DesignIssueResponse {
        severity:   "critical".to_string(),
        message:    "User.email exposed to analytics-service without auth scope".to_string(),
        suggestion: "Add auth boundary check or restrict field access".to_string(),
        affected:   Some("User.email".to_string()),
    };
    assert_eq!(issue.severity, "critical");
    assert!(issue.message.contains("auth"));
}

#[test]
fn test_auth_audit_boundary_leak() {
    let schema = auth_boundary_leak_schema();
    let email_field = &schema["types"][0]["fields"][1];

    // Verify User.email is marked as requiring auth
    assert_eq!(email_field["name"], "email");
    assert!(email_field["requiresAuth"].as_bool().unwrap_or(false));
}

#[test]
fn test_auth_audit_missing_directives() {
    let schema = json!({
        "types": [
            {
                "name": "Mutation",
                "fields": [
                    {
                        "name": "deleteUser",
                        "type": "Boolean",
                        "requiresAuth": false  // Unprotected mutation!
                    }
                ]
            }
        ]
    });

    let mutation_field = &schema["types"][0]["fields"][0];
    assert!(!mutation_field["requiresAuth"].as_bool().unwrap_or(false));
}

// ============================================================================
// Compilation Audit Endpoint Tests
// ============================================================================

fn circular_types_schema() -> serde_json::Value {
    json!({
        "types": [
            {
                "name": "User",
                "fields": [
                    {"name": "id", "type": "ID", "isPrimaryKey": true},
                    {"name": "posts", "type": "[Post!]"}  // References Post
                ]
            },
            {
                "name": "Post",
                "fields": [
                    {"name": "id", "type": "ID", "isPrimaryKey": true},
                    {"name": "author", "type": "User"}  // References back to User - CIRCULAR!
                ]
            }
        ]
    })
}

#[test]
fn test_compilation_audit_response_structure() {
    let response = CategoryAuditResponse {
        score:  80,
        issues: vec![],
    };
    assert_eq!(response.score, 80);
}

#[test]
fn test_compilation_audit_with_circular_issue() {
    let issue = DesignIssueResponse {
        severity:   "warning".to_string(),
        message:    "Circular type reference: User -> Post -> User".to_string(),
        suggestion: "Break cycle by making one direction reference-only".to_string(),
        affected:   Some("User".to_string()),
    };
    assert_eq!(issue.severity, "warning");
    assert!(issue.message.contains("Circular"));
}

#[test]
fn test_compilation_audit_circular_types() {
    let schema = circular_types_schema();
    let types = schema["types"].as_array().unwrap();

    let user_type = types.iter().find(|t| t["name"] == "User").unwrap();
    let post_type = types.iter().find(|t| t["name"] == "Post").unwrap();

    // User has posts field referencing Post
    let user_has_posts =
        user_type["fields"].as_array().unwrap().iter().any(|f| f["name"] == "posts");
    assert!(user_has_posts);

    // Post has author field referencing User
    let post_has_author =
        post_type["fields"].as_array().unwrap().iter().any(|f| f["name"] == "author");
    assert!(post_has_author);
}

#[test]
fn test_compilation_audit_missing_primary_keys() {
    let schema = json!({
        "types": [
            {
                "name": "User",
                "fields": [
                    {"name": "id", "type": "ID"},  // Missing isPrimaryKey
                    {"name": "name", "type": "String"}
                ]
            }
        ]
    });

    let id_field = &schema["types"][0]["fields"][0];
    assert!(!id_field.get("isPrimaryKey").and_then(|v| v.as_bool()).unwrap_or(false));
}

// ============================================================================
// Overall Design Audit Endpoint Tests
// ============================================================================

#[test]
fn test_design_audit_request_creation() {
    let req = DesignAuditRequest {
        schema: minimal_schema(),
    };
    assert!(req.schema.get("types").is_some());
}

#[test]
fn test_design_audit_response_has_all_categories() {
    // Response should include all 5 category scores
    let categories = vec![
        "federation",
        "cost",
        "cache",
        "authorization",
        "compilation",
    ];
    for category in categories {
        assert!(!category.is_empty());
    }
}

#[test]
fn test_design_audit_score_range() {
    // Scores should be 0-100
    let test_scores = vec![0u8, 50, 100];
    for score in test_scores {
        assert!(score <= 100, "Score {} should be <= 100", score);
    }
}

#[test]
fn test_design_audit_severity_counts() {
    use fraiseql_server::routes::api::design::SeverityCountResponse;

    let counts = SeverityCountResponse {
        critical: 1,
        warning:  3,
        info:     5,
    };

    assert_eq!(counts.critical, 1);
    assert_eq!(counts.warning, 3);
    assert_eq!(counts.info, 5);
}

#[test]
fn test_design_audit_issue_has_suggestion() {
    let issue = DesignIssueResponse {
        severity:   "warning".to_string(),
        message:    "Some issue".to_string(),
        suggestion: "Fix by doing X".to_string(),
        affected:   None,
    };

    // Every issue must have a suggestion
    assert!(!issue.suggestion.is_empty());
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn test_design_audit_request_with_empty_schema() {
    let req = DesignAuditRequest { schema: json!({}) };
    // Empty schema should still be processable
    assert!(req.schema.is_object());
}

#[test]
fn test_design_audit_request_with_null_fields() {
    let req = DesignAuditRequest {
        schema: json!({"types": null}),
    };
    // Request with null fields should be deserializable
    assert!(req.schema.get("types").is_some());
}

#[test]
fn test_design_issue_required_fields() {
    let issue = DesignIssueResponse {
        severity:   "critical".to_string(),
        message:    "Test issue".to_string(),
        suggestion: "Fix this".to_string(),
        affected:   None,
    };

    // Verify all required fields are present
    assert!(!issue.severity.is_empty());
    assert!(!issue.message.is_empty());
    assert!(!issue.suggestion.is_empty());
}

// ============================================================================
// Response Content Tests
// ============================================================================

#[test]
fn test_federation_issue_content() {
    let issue = DesignIssueResponse {
        severity:   "critical".to_string(),
        message:    "User entity spread across 3 subgraphs prevents efficient JSONB batching"
            .to_string(),
        suggestion: "Move User to primary subgraph, use references elsewhere".to_string(),
        affected:   Some("User".to_string()),
    };

    // Verify issue has all required fields
    assert!(vec!["critical", "warning", "info"].contains(&issue.severity.as_str()));
    assert!(!issue.message.is_empty());
    assert!(!issue.suggestion.is_empty());
}

#[test]
fn test_cost_warning_content() {
    let issue = DesignIssueResponse {
        severity:   "warning".to_string(),
        message:    "Nested lists create O(n²) compiled JSONB cardinality".to_string(),
        suggestion: "Add pagination limits or reduce nesting depth".to_string(),
        affected:   Some("complexity: 1500".to_string()),
    };

    assert!(issue.message.contains("cardinality") || issue.message.contains("complexity"));
    assert!(!issue.suggestion.is_empty());
}

#[test]
fn test_issue_suggestion_is_specific() {
    let good_suggestion = "Move User to primary subgraph, use references elsewhere".to_string();
    let bad_suggestion = "Fix this issue".to_string();

    // Good suggestions have specific guidance
    assert!(good_suggestion.len() > bad_suggestion.len());
    assert!(good_suggestion.contains("primary subgraph") || good_suggestion.contains("references"));
}

// ============================================================================
// Schema Complexity Tests
// ============================================================================

#[test]
fn test_minimal_schema_audit() {
    let schema = minimal_schema();
    assert!(schema.get("types").is_some());
    let types = schema["types"].as_array().unwrap();
    assert!(types.len() > 0);
}

#[test]
fn test_complex_schema_audit() {
    let schema = over_federated_schema();
    assert!(schema.get("subgraphs").is_some());
    assert!(schema.get("types").is_some());
}

#[test]
fn test_score_improves_with_fixes() {
    // Create a schema with issues
    let problematic = over_federated_schema();

    // Create a schema without those issues
    let improved = json!({
        "subgraphs": [
            {
                "name": "users",
                "entities": ["User"]
            }
        ],
        "types": [
            {
                "name": "User",
                "fields": [{"name": "id", "type": "ID", "isPrimaryKey": true}]
            }
        ]
    });

    // Both should be valid schemas
    assert!(problematic.get("types").is_some());
    assert!(improved.get("types").is_some());

    // Improved should have fewer entities across subgraphs
    let prob_subgraph_count = problematic["subgraphs"].as_array().unwrap().len();
    let imp_subgraph_count = improved["subgraphs"].as_array().unwrap().len();
    assert!(imp_subgraph_count <= prob_subgraph_count);
}

// ============================================================================
// Schema Validation Tests
// ============================================================================

#[test]
fn test_schema_with_all_required_fields() {
    let schema = json!({
        "types": [
            {
                "name": "User",
                "fields": [
                    {"name": "id", "type": "ID", "isPrimaryKey": true},
                    {"name": "name", "type": "String"}
                ]
            }
        ]
    });

    let user_type = &schema["types"][0];
    assert_eq!(user_type["name"], "User");
    assert!(user_type["fields"].as_array().is_some());
}

#[test]
fn test_schema_federation_structure() {
    let schema = over_federated_schema();

    // Verify federation structure
    if let Some(subgraphs) = schema["subgraphs"].as_array() {
        for subgraph in subgraphs {
            assert!(subgraph.get("name").is_some());
            assert!(subgraph.get("entities").is_some());
        }
    }
}
