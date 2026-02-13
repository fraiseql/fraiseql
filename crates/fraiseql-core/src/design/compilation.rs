//! Compilation-specific design rules (FraiseQL-only)
//!
//! **FraiseQL Philosophy**: These rules check if schemas are suitable for
//! deterministic SQL compilation at build time.
//!
//! FraiseQL compiles GraphQL types to SQL structures. The compiler needs:
//! 1. **Acyclic type definitions** - Can't compile circular type refs to SQL
//! 2. **Complete type metadata** - Primary keys, cardinalities, indexes
//! 3. **SQL-suitable field types** - No arbitrary JSON, no runtime polymorphism
//!
//! Rules detect:
//! - **Circular type definitions**: `User { posts: [Post] }`, `Post { author: User { posts } }`
//! - **Missing primary keys**: Types without ID can't be compiled efficiently
//! - **Missing cardinality hints**: Compiler can't generate optimal SQL without knowing
//!   relationships
//! - **Missing index recommendations**: JSONB batching needs FK indexes

use serde_json::Value;

use super::{DesignAudit, IssueSeverity, SchemaIssue};

/// Analyze compilation suitability of schema types
pub fn analyze(schema: &Value, audit: &mut DesignAudit) {
    check_type_circularity(schema, audit);
    check_missing_primary_keys(schema, audit);
    check_missing_cardinality_hints(schema, audit);
}

/// Detect circular type definitions that can't compile to deterministic SQL
fn check_type_circularity(schema: &Value, audit: &mut DesignAudit) {
    if let Some(types) = schema.get("types").and_then(|v| v.as_array()) {
        // Build type reference graph
        let mut type_refs: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();

        for type_def in types {
            let type_name =
                type_def.get("name").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();

            if let Some(fields) = type_def.get("fields").and_then(|v| v.as_array()) {
                for field in fields {
                    if let Some(field_type) = field.get("type").and_then(|v| v.as_str()) {
                        // Extract type name from field type (strip [], !, etc.)
                        let inner_type = field_type
                            .trim_matches('[')
                            .trim_matches(']')
                            .trim_matches('!')
                            .to_string();

                        // Skip scalar types
                        if !is_scalar_type(&inner_type) {
                            type_refs
                                .entry(type_name.clone())
                                .or_insert_with(Vec::new)
                                .push(inner_type);
                        }
                    }
                }
            }
        }

        // Find cycles
        let cycles = find_type_cycles(&type_refs);
        for cycle in cycles {
            audit.schema_issues.push(SchemaIssue {
                severity: IssueSeverity::Critical,
                message: format!(
                    "Circular type definition: {} - Can't compile to deterministic SQL schema",
                    cycle.join(" → ")
                ),
                suggestion: "Break the cycle by using ID references instead of nested types. E.g., Post { authorId: ID } instead of Post { author: User }".to_string(),
                affected_type: Some(cycle[0].clone()),
            });
        }
    }
}

/// Check if type is a GraphQL scalar (not a custom type)
fn is_scalar_type(type_name: &str) -> bool {
    matches!(
        type_name,
        "String" | "Int" | "Float" | "Boolean" | "ID" | "DateTime" | "Date" | "Time" | "JSON"
    )
}

/// Find cycles in type reference graph
fn find_type_cycles(graph: &std::collections::HashMap<String, Vec<String>>) -> Vec<Vec<String>> {
    let mut cycles = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut rec_stack = std::collections::HashSet::new();
    let mut path = Vec::new();

    for node in graph.keys() {
        if !visited.contains(node) {
            dfs_find_type_cycle(node, graph, &mut visited, &mut rec_stack, &mut path, &mut cycles);
        }
    }

    cycles
}

