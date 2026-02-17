//! Dependency graph construction and cycle detection for federation directives
//!
//! Builds a directed graph from @requires directives and provides cycle
//! detection and topological sort capabilities.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::federation::types::{FederationMetadata, FieldPathSelection};

/// Dependency graph node representing a field with @requires directives
#[derive(Debug, Clone)]
struct DependencyNode {
    /// Node ID: "TypeName.fieldName"
    id:       String,
    /// Type this field belongs to (kept for debugging and error messages)
    #[allow(dead_code)]
    typename: String,
    /// Field name (kept for debugging and error messages)
    #[allow(dead_code)]
    field:    String,
    /// Fields this node requires
    requires: Vec<FieldPathSelection>,
}

/// Directed edge in the dependency graph
#[derive(Debug, Clone)]
struct DependencyEdge {
    /// Source node (from)
    from: String,
    /// Target node (to)
    to:   String,
}

/// Dependency graph for federation @requires directives
///
/// Represents dependencies between fields based on @requires directives.
/// Used to detect cycles and determine resolution order.
pub struct DependencyGraph {
    /// All nodes in the graph
    nodes: HashMap<String, DependencyNode>,
    /// All edges in the graph
    edges: Vec<DependencyEdge>,
}

impl DependencyGraph {
    /// Build a dependency graph from federation metadata
    ///
    /// Scans all types and fields for @requires directives and constructs
    /// a directed graph where edges represent dependencies.
    ///
    /// # Errors
    ///
    /// Returns error if graph construction fails (should not happen in normal operation).
    pub fn build(metadata: &FederationMetadata) -> Result<Self, String> {
        let mut nodes: HashMap<String, DependencyNode> = HashMap::new();
        let mut edges = Vec::new();

        // Step 1: Build nodes for all fields with @requires directives
        for federated_type in &metadata.types {
            for (field_name, directives) in &federated_type.field_directives {
                // Only create nodes for fields that have @requires directives
                if !directives.requires.is_empty() {
                    let node_id = format!("{}.{}", federated_type.name, field_name);

                    nodes.insert(
                        node_id.clone(),
                        DependencyNode {
                            id:       node_id,
                            typename: federated_type.name.clone(),
                            field:    field_name.clone(),
                            requires: directives.requires.clone(),
                        },
                    );
                }
            }
        }

        // Step 2: Build edges based on @requires dependencies
        // For each node, create edges to the fields it requires
        for node in nodes.values() {
            for required in &node.requires {
                // The target is "TypeName.fieldName" for the required field
                let target_id = format!("{}.{}", required.typename, required.path.join("."));

                edges.push(DependencyEdge {
                    from: node.id.clone(),
                    to:   target_id,
                });
            }
        }

        Ok(DependencyGraph { nodes, edges })
    }

    /// Get the number of nodes in the graph
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Check if a node exists in the graph
    pub fn has_node(&self, node_id: &str) -> bool {
        self.nodes.contains_key(node_id)
    }

    /// Get all edges from a given node
    fn edges_from(&self, node_id: &str) -> Vec<&DependencyEdge> {
        self.edges.iter().filter(|e| e.from == node_id).collect()
    }

    /// Detect cycles in the dependency graph using DFS
    ///
    /// Returns a vector of cycles, where each cycle is a vector of node IDs
    /// that form a circular dependency.
    pub fn detect_cycles(&self) -> Vec<Vec<String>> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut cycles = Vec::new();

        for node_id in self.nodes.keys() {
            if !visited.contains(node_id) {
                self.dfs_cycle_detection(
                    node_id,
                    &mut visited,
                    &mut rec_stack,
                    &mut cycles,
                    &mut vec![],
                );
            }
        }

        cycles
    }

    /// DFS helper for cycle detection using recursion stack tracking
    fn dfs_cycle_detection(
        &self,
        node_id: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        cycles: &mut Vec<Vec<String>>,
        path: &mut Vec<String>,
    ) {
        visited.insert(node_id.to_string());
        rec_stack.insert(node_id.to_string());
        path.push(node_id.to_string());

        // Visit all neighbors (dependencies)
        for edge in self.edges_from(node_id) {
            let next = &edge.to;

            if !visited.contains(next) {
                self.dfs_cycle_detection(next, visited, rec_stack, cycles, path);
            } else if rec_stack.contains(next) {
                // Found a back edge - extract cycle from path
                if let Some(cycle_start) = path.iter().position(|x| x == next) {
                    let cycle: Vec<String> = path[cycle_start..].to_vec();
                    cycles.push(cycle);
                }
            }
        }

        rec_stack.remove(node_id);
        path.pop();
    }

    /// Perform topological sort using Kahn's algorithm
    ///
    /// # Errors
    ///
    /// Returns error if cycles are detected in the graph.
    pub fn topological_sort(&self) -> Result<Vec<String>, String> {
        // First check for cycles
        let cycles = self.detect_cycles();
        if !cycles.is_empty() {
            return Err(format!("Circular dependencies detected: {:?}", cycles));
        }

        // Compute in-degree for each node
        let mut in_degree: HashMap<String, usize> = HashMap::new();

        // Initialize in-degree to 0
        for node_id in self.nodes.keys() {
            in_degree.insert(node_id.clone(), 0);
        }

        // Count incoming edges (only for nodes that exist in the graph)
        for edge in &self.edges {
            if self.nodes.contains_key(&edge.to) {
                *in_degree.get_mut(&edge.to).unwrap() += 1;
            }
        }

        // Find nodes with no incoming edges
        let mut queue: VecDeque<String> = in_degree
            .iter()
            .filter(|(_, degree)| **degree == 0)
            .map(|(id, _)| id.clone())
            .collect();

        let mut result = Vec::new();

        // Process nodes in topological order
        while let Some(node_id) = queue.pop_front() {
            result.push(node_id.clone());

            // For each edge from this node
            for edge in self.edges_from(&node_id) {
                // Only process if target is in the graph
                if let Some(target_degree) = in_degree.get_mut(&edge.to) {
                    *target_degree -= 1;

                    // If target now has in-degree 0, add to queue
                    if *target_degree == 0 {
                        queue.push_back(edge.to.clone());
                    }
                }
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dependency_node_creation() {
        let node = DependencyNode {
            id:       "User.orders".to_string(),
            typename: "User".to_string(),
            field:    "orders".to_string(),
            requires: vec![FieldPathSelection {
                path:     vec!["email".to_string()],
                typename: "User".to_string(),
            }],
        };

        assert_eq!(node.id, "User.orders");
        assert_eq!(node.requires.len(), 1);
    }

    #[test]
    fn test_empty_graph() {
        let metadata = FederationMetadata {
            enabled: true,
            version: "v2".to_string(),
            types:   vec![],
        };

        let graph = DependencyGraph::build(&metadata).unwrap();
        assert_eq!(graph.node_count(), 0);
        assert!(graph.detect_cycles().is_empty());
    }
}
