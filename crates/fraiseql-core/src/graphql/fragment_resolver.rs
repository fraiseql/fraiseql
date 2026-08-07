//! Fragment resolution for GraphQL queries.
//!
//! Handles:
//! - Fragment spread resolution (`...FragmentName`)
//! - Inline fragment handling (`... on TypeName { fields }`)

use std::collections::{HashMap, HashSet};

use thiserror::Error;

use crate::graphql::types::{FieldSelection, FragmentDefinition};

/// Errors that can occur during fragment resolution.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum FragmentError {
    /// Indicates that the requested fragment was not found.
    #[error("Fragment not found: {0}")]
    FragmentNotFound(String),

    /// Indicates that fragment depth limit was exceeded.
    #[error("Fragment depth exceeded (max: {0})")]
    FragmentDepthExceeded(u32),

    /// Indicates a circular reference was detected in fragments.
    #[error("Circular fragment reference detected")]
    CircularFragmentReference,
}

/// Resolves GraphQL fragment spreads in query selection sets.
///
/// Handles fragment spreads (`...FragmentName`) by expanding them to their actual field selections,
/// in document order — a GraphQL response's fields must appear in the order the query asked for
/// them (spec § Response Format).
///
/// # Example
///
/// ```
/// use fraiseql_core::graphql::{FragmentResolver, FragmentDefinition, FieldSelection};
///
/// let fragment = FragmentDefinition {
///     name: "UserFields".to_string(),
///     type_condition: "User".to_string(),
///     selections: vec![
///         FieldSelection {
///             name: "id".to_string(),
///             alias: None,
///             arguments: vec![],
///             nested_fields: vec![],
///             directives: vec![],
///         },
///     ],
///     fragment_spreads: vec![],
/// };
///
/// let resolver = FragmentResolver::new(&[fragment]);
/// ```
pub struct FragmentResolver {
    fragments: HashMap<String, FragmentDefinition>,
    max_depth: u32,
}

impl FragmentResolver {
    /// Create a new fragment resolver from a list of fragment definitions.
    #[must_use]
    pub fn new(fragments: &[FragmentDefinition]) -> Self {
        let map = fragments.iter().map(|f| (f.name.clone(), f.clone())).collect();
        Self {
            fragments: map,
            max_depth: 10,
        }
    }

    /// Create a resolver with a custom max depth.
    #[must_use]
    pub const fn with_max_depth(mut self, max_depth: u32) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Resolve all fragment spreads in selections.
    ///
    /// # Errors
    /// Returns error if:
    /// - Fragment is not found
    /// - Fragment depth exceeds maximum
    /// - Circular references are detected
    pub fn resolve_spreads(
        &self,
        selections: &[FieldSelection],
    ) -> Result<Vec<FieldSelection>, FragmentError> {
        self.resolve_selections(selections, 0, &mut HashSet::new())
    }

    /// Recursively resolve selections at a given depth.
    fn resolve_selections(
        &self,
        selections: &[FieldSelection],
        depth: u32,
        visited_fragments: &mut HashSet<String>,
    ) -> Result<Vec<FieldSelection>, FragmentError> {
        if depth > self.max_depth {
            return Err(FragmentError::FragmentDepthExceeded(self.max_depth));
        }

        let mut result = Vec::new();

        for selection in selections {
            // Check if this is a fragment spread (starts with "...")
            if let Some(fragment_name) = selection.name.strip_prefix("...") {
                // Skip inline fragments (they have " on " in the name)
                if fragment_name.starts_with("on ") {
                    // Inline fragment — counts as a nesting level (depth + 1) so that
                    // deeply-nested inline fragments cannot bypass the depth limit.
                    let mut field = selection.clone();
                    if !field.nested_fields.is_empty() {
                        field.nested_fields = self.resolve_selections(
                            &field.nested_fields,
                            depth + 1,
                            visited_fragments,
                        )?;
                    }
                    result.push(field);
                    continue;
                }

                // Named fragment spread
                let fragment_name = fragment_name.to_string();

                // Detect circular references
                if visited_fragments.contains(&fragment_name) {
                    return Err(FragmentError::CircularFragmentReference);
                }

                // Get fragment definition
                let fragment = self
                    .fragments
                    .get(&fragment_name)
                    .ok_or_else(|| FragmentError::FragmentNotFound(fragment_name.clone()))?;

                // Mark as visited
                visited_fragments.insert(fragment_name.clone());

                // Recursively resolve the fragment's selections
                let mut resolved =
                    self.resolve_selections(&fragment.selections, depth + 1, visited_fragments)?;

                // Carry the spread's own directives onto every field it
                // contributes. `@skip`/`@include` are spec-valid on
                // `FRAGMENT_SPREAD`, and expansion is the last moment at which
                // the association still exists: once the fragment's fields are
                // flattened into the parent set they are indistinguishable from
                // fields written there directly (#826).
                for field in &mut resolved {
                    field.inherit_directives(&selection.directives);
                }
                result.extend(resolved);

                // Unmark for other paths
                visited_fragments.remove(&fragment_name);
            } else {
                // Regular field: nested fields are one level deeper.
                let mut field = selection.clone();
                if !field.nested_fields.is_empty() {
                    field.nested_fields = self.resolve_selections(
                        &field.nested_fields,
                        depth + 1,
                        visited_fragments,
                    )?;
                }
                result.push(field);
            }
        }

        Ok(result)
    }
}
