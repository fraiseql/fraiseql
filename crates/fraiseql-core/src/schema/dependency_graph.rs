//! Schema dependency graph analysis.
//!
//! This module provides tools for analyzing type dependencies in a compiled schema,
//! including cycle detection, unused type detection, and impact analysis.
//!
//! # Example
//!
//! ```
//! use fraiseql_core::schema::{CompiledSchema, SchemaDependencyGraph};
//!
//! let schema = CompiledSchema::default();
//! let graph = SchemaDependencyGraph::build(&schema);
//!
//! // Check for circular dependencies
//! let cycles = graph.find_cycles();
//! if !cycles.is_empty() {
//!     for cycle in &cycles {
//!         println!("Cycle detected: {}", cycle.path_string());
//!     }
//! }
//!
//! // Find unused types
//! let unused = graph.find_unused();
//! for type_name in &unused {
//!     println!("Unused type: {}", type_name);
//! }
//! ```

use std::collections::{HashMap, HashSet};

use super::{CompiledSchema, FieldType};

/// A path representing a circular dependency in the schema.
///
/// Contains the sequence of type names that form the cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CyclePath {
    /// Type names in the cycle, in dependency order.
    /// For a cycle A → B → C → A, this would be `["A", "B", "C"]`.
    pub nodes: Vec<String>,
}

impl CyclePath {
    /// Create a new cycle path from a list of nodes.
    #[must_use]
    pub fn new(nodes: Vec<String>) -> Self {
        Self { nodes }
    }

    /// Format the cycle as a readable path string.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::CyclePath;
    ///
    /// let cycle = CyclePath::new(vec!["A".to_string(), "B".to_string(), "C".to_string()]);
    /// assert_eq!(cycle.path_string(), "A → B → C → A");
    /// ```
    #[must_use]
    pub fn path_string(&self) -> String {
        if self.nodes.is_empty() {
            return String::new();
        }
        let mut path = self.nodes.join(" → ");
        // Add the first node at the end to show it's a cycle
        path.push_str(" → ");
        path.push_str(&self.nodes[0]);
        path
    }

    /// Check if this is a self-referencing cycle (single node).
    #[must_use]
    pub fn is_self_reference(&self) -> bool {
        self.nodes.len() == 1
    }

    /// Get the length of the cycle (number of types involved).
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if the cycle is empty (should never happen in practice).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// Result of analyzing the impact of deleting or modifying a type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeImpact {
    /// Types that would be affected by this change.
    pub affected_types: HashSet<String>,
    /// Human-readable descriptions of breaking changes.
    pub breaking_changes: Vec<String>,
}

impl ChangeImpact {
    /// Create a new change impact result.
    #[must_use]
    pub fn new(affected_types: HashSet<String>, breaking_changes: Vec<String>) -> Self {
        Self {
            affected_types,
            breaking_changes,
        }
    }

    /// Check if this change has any impact.
    #[must_use]
    pub fn has_impact(&self) -> bool {
        !self.affected_types.is_empty()
    }
}

/// Schema dependency graph for analyzing type relationships.
///
/// This graph tracks which types depend on which other types, enabling:
/// - Circular dependency detection
/// - Unused type detection
/// - Impact analysis for schema changes
#[derive(Debug, Clone)]
pub struct SchemaDependencyGraph {
    /// Map of type name to types it depends on (outgoing edges).
    outgoing: HashMap<String, HashSet<String>>,
    /// Map of type name to types that depend on it (incoming edges).
    incoming: HashMap<String, HashSet<String>>,
    /// All type names in the schema.
    all_types: HashSet<String>,
    /// Root types that are always considered "used" (Query, Mutation, Subscription).
    root_types: HashSet<String>,
}