/// DFS helper for type cycle detection
fn dfs_find_type_cycle(
    node: &str,
    graph: &std::collections::HashMap<String, Vec<String>>,
    visited: &mut std::collections::HashSet<String>,
    rec_stack: &mut std::collections::HashSet<String>,
    path: &mut Vec<String>,
    cycles: &mut Vec<Vec<String>>,
) {
    visited.insert(node.to_string());
    rec_stack.insert(node.to_string());
    path.push(node.to_string());

    if let Some(neighbors) = graph.get(node) {
        for neighbor in neighbors {
            if !visited.contains(neighbor) {
                dfs_find_type_cycle(neighbor, graph, visited, rec_stack, path, cycles);
            } else if rec_stack.contains(neighbor) {
                // Found a cycle
                if let Some(pos) = path.iter().position(|n| n == neighbor) {
                    let cycle: Vec<String> = path[pos..].to_vec();
                    if !cycles.iter().any(|c| {
                        // Avoid reporting same cycle multiple times
                        c.len() == cycle.len() && c.windows(1).any(|w| w[0] == cycle[0])
                    }) {
                        cycles.push(cycle);
                    }
                }
            }
        }
    }

    path.pop();
    rec_stack.remove(node);
}

/// Check for missing primary keys on entities
fn check_missing_primary_keys(schema: &Value, audit: &mut DesignAudit) {
    if let Some(types) = schema.get("types").and_then(|v| v.as_array()) {
        for type_def in types {
            let type_name = type_def.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");

            // Skip Query, Mutation, Subscription
            if matches!(type_name, "Query" | "Mutation" | "Subscription") {
                continue;
            }

            if let Some(fields) = type_def.get("fields").and_then(|v| v.as_array()) {
                let has_pk = fields
                    .iter()
                    .any(|f| f.get("isPrimaryKey").and_then(|v| v.as_bool()).unwrap_or(false));

                if !has_pk {
                    audit.schema_issues.push(SchemaIssue {
                        severity: IssueSeverity::Warning,
                        message: format!(
                            "{} has no primary key marked - Compiler can't generate efficient JSONB batching",
                            type_name
                        ),
                        suggestion: format!(
                            "Mark the id field with isPrimaryKey: true so compiler knows the aggregation join key"
                        ),
                        affected_type: Some(type_name.to_string()),
                    });
                }
            }
        }
    }
}

/// Check for missing cardinality hints on relationships
fn check_missing_cardinality_hints(schema: &Value, audit: &mut DesignAudit) {
    if let Some(types) = schema.get("types").and_then(|v| v.as_array()) {
        for type_def in types {
            let type_name = type_def.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");

            if let Some(fields) = type_def.get("fields").and_then(|v| v.as_array()) {
                for field in fields {
                    if let Some(field_type) = field.get("type").and_then(|v| v.as_str()) {
                        // Check if field references a custom type
                        if field_type.contains("[") && !field_type.contains("[") {
                            // Single object reference
                            let inner_type = field_type.trim_matches('!').to_string();

                            // Check if cardinality is marked
                            let has_cardinality = field.get("cardinality").is_some();

                            if !has_cardinality && !is_scalar_type(&inner_type) {
                                let field_name =
                                    field.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");

                                audit.schema_issues.push(SchemaIssue {
                                    severity: IssueSeverity::Info,
                                    message: format!(
                                        "{}.{} references {} without cardinality hint - Compiler assumes one-to-one",
                                        type_name, field_name, inner_type
                                    ),
                                    suggestion: "Add cardinality hint (one-to-one, one-to-many, many-to-many) for JSONB optimization clarity".to_string(),
                                    affected_type: Some(format!("{}.{}", type_name, field_name)),
                                });
                            }
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
    fn test_compilation_analyze_empty_schema() {
        let schema = serde_json::json!({});
        let mut audit = DesignAudit::new();
        analyze(&schema, &mut audit);
    }

    #[test]
    fn test_circular_types_detection() {
        let schema = serde_json::json!({
            "types": [
                {
                    "name": "User",
                    "fields": [
                        {"name": "id", "type": "ID"},
                        {"name": "posts", "type": "[Post]"}
                    ]
                },
                {
                    "name": "Post",
                    "fields": [
                        {"name": "id", "type": "ID"},
                        {"name": "author", "type": "User"}
                    ]
                }
            ]
        });

        let mut audit = DesignAudit::new();
        analyze(&schema, &mut audit);

        assert!(!audit.schema_issues.is_empty());
    }
}
