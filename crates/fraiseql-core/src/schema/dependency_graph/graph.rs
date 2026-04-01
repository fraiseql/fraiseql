//! Core graph structure and direct query methods.

use std::collections::{HashMap, HashSet};

/// Schema dependency graph for analyzing type relationships.
///
/// This graph tracks which types depend on which other types, enabling:
/// - Circular dependency detection
/// - Unused type detection
/// - Impact analysis for schema changes
#[derive(Debug, Clone)]
pub struct SchemaDependencyGraph {
    /// Map of type name to types it depends on (outgoing edges).
    pub(super) outgoing: HashMap<String, HashSet<String>>,
    /// Map of type name to types that depend on it (incoming edges).
    pub(super) incoming: HashMap<String, HashSet<String>>,
    /// All type names in the schema.
    pub(super) all_types: HashSet<String>,
    /// Root types that are always considered "used" (Query, Mutation, Subscription).
    pub(super) root_types: HashSet<String>,
}

impl SchemaDependencyGraph {
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
}