impl SchemaDependencyGraph {
    /// Build a dependency graph from a compiled schema.
    ///
    /// This analyzes all types, queries, mutations, and subscriptions to
    /// build a complete dependency graph.
    #[must_use]
    pub fn build(schema: &CompiledSchema) -> Self {
        let mut outgoing: HashMap<String, HashSet<String>> = HashMap::new();
        let mut incoming: HashMap<String, HashSet<String>> = HashMap::new();
        let mut all_types: HashSet<String> = HashSet::new();
        let mut root_types: HashSet<String> = HashSet::new();

        // Collect all type names first
        for type_def in &schema.types {
            all_types.insert(type_def.name.clone());
            outgoing.insert(type_def.name.clone(), HashSet::new());
            incoming.insert(type_def.name.clone(), HashSet::new());
        }

        for enum_def in &schema.enums {
            all_types.insert(enum_def.name.clone());
            outgoing.insert(enum_def.name.clone(), HashSet::new());
            incoming.insert(enum_def.name.clone(), HashSet::new());
        }

        for input_def in &schema.input_types {
            all_types.insert(input_def.name.clone());
            outgoing.insert(input_def.name.clone(), HashSet::new());
            incoming.insert(input_def.name.clone(), HashSet::new());
        }

        for interface_def in &schema.interfaces {
            all_types.insert(interface_def.name.clone());
            outgoing.insert(interface_def.name.clone(), HashSet::new());
            incoming.insert(interface_def.name.clone(), HashSet::new());
        }

        for union_def in &schema.unions {
            all_types.insert(union_def.name.clone());
            outgoing.insert(union_def.name.clone(), HashSet::new());
            incoming.insert(union_def.name.clone(), HashSet::new());
        }

        // Add virtual root types for operations
        if !schema.queries.is_empty() {
            root_types.insert("Query".to_string());
            all_types.insert("Query".to_string());
            outgoing.insert("Query".to_string(), HashSet::new());
            incoming.insert("Query".to_string(), HashSet::new());
        }
        if !schema.mutations.is_empty() {
            root_types.insert("Mutation".to_string());
            all_types.insert("Mutation".to_string());
            outgoing.insert("Mutation".to_string(), HashSet::new());
            incoming.insert("Mutation".to_string(), HashSet::new());
        }
        if !schema.subscriptions.is_empty() {
            root_types.insert("Subscription".to_string());
            all_types.insert("Subscription".to_string());
            outgoing.insert("Subscription".to_string(), HashSet::new());
            incoming.insert("Subscription".to_string(), HashSet::new());
        }

        // Build dependencies for object types
        for type_def in &schema.types {
            for field in &type_def.fields {
                if let Some(ref_type) = Self::extract_referenced_type(&field.field_type) {
                    if all_types.contains(&ref_type) {
                        outgoing.get_mut(&type_def.name).unwrap().insert(ref_type.clone());
                        incoming.get_mut(&ref_type).unwrap().insert(type_def.name.clone());
                    }
                }
            }

            // Track interface implementations
            for interface_name in &type_def.implements {
                if all_types.contains(interface_name) {
                    outgoing
                        .get_mut(&type_def.name)
                        .unwrap()
                        .insert(interface_name.clone());
                    incoming
                        .get_mut(interface_name)
                        .unwrap()
                        .insert(type_def.name.clone());
                }
            }
        }

        // Build dependencies for interfaces
        for interface_def in &schema.interfaces {
            for field in &interface_def.fields {
                if let Some(ref_type) = Self::extract_referenced_type(&field.field_type) {
                    if all_types.contains(&ref_type) {
                        outgoing
                            .get_mut(&interface_def.name)
                            .unwrap()
                            .insert(ref_type.clone());
                        incoming
                            .get_mut(&ref_type)
                            .unwrap()
                            .insert(interface_def.name.clone());
                    }
                }
            }
        }

        // Build dependencies for unions
        for union_def in &schema.unions {
            for member_type in &union_def.member_types {
                if all_types.contains(member_type) {
                    outgoing
                        .get_mut(&union_def.name)
                        .unwrap()
                        .insert(member_type.clone());
                    incoming
                        .get_mut(member_type)
                        .unwrap()
                        .insert(union_def.name.clone());
                }
            }
        }

        // Build dependencies for input types (they can reference other input types)
        for input_def in &schema.input_types {
            for field in &input_def.fields {
                // Parse the field_type string to extract type references
                let parsed = FieldType::parse(&field.field_type, &all_types);
                if let Some(ref_type) = Self::extract_referenced_type(&parsed) {
                    if all_types.contains(&ref_type) {
                        outgoing.get_mut(&input_def.name).unwrap().insert(ref_type.clone());
                        incoming.get_mut(&ref_type).unwrap().insert(input_def.name.clone());
                    }
                }
            }
        }

        // Build dependencies from queries to their return types
        for query in &schema.queries {
            let parsed = FieldType::parse(&query.return_type, &all_types);
            if let Some(ref_type) = Self::extract_referenced_type(&parsed) {
                if all_types.contains(&ref_type) {
                    outgoing.get_mut("Query").unwrap().insert(ref_type.clone());
                    incoming.get_mut(&ref_type).unwrap().insert("Query".to_string());
                }
            }
        }

        // Build dependencies from mutations to their return types
        for mutation in &schema.mutations {
            let parsed = FieldType::parse(&mutation.return_type, &all_types);
            if let Some(ref_type) = Self::extract_referenced_type(&parsed) {
                if all_types.contains(&ref_type) {
                    outgoing.get_mut("Mutation").unwrap().insert(ref_type.clone());
                    incoming.get_mut(&ref_type).unwrap().insert("Mutation".to_string());
                }
            }
        }

        // Build dependencies from subscriptions to their return types
        for subscription in &schema.subscriptions {
            let parsed = FieldType::parse(&subscription.return_type, &all_types);
            if let Some(ref_type) = Self::extract_referenced_type(&parsed) {
                if all_types.contains(&ref_type) {
                    outgoing
                        .get_mut("Subscription")
                        .unwrap()
                        .insert(ref_type.clone());
                    incoming
                        .get_mut(&ref_type)
                        .unwrap()
                        .insert("Subscription".to_string());
                }
            }
        }

        Self {
            outgoing,
            incoming,
            all_types,
            root_types,
        }
    }

