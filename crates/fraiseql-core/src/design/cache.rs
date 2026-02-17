//! Cache coherency design rules
//!
//! Detects cache-related issues:
//! - TTL inconsistencies across subgraphs
//! - Missing cache directives on expensive fields
//! - Cache coherency violations

use serde_json::Value;

use super::{CacheIssue, DesignAudit, IssueSeverity};

/// Analyze cache patterns in the schema
pub fn analyze(schema: &Value, audit: &mut DesignAudit) {
    check_ttl_consistency(schema, audit);
    check_missing_cache_directives(schema, audit);
}

/// Detect TTL inconsistencies across subgraphs
fn check_ttl_consistency(schema: &Value, audit: &mut DesignAudit) {
    if let Some(subgraphs) = schema.get("subgraphs").and_then(|v| v.as_array()) {
        let mut entity_ttls: std::collections::HashMap<String, Vec<(String, u32)>> =
            std::collections::HashMap::new();

        for subgraph in subgraphs {
            let subgraph_name =
                subgraph.get("name").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();

            if let Some(entities) = subgraph.get("entities").and_then(|v| v.as_array()) {
                for entity in entities {
                    let entity_name = if let Some(name) = entity.as_str() {
                        name.to_string()
                    } else if let Some(name) = entity.get("name").and_then(|v| v.as_str()) {
                        name.to_string()
                    } else {
                        continue;
                    };

                    let ttl = entity.get("cache_ttl_seconds").and_then(|v| v.as_u64()).unwrap_or(0)
                        as u32;

                    entity_ttls
                        .entry(entity_name)
                        .or_insert_with(Vec::new)
                        .push((subgraph_name.clone(), ttl));
                }
            }
        }

        // Check for inconsistent TTLs
        for (entity, ttls) in entity_ttls {
            let mut unique_ttls: Vec<u32> = ttls.iter().map(|(_, ttl)| *ttl).collect();
            unique_ttls.sort_unstable();
            unique_ttls.dedup();

            if unique_ttls.len() > 1 && unique_ttls.iter().any(|&ttl| ttl > 0) {
                let ttl_list = ttls
                    .iter()
                    .map(|(sg, ttl)| format!("{}: {}s", sg, ttl))
                    .collect::<Vec<_>>()
                    .join(", ");

                audit.cache_issues.push(CacheIssue {
                    severity:   IssueSeverity::Warning,
                    message:    format!("TTL inconsistency for {}: {}", entity, ttl_list),
                    suggestion: format!(
                        "Synchronize cache TTL for {} across all subgraphs to prevent stale data",
                        entity
                    ),
                    affected:   Some(entity),
                });
            }
        }
    }
}

/// Detect missing cache directives on expensive fields
fn check_missing_cache_directives(schema: &Value, audit: &mut DesignAudit) {
    if let Some(types) = schema.get("types").and_then(|v| v.as_array()) {
        for type_def in types {
            if let Some(fields) = type_def.get("fields").and_then(|v| v.as_array()) {
                for field in fields {
                    let is_expensive =
                        field.get("is_expensive").and_then(|v| v.as_bool()).unwrap_or(false);

                    let has_cache =
                        field.get("has_cache_directive").and_then(|v| v.as_bool()).unwrap_or(false);

                    if is_expensive && !has_cache {
                        let field_name =
                            field.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
                        let type_name =
                            type_def.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");

                        audit.cache_issues.push(CacheIssue {
                            severity:   IssueSeverity::Info,
                            message:    format!(
                                "Expensive field {}.{} has no cache directive",
                                type_name, field_name
                            ),
                            suggestion: "Add @cache directive to reduce repeated computation"
                                .to_string(),
                            affected:   Some(format!("{}.{}", type_name, field_name)),
                        });
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
    fn test_cache_analyze_empty_schema() {
        let schema = serde_json::json!({});
        let mut audit = DesignAudit::new();
        analyze(&schema, &mut audit);
        // Should not panic
    }
}
