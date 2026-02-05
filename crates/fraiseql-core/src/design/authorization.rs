//! Authorization boundary design rules
//!
//! Detects auth-related issues:
//! - Authorization boundary leaks (cross-subgraph auth violations)
//! - Missing @auth directives on sensitive fields
//! - Scope mismatches between subgraphs

use serde_json::Value;

use super::{AuthIssue, DesignAudit, IssueSeverity};

/// Analyze authorization patterns in the schema
pub fn analyze(schema: &Value, audit: &mut DesignAudit) {
    check_auth_boundary_leaks(schema, audit);
    check_missing_auth_directives(schema, audit);
}

/// Detect authorization boundary leaks
fn check_auth_boundary_leaks(schema: &Value, audit: &mut DesignAudit) {
    if let Some(subgraphs) = schema.get("subgraphs").and_then(|v| v.as_array()) {
        // First, collect fields that require auth from each type
        let mut auth_required_fields: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();

        for subgraph in subgraphs {
            if let Some(entities) = subgraph.get("entities").and_then(|v| v.as_array()) {
                for entity in entities {
                    let entity_name = if let Some(name) = entity.as_str() {
                        name.to_string()
                    } else if let Some(name) = entity.get("name").and_then(|v| v.as_str()) {
                        name.to_string()
                    } else {
                        continue;
                    };

                    if let Some(fields) = entity.get("fields").and_then(|v| v.as_array()) {
                        for field in fields {
                            let requires_auth = field
                                .get("requires_auth")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);

                            if requires_auth {
                                let field_name = field
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown")
                                    .to_string();
                                auth_required_fields
                                    .entry(entity_name.clone())
                                    .or_insert_with(Vec::new)
                                    .push(field_name);
                            }
                        }
                    }
                }
            }
        }

        // Now check for cross-subgraph references to auth-required fields
        for subgraph in subgraphs {
            if let Some(references) = subgraph.get("references").and_then(|v| v.as_array()) {
                for reference in references {
                    let target_type = reference
                        .get("target_type")
                        .and_then(|v| v.as_str())
                        .or_else(|| reference.get("target_subgraph").and_then(|v| v.as_str()))
                        .unwrap_or("unknown");
                    let accessed_fields =
                        reference.get("accessed_fields").and_then(|v| v.as_array());
                    let has_auth_check =
                        reference.get("has_auth_check").and_then(|v| v.as_bool()).unwrap_or(false);

                    if !has_auth_check {
                        if let Some(target_auth_fields) = auth_required_fields.get(target_type) {
                            if let Some(fields) = accessed_fields {
                                for field in fields {
                                    if let Some(field_str) = field.as_str() {
                                        if target_auth_fields.contains(&field_str.to_string()) {
                                            audit.auth_issues.push(AuthIssue {
                                                severity: IssueSeverity::Critical,
                                                message: format!(
                                                    "Auth boundary leak: Cross-subgraph reference accesses protected field {}.{} without auth check",
                                                    target_type, field_str
                                                ),
                                                suggestion: "Add authorization check or use auth-scoped references".to_string(),
                                                affected_field: Some(field_str.to_string()),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Detect missing auth directives
fn check_missing_auth_directives(schema: &Value, audit: &mut DesignAudit) {
    if let Some(types) = schema.get("types").and_then(|v| v.as_array()) {
        for type_def in types {
            let type_name = type_def.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");

            if type_name == "Mutation" || type_name == "Subscription" {
                if let Some(fields) = type_def.get("fields").and_then(|v| v.as_array()) {
                    for field in fields {
                        let requires_auth =
                            field.get("requires_auth").and_then(|v| v.as_bool()).unwrap_or(false);

                        if !requires_auth {
                            let field_name =
                                field.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");

                            audit.auth_issues.push(AuthIssue {
                                severity:       IssueSeverity::Warning,
                                message:        format!(
                                    "{}.{} is not protected by auth directive",
                                    type_name, field_name
                                ),
                                suggestion:     "Add @auth directive or authentication requirement"
                                    .to_string(),
                                affected_field: Some(field_name.to_string()),
                            });
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_analyze_empty_schema() {
        let schema = serde_json::json!({});
        let mut audit = DesignAudit::new();
        analyze(&schema, &mut audit);
        // Should not panic
    }
}