    /// Extract the referenced type name from a `FieldType`, recursively unwrapping lists.
    fn extract_referenced_type(field_type: &FieldType) -> Option<String> {
        match field_type {
            FieldType::Object(name)
            | FieldType::Enum(name)
            | FieldType::Input(name)
            | FieldType::Interface(name)
            | FieldType::Union(name) => Some(name.clone()),
            FieldType::List(inner) => Self::extract_referenced_type(inner),
            _ => None, // Scalars don't create dependencies
        }
    }

    /// Get all types that a given type depends on (outgoing edges).
    #[must_use]
    pub fn dependencies_of(&self, type_name: &str) -> Vec<String> {
        self.outgoing
            .get(type_name)
            .map(|deps| {
                let mut v: Vec<_> = deps.iter().cloned().collect();
                v.sort();
                v
            })
            .unwrap_or_default()
    }

    /// Get all types that depend on a given type (incoming edges).
    #[must_use]
    pub fn dependents_of(&self, type_name: &str) -> Vec<String> {
        self.incoming
            .get(type_name)
            .map(|refs| {
                let mut v: Vec<_> = refs.iter().cloned().collect();
                v.sort();
                v
            })
            .unwrap_or_default()
    }

    /// Get all type names in the graph.
    #[must_use]
    pub fn all_types(&self) -> Vec<String> {
        let mut types: Vec<_> = self.all_types.iter().cloned().collect();
        types.sort();
        types
    }

    /// Get the total number of types in the graph.
    #[must_use]
    pub fn type_count(&self) -> usize {
        self.all_types.len()
    }

    /// Check if a type exists in the graph.
    #[must_use]
    pub fn has_type(&self, type_name: &str) -> bool {
        self.all_types.contains(type_name)
    }

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
                .map(|(i, _)| i)
                .unwrap_or(0);

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
            let has_references = self
                .incoming
                .get(type_name)
                .map(|refs| !refs.is_empty())
                .unwrap_or(false);

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{
        EnumDefinition, EnumValueDefinition, FieldDefinition, InputFieldDefinition,
        InputObjectDefinition, InterfaceDefinition, MutationDefinition, QueryDefinition,
        SubscriptionDefinition, TypeDefinition, UnionDefinition,
    };

