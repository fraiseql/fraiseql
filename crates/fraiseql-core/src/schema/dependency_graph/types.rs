//! Helper types for schema dependency graph analysis.

use std::collections::HashSet;

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
    pub const fn new(nodes: Vec<String>) -> Self {
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
    pub const fn is_self_reference(&self) -> bool {
        self.nodes.len() == 1
    }

    /// Get the length of the cycle (number of types involved).
    #[must_use]
    pub const fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if the cycle is empty (should never happen in practice).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// Result of analyzing the impact of deleting or modifying a type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeImpact {
    /// Types that would be affected by this change.
    pub affected_types:   HashSet<String>,
    /// Human-readable descriptions of breaking changes.
    pub breaking_changes: Vec<String>,
}

impl ChangeImpact {
    /// Create a new change impact result.
    #[must_use]
    pub const fn new(affected_types: HashSet<String>, breaking_changes: Vec<String>) -> Self {
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
