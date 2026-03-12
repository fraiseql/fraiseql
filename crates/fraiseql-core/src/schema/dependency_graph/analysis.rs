//! Analysis methods: cycle detection, unused types, impact analysis, transitive queries.

use std::collections::HashSet;

use super::{
    graph::SchemaDependencyGraph,
    types::{ChangeImpact, CyclePath},
};

impl SchemaDependencyGraph {
    /// Find all circular dependencies in the schema.
    ///
    /// Returns a list of cycle paths. Each cycle is reported once,
    /// starting from the lexicographically smallest type name.
    #[must_use]
    pub fn find_cycles(&self) -> Vec<CyclePath> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut cycles = Vec::new();

        for type_name in &self.all_types {
            if !visited.contains(type_name) {
                self.dfs_find_cycles(
                    type_name,
                    &mut visited,
                    &mut rec_stack,
                    &mut Vec::new(),
                    &mut cycles,
                );
            }
        }

        // Normalize and deduplicate cycles
        Self::normalize_cycles(cycles)
    }

    /// DFS helper for cycle detection.
    fn dfs_find_cycles(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
        cycles: &mut Vec<CyclePath>,
    ) {
        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());
        path.push(node.to_string());

        if let Some(deps) = self.outgoing.get(node) {
            for dep in deps {
                if !visited.contains(dep) {
                    self.dfs_find_cycles(dep, visited, rec_stack, path, cycles);
                } else if rec_stack.contains(dep) {
                    // Found a cycle - extract it from the path
                    if let Some(start_idx) = path.iter().position(|x| x == dep) {
                        let cycle_nodes: Vec<String> = path[start_idx..].to_vec();
                        cycles.push(CyclePath::new(cycle_nodes));
                    }
                }
            }
        }

        rec_stack.remove(node);
        path.pop();
    }

    /// Normalize cycles so each cycle starts from its lexicographically smallest node,
    /// and deduplicate equivalent cycles.
    fn normalize_cycles(mut cycles: Vec<CyclePath>) -> Vec<CyclePath> {
        // Normalize each cycle to start from the smallest element
        for cycle in &mut cycles {
            if cycle.nodes.is_empty() {
                continue;
            }
            // Find the position of the minimum element
            let min_pos = cycle
                .nodes
                .iter()
                .enumerate()
                .min_by_key(|(_, name)| *name)
                .map_or(0, |(i, _)| i);

            // Rotate so minimum is first
            cycle.nodes.rotate_left(min_pos);
        }

        // Sort by nodes for consistent ordering
        cycles.sort_by(|a, b| a.nodes.cmp(&b.nodes));

        // Deduplicate
        cycles.dedup();

        cycles
    }

    /// Find all types that have no incoming references (orphaned types).
    ///
    /// Root types (Query, Mutation, Subscription) are excluded from this list
    /// as they are always considered used.
    #[must_use]
    pub fn find_unused(&self) -> Vec<String> {
        let mut unused = Vec::new();

        for type_name in &self.all_types {
            // Skip root types - they're always used
            if self.root_types.contains(type_name) {
                continue;
            }

            // Check if any type references this one
            let has_references =
                self.incoming.get(type_name).is_some_and(|refs| !refs.is_empty());

            if !has_references {
                unused.push(type_name.clone());
            }
        }

        unused.sort();
        unused
    }

    /// Analyze the impact of deleting a type from the schema.
    ///
    /// Returns all types that would be affected (transitively) by removing
    /// the specified type.
    #[must_use]
    pub fn impact_of_deletion(&self, type_name: &str) -> ChangeImpact {
        let mut affected = HashSet::new();
        let mut breaking_changes = Vec::new();

        // Find all types that depend on this type (directly or transitively)
        let mut to_visit = vec![type_name.to_string()];
        let mut visited = HashSet::new();

        while let Some(current) = to_visit.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }

            // Get types that depend on current
            if let Some(dependents) = self.incoming.get(&current) {
                for dependent in dependents {
                    if !visited.contains(dependent) {
                        affected.insert(dependent.clone());
                        to_visit.push(dependent.clone());
                        breaking_changes.push(format!(
                            "Type '{}' references '{}' which would be deleted",
                            dependent, type_name
                        ));
                    }
                }
            }
        }

        // Remove the type itself from affected (it's the one being deleted)
        affected.remove(type_name);

        ChangeImpact::new(affected, breaking_changes)
    }

    /// Get transitive dependencies of a type (all types it depends on, recursively).
    #[must_use]
    pub fn transitive_dependencies(&self, type_name: &str) -> HashSet<String> {
        let mut visited = HashSet::new();
        let mut to_visit = vec![type_name.to_string()];

        while let Some(current) = to_visit.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }

            if let Some(deps) = self.outgoing.get(&current) {
                for dep in deps {
                    if !visited.contains(dep) {
                        to_visit.push(dep.clone());
                    }
                }
            }
        }

        // Remove the starting type
        visited.remove(type_name);
        visited
    }

    /// Get transitive dependents of a type (all types that depend on it, recursively).
    #[must_use]
    pub fn transitive_dependents(&self, type_name: &str) -> HashSet<String> {
        let mut visited = HashSet::new();
        let mut to_visit = vec![type_name.to_string()];

        while let Some(current) = to_visit.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }

            if let Some(refs) = self.incoming.get(&current) {
                for ref_type in refs {
                    if !visited.contains(ref_type) {
                        to_visit.push(ref_type.clone());
                    }
                }
            }
        }

        // Remove the starting type
        visited.remove(type_name);
        visited
    }
}