    /// Helper to create a simple type with the given fields.
    fn make_type(name: &str, fields: Vec<(&str, FieldType)>) -> TypeDefinition {
        TypeDefinition {
            name:                name.to_string(),
            sql_source:          format!("v_{}", name.to_lowercase()),
            jsonb_column:        "data".to_string(),
            fields:              fields
                .into_iter()
                .map(|(n, ft)| FieldDefinition::new(n, ft))
                .collect(),
            description:         None,
            sql_projection_hint: None,
            implements:          vec![],
        }
    }

    // =========================================================================
    // Basic Graph Construction Tests
    // =========================================================================

    #[test]
    fn test_empty_schema() {
        let schema = CompiledSchema::default();
        let graph = SchemaDependencyGraph::build(&schema);

        assert_eq!(graph.type_count(), 0);
        assert!(graph.find_cycles().is_empty());
        assert!(graph.find_unused().is_empty());
    }

    #[test]
    fn test_single_type_no_dependencies() {
        let schema = CompiledSchema {
            types: vec![make_type(
                "User",
                vec![
                    ("id", FieldType::Id),
                    ("name", FieldType::String),
                    ("email", FieldType::String),
                ],
            )],
            queries: vec![QueryDefinition::new("users", "User").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        assert!(graph.has_type("User"));
        assert!(graph.has_type("Query"));
        assert_eq!(graph.dependencies_of("User").len(), 0);
        assert_eq!(graph.dependents_of("User"), vec!["Query"]);
    }

    #[test]
    fn test_type_with_object_reference() {
        let schema = CompiledSchema {
            types: vec![
                make_type(
                    "User",
                    vec![
                        ("id", FieldType::Id),
                        ("profile", FieldType::Object("Profile".to_string())),
                    ],
                ),
                make_type("Profile", vec![("bio", FieldType::String)]),
            ],
            queries: vec![QueryDefinition::new("users", "User").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        // User depends on Profile
        assert_eq!(graph.dependencies_of("User"), vec!["Profile"]);
        // Profile is referenced by User
        assert_eq!(graph.dependents_of("Profile"), vec!["User"]);
    }

    #[test]
    fn test_type_with_list_reference() {
        let schema = CompiledSchema {
            types: vec![
                make_type(
                    "User",
                    vec![
                        ("id", FieldType::Id),
                        (
                            "posts",
                            FieldType::List(Box::new(FieldType::Object("Post".to_string()))),
                        ),
                    ],
                ),
                make_type("Post", vec![("title", FieldType::String)]),
            ],
            queries: vec![QueryDefinition::new("users", "User").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        // User depends on Post (through list)
        assert_eq!(graph.dependencies_of("User"), vec!["Post"]);
        assert_eq!(graph.dependents_of("Post"), vec!["User"]);
    }

    #[test]
    fn test_enum_reference() {
        let schema = CompiledSchema {
            types: vec![make_type(
                "User",
                vec![
                    ("id", FieldType::Id),
                    ("status", FieldType::Enum("UserStatus".to_string())),
                ],
            )],
            enums: vec![EnumDefinition {
                name:        "UserStatus".to_string(),
                values:      vec![
                    EnumValueDefinition::new("ACTIVE"),
                    EnumValueDefinition::new("INACTIVE"),
                ],
                description: None,
            }],
            queries: vec![QueryDefinition::new("users", "User").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        assert!(graph.has_type("UserStatus"));
        assert_eq!(graph.dependencies_of("User"), vec!["UserStatus"]);
        assert_eq!(graph.dependents_of("UserStatus"), vec!["User"]);
    }

    // =========================================================================
    // Cycle Detection Tests
    // =========================================================================

    #[test]
    fn test_no_cycles() {
        let schema = CompiledSchema {
            types: vec![
                make_type(
                    "User",
                    vec![("profile", FieldType::Object("Profile".to_string()))],
                ),
                make_type("Profile", vec![("bio", FieldType::String)]),
            ],
            queries: vec![QueryDefinition::new("users", "User").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);
        let cycles = graph.find_cycles();

        assert!(cycles.is_empty());
    }

    #[test]
    fn test_self_referencing_cycle() {
        let schema = CompiledSchema {
            types: vec![make_type(
                "Node",
                vec![
                    ("id", FieldType::Id),
                    ("next", FieldType::Object("Node".to_string())),
                ],
            )],
            queries: vec![QueryDefinition::new("nodes", "Node").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);
        let cycles = graph.find_cycles();

        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].nodes, vec!["Node"]);
        assert!(cycles[0].is_self_reference());
        assert_eq!(cycles[0].path_string(), "Node → Node");
    }

    #[test]
    fn test_two_node_cycle() {
        let schema = CompiledSchema {
            types: vec![
                make_type("A", vec![("b", FieldType::Object("B".to_string()))]),
                make_type("B", vec![("a", FieldType::Object("A".to_string()))]),
            ],
            queries: vec![QueryDefinition::new("items", "A").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);
        let cycles = graph.find_cycles();

        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].len(), 2);
        // Normalized to start from "A"
        assert_eq!(cycles[0].nodes, vec!["A", "B"]);
        assert_eq!(cycles[0].path_string(), "A → B → A");
    }

    #[test]
    fn test_three_node_cycle() {
        let schema = CompiledSchema {
            types: vec![
                make_type("A", vec![("b", FieldType::Object("B".to_string()))]),
                make_type("B", vec![("c", FieldType::Object("C".to_string()))]),
                make_type("C", vec![("a", FieldType::Object("A".to_string()))]),
            ],
            queries: vec![QueryDefinition::new("items", "A").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);
        let cycles = graph.find_cycles();

        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].len(), 3);
        assert_eq!(cycles[0].nodes, vec!["A", "B", "C"]);
        assert_eq!(cycles[0].path_string(), "A → B → C → A");
    }

    #[test]
    fn test_multiple_independent_cycles() {
        let schema = CompiledSchema {
            types: vec![
                // Cycle 1: A <-> B
                make_type("A", vec![("b", FieldType::Object("B".to_string()))]),
                make_type("B", vec![("a", FieldType::Object("A".to_string()))]),
                // Cycle 2: X <-> Y
                make_type("X", vec![("y", FieldType::Object("Y".to_string()))]),
                make_type("Y", vec![("x", FieldType::Object("X".to_string()))]),
            ],
            queries: vec![
                QueryDefinition::new("aItems", "A").returning_list(),
                QueryDefinition::new("xItems", "X").returning_list(),
            ],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);
        let cycles = graph.find_cycles();

        assert_eq!(cycles.len(), 2);
    }

    // =========================================================================
    // Unused Type Detection Tests
    // =========================================================================

    #[test]
    fn test_no_unused_types() {
        let schema = CompiledSchema {
            types: vec![
                make_type(
                    "User",
                    vec![("profile", FieldType::Object("Profile".to_string()))],
                ),
                make_type("Profile", vec![("bio", FieldType::String)]),
            ],
            queries: vec![QueryDefinition::new("users", "User").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);
        let unused = graph.find_unused();

        assert!(unused.is_empty());
    }

    #[test]
    fn test_unused_type_no_references() {
        let schema = CompiledSchema {
            types: vec![
                make_type("User", vec![("name", FieldType::String)]),
                make_type("OrphanType", vec![("data", FieldType::String)]),
            ],
            queries: vec![QueryDefinition::new("users", "User").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);
        let unused = graph.find_unused();

        assert_eq!(unused, vec!["OrphanType"]);
    }

    #[test]
    fn test_multiple_unused_types() {
        let schema = CompiledSchema {
            types: vec![
                make_type("User", vec![("name", FieldType::String)]),
                make_type("Orphan1", vec![("data", FieldType::String)]),
                make_type("Orphan2", vec![("data", FieldType::String)]),
            ],
            queries: vec![QueryDefinition::new("users", "User").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);
        let unused = graph.find_unused();

        assert_eq!(unused, vec!["Orphan1", "Orphan2"]);
    }

    #[test]
    fn test_root_types_never_unused() {
        // Query type exists but has no incoming references (it's a root)
        let schema = CompiledSchema {
            types: vec![make_type("User", vec![("name", FieldType::String)])],
            queries: vec![QueryDefinition::new("users", "User").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);
        let unused = graph.find_unused();

        // Query should NOT appear in unused list (it's a root type)
        assert!(!unused.contains(&"Query".to_string()));
    }

    // =========================================================================
    // Impact Analysis Tests
    // =========================================================================

    #[test]
    fn test_impact_of_deletion_no_dependents() {
        let schema = CompiledSchema {
            types: vec![
                make_type(
                    "User",
                    vec![("profile", FieldType::Object("Profile".to_string()))],
                ),
                make_type("Profile", vec![("bio", FieldType::String)]),
            ],
            queries: vec![QueryDefinition::new("users", "User").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        // Deleting Profile affects User (and Query transitively)
        let impact = graph.impact_of_deletion("Profile");
        assert!(impact.has_impact());
        assert!(impact.affected_types.contains("User"));
    }

    #[test]
    fn test_impact_of_deletion_chain() {
        let schema = CompiledSchema {
            types: vec![
                make_type("A", vec![("b", FieldType::Object("B".to_string()))]),
                make_type("B", vec![("c", FieldType::Object("C".to_string()))]),
                make_type("C", vec![("d", FieldType::Object("D".to_string()))]),
                make_type("D", vec![("value", FieldType::String)]),
            ],
            queries: vec![QueryDefinition::new("items", "A").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        // Deleting D affects C, B, A (and Query)
        let impact = graph.impact_of_deletion("D");
        assert!(impact.affected_types.contains("C"));
        assert!(impact.affected_types.contains("B"));
        assert!(impact.affected_types.contains("A"));
    }

    // =========================================================================
    // Transitive Dependency Tests
    // =========================================================================

    #[test]
    fn test_transitive_dependencies() {
        let schema = CompiledSchema {
            types: vec![
                make_type("A", vec![("b", FieldType::Object("B".to_string()))]),
                make_type("B", vec![("c", FieldType::Object("C".to_string()))]),
                make_type("C", vec![("value", FieldType::String)]),
            ],
            queries: vec![QueryDefinition::new("items", "A").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        let deps = graph.transitive_dependencies("A");
        assert!(deps.contains("B"));
        assert!(deps.contains("C"));
        assert!(!deps.contains("A")); // Should not include self
    }

    #[test]
    fn test_transitive_dependents() {
        let schema = CompiledSchema {
            types: vec![
                make_type("A", vec![("b", FieldType::Object("B".to_string()))]),
                make_type("B", vec![("c", FieldType::Object("C".to_string()))]),
                make_type("C", vec![("value", FieldType::String)]),
            ],
            queries: vec![QueryDefinition::new("items", "A").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        let refs = graph.transitive_dependents("C");
        assert!(refs.contains("B"));
        assert!(refs.contains("A"));
        assert!(refs.contains("Query"));
        assert!(!refs.contains("C")); // Should not include self
    }

    // =========================================================================
    // Interface and Union Tests
    // =========================================================================

    #[test]
    fn test_interface_dependencies() {
        let schema = CompiledSchema {
            types: vec![TypeDefinition {
                name:                "User".to_string(),
                sql_source:          "v_user".to_string(),
                jsonb_column:        "data".to_string(),
                fields:              vec![FieldDefinition::new("id", FieldType::Id)],
                description:         None,
                sql_projection_hint: None,
                implements:          vec!["Node".to_string()],
            }],
            interfaces: vec![InterfaceDefinition {
                name:        "Node".to_string(),
                fields:      vec![FieldDefinition::new("id", FieldType::Id)],
                description: None,
            }],
            queries: vec![QueryDefinition::new("users", "User").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        // User depends on Node (implements it)
        assert!(graph.dependencies_of("User").contains(&"Node".to_string()));
        // Node is referenced by User
        assert!(graph.dependents_of("Node").contains(&"User".to_string()));
    }

    #[test]
    fn test_union_dependencies() {
        let schema = CompiledSchema {
            types: vec![
                make_type("User", vec![("name", FieldType::String)]),
                make_type("Post", vec![("title", FieldType::String)]),
            ],
            unions: vec![UnionDefinition {
                name:         "SearchResult".to_string(),
                member_types: vec!["User".to_string(), "Post".to_string()],
                description:  None,
            }],
            queries: vec![QueryDefinition::new("search", "SearchResult").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        // SearchResult depends on User and Post
        let deps = graph.dependencies_of("SearchResult");
        assert!(deps.contains(&"User".to_string()));
        assert!(deps.contains(&"Post".to_string()));
    }

    // =========================================================================
    // Input Type Tests
    // =========================================================================

    #[test]
    fn test_input_type_dependencies() {
        let schema = CompiledSchema {
            types: vec![make_type("User", vec![("name", FieldType::String)])],
            input_types: vec![
                InputObjectDefinition {
                    name:        "UserFilter".to_string(),
                    fields:      vec![InputFieldDefinition::new("status", "UserStatus")],
                    description: None,
                    metadata: None,
                },
                InputObjectDefinition {
                    name:        "UserStatus".to_string(),
                    fields:      vec![InputFieldDefinition::new("active", "Boolean")],
                    description: None,
                    metadata: None,
                },
            ],
            queries: vec![QueryDefinition::new("users", "User").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        // UserFilter depends on UserStatus
        assert!(graph.has_type("UserFilter"));
        assert!(graph.has_type("UserStatus"));
        assert!(graph
            .dependencies_of("UserFilter")
            .contains(&"UserStatus".to_string()));
    }

    // =========================================================================
    // Mutation and Subscription Tests
    // =========================================================================

    #[test]
    fn test_mutation_return_type_dependency() {
        let schema = CompiledSchema {
            types: vec![make_type("User", vec![("name", FieldType::String)])],
            mutations: vec![MutationDefinition::new("createUser", "User")],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        assert!(graph.has_type("Mutation"));
        assert!(graph.dependencies_of("Mutation").contains(&"User".to_string()));
        assert!(graph.dependents_of("User").contains(&"Mutation".to_string()));
    }

    #[test]
    fn test_subscription_return_type_dependency() {
        let schema = CompiledSchema {
            types: vec![make_type("User", vec![("name", FieldType::String)])],
            subscriptions: vec![SubscriptionDefinition::new("userCreated", "User")],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        assert!(graph.has_type("Subscription"));
        assert!(graph
            .dependencies_of("Subscription")
            .contains(&"User".to_string()));
    }

    // =========================================================================
    // CyclePath Tests
    // =========================================================================

    #[test]
    fn test_cycle_path_formatting() {
        let cycle = CyclePath::new(vec![
            "A".to_string(),
            "B".to_string(),
            "C".to_string(),
        ]);
        assert_eq!(cycle.path_string(), "A → B → C → A");
        assert_eq!(cycle.len(), 3);
        assert!(!cycle.is_self_reference());
        assert!(!cycle.is_empty());
    }

    #[test]
    fn test_cycle_path_self_reference() {
        let cycle = CyclePath::new(vec!["Node".to_string()]);
        assert_eq!(cycle.path_string(), "Node → Node");
        assert!(cycle.is_self_reference());
    }

    #[test]
    fn test_cycle_path_empty() {
        let cycle = CyclePath::new(vec![]);
        assert_eq!(cycle.path_string(), "");
        assert!(cycle.is_empty());
    }
}
