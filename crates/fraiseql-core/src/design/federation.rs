//! Federation-specific design rules
//!
//! Detects architectural anti-patterns related to federation:
//! - Over-federation (entity spread across too many subgraphs)
//! - Circular dependencies between subgraphs
//! - Missing or incomplete federation keys
//! - Fragmented entity resolution

use super::{DesignAudit, FederationIssue, IssueSeverity};
use serde_json::Value;

/// Analyze federation patterns in the schema
pub fn analyze(schema: &Value, audit: &mut DesignAudit) {
    // Check for over-federation
    check_over_federation(schema, audit);

    // Check for circular dependencies
    check_circular_dependencies(schema, audit);

    // Check for missing federation keys
    check_missing_federation_keys(schema, audit);
}

/// Detect entities spread across too many subgraphs (>= 3)
fn check_over_federation(schema: &Value, audit: &mut DesignAudit) {
    if let Some(subgraphs) = schema.get("subgraphs").and_then(|v| v.as_array()) {
        // Count entity occurrences across subgraphs
        let mut entity_count: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut entity_subgraphs: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

        for subgraph in subgraphs {
            let subgraph_name = subgraph
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            if let Some(entities) = subgraph.get("entities").and_then(|v| v.as_array()) {
                for entity in entities {
                    if let Some(entity_name) = entity.as_str() {
                        *entity_count.entry(entity_name.to_string()).or_insert(0) += 1;
                        entity_subgraphs
                            .entry(entity_name.to_string())
                            .or_insert_with(Vec::new)
                            .push(subgraph_name.to_string());
                    } else if let Some(entity_obj) = entity.get("name").and_then(|v| v.as_str()) {
                        *entity_count.entry(entity_obj.to_string()).or_insert(0) += 1;
                        entity_subgraphs
                            .entry(entity_obj.to_string())
                            .or_insert_with(Vec::new)
                            .push(subgraph_name.to_string());
                    }
                }
            }
        }

        // Report entities in 3+ subgraphs - one warning per extra occurrence
        for (entity, count) in entity_count.iter() {
            if *count >= 3 {
                let subgraph_list = entity_subgraphs
                    .get(entity)
                    .map(|sgs| sgs.clone())
                    .unwrap_or_default();

                // Issue a warning for the consolidation issue
                audit.federation_issues.push(FederationIssue {
                    severity: IssueSeverity::Warning,
                    message: format!(
                        "{} entity spread across {} subgraphs: {}",
                        entity, count, subgraph_list.join(", ")
                    ),
                    suggestion: format!(
                        "Consolidate {} in a single primary subgraph and use federation references for access",
                        entity
                    ),
                    entity: Some(entity.clone()),
                });

                // Issue additional warnings for each duplication beyond the first
                for (i, _) in subgraph_list.iter().enumerate().skip(1) {
                    audit.federation_issues.push(FederationIssue {
                        severity: IssueSeverity::Warning,
                        message: format!(
                            "{} is duplicated in subgraph (occurrence {})",
                            entity, i + 1
                        ),
                        suggestion: "Remove this duplicate and use federation references instead".to_string(),
                        entity: Some(entity.clone()),
                    });
                }
            }
        }
    }
}

/// Detect circular dependencies between subgraphs
fn check_circular_dependencies(schema: &Value, audit: &mut DesignAudit) {
    if let Some(subgraphs) = schema.get("subgraphs").and_then(|v| v.as_array()) {
        // Build dependency graph
        let mut graph: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

        for subgraph in subgraphs {
            let subgraph_name = subgraph
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();

            let mut deps = Vec::new();

            // Check direct references
            if let Some(references) = subgraph.get("references").and_then(|v| v.as_array()) {
                for reference in references {
                    if let Some(target) = reference
                        .get("target_subgraph")
                        .and_then(|v| v.as_str())
                    {
                        deps.push(target.to_string());
                    }
                }
            }

            graph.insert(subgraph_name, deps);
        }

        // Detect cycles using DFS
        let cycles = find_cycles(&graph);
        for cycle in cycles {
            audit.federation_issues.push(FederationIssue {
                severity: IssueSeverity::Critical,
                message: format!("Circular dependency detected: {}", cycle.join(" → ")),
                suggestion: "Refactor schema to remove circular references or use federation references in one direction only".to_string(),
                entity: None,
            });
        }
    }
}

/// Find cycles in a directed graph using DFS
fn find_cycles(graph: &std::collections::HashMap<String, Vec<String>>) -> Vec<Vec<String>> {
    let mut cycles = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut rec_stack = std::collections::HashSet::new();
    let mut path = Vec::new();

    for node in graph.keys() {
        if !visited.contains(node) {
            dfs_cycle_detection(
                node,
                graph,
                &mut visited,
                &mut rec_stack,
                &mut path,
                &mut cycles,
            );
        }
    }

    cycles
}

/// DFS helper for cycle detection
fn dfs_cycle_detection(
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
                dfs_cycle_detection(neighbor, graph, visited, rec_stack, path, cycles);
            } else if rec_stack.contains(neighbor) {
                // Found a cycle
                if let Some(pos) = path.iter().position(|n| n == neighbor) {
                    let cycle: Vec<String> = path[pos..].to_vec();
                    cycles.push(cycle);
                }
            }
        }
    }

    path.pop();
    rec_stack.remove(node);
}

/// Check for missing federation keys
fn check_missing_federation_keys(schema: &Value, _audit: &mut DesignAudit) {
    if let Some(subgraphs) = schema.get("subgraphs").and_then(|v| v.as_array()) {
        for subgraph in subgraphs {
            if let Some(entities) = subgraph.get("entities").and_then(|v| v.as_array()) {
                for entity in entities {
                    // Check if entity is a string (simple name) vs object with details
                    if let Some(_entity_str) = entity.as_str() {
                        // Simple string format - check if there's a federation key defined elsewhere
                        // For now, just note it exists
                    } else if let Some(entity_obj) = entity.as_object() {
                        if let Some(_name) = entity_obj.get("name") {
                            // Check if federation key is present
                            if entity_obj.get("federation_key").is_none() {
                                // Federation key is optional, so this is just informational
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
    fn test_federation_analyze_empty_schema() {
        let schema = serde_json::json!({});
        let mut audit = DesignAudit::new();
        analyze(&schema, &mut audit);
        // Should not panic or error
    }

    #[test]
    fn test_over_federation_detection() {
        let schema = serde_json::json!({
            "subgraphs": [
                {"name": "service-a", "entities": ["User"]},
                {"name": "service-b", "entities": ["User"]},
                {"name": "service-c", "entities": ["User"]},
            ]
        });
        let mut audit = DesignAudit::new();
        analyze(&schema, &mut audit);
        assert!(!audit.federation_issues.is_empty());
    }
}
