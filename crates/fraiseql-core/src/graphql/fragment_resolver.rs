//! Fragment resolution for GraphQL queries.
//!
//! Handles:
//! - Fragment spread resolution (`...FragmentName`)
//! - Inline fragment handling (`... on TypeName { fields }`)
//! - Selection set merging with deduplication

use crate::graphql::types::{FieldSelection, FragmentDefinition};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

/// Errors that can occur during fragment resolution.
#[derive(Debug, Error)]
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
/// Handles fragment spreads (`...FragmentName`) by expanding them to their actual field selections.
/// Also merges multiple fragment definitions and handles field deduplication.
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
        let map = fragments
            .iter()
            .map(|f| (f.name.clone(), f.clone()))
            .collect();
        Self {
            fragments: map,
            max_depth: 10,
        }
    }

    /// Create a resolver with a custom max depth.
    #[must_use]
    pub fn with_max_depth(mut self, max_depth: u32) -> Self {
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
                    // Inline fragment - just recurse into nested fields
                    let mut field = selection.clone();
                    if !field.nested_fields.is_empty() {
                        field.nested_fields = self.resolve_selections(
                            &field.nested_fields,
                            depth,
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
                let resolved =
                    self.resolve_selections(&fragment.selections, depth + 1, visited_fragments)?;
                result.extend(resolved);

                // Unmark for other paths
                visited_fragments.remove(&fragment_name);
            } else {
                // Regular field: recurse into nested fields
                let mut field = selection.clone();
                if !field.nested_fields.is_empty() {
                    field.nested_fields =
                        self.resolve_selections(&field.nested_fields, depth, visited_fragments)?;
                }
                result.push(field);
            }
        }

        Ok(result)
    }

    /// Handle inline fragments with type conditions.
    ///
    /// Evaluates whether an inline fragment applies based on type conditions.
    /// Returns the selections if the type condition matches, or an empty vector if it doesn't.
    #[must_use]
    pub fn evaluate_inline_fragment(
        selections: &[FieldSelection],
        type_condition: Option<&str>,
        actual_type: &str,
    ) -> Vec<FieldSelection> {
        // If no type condition, inline fragment applies to all types
        if type_condition.is_none() {
            return selections.to_vec();
        }

        // If type condition matches actual type, include the fields
        if type_condition == Some(actual_type) {
            selections.to_vec()
        } else {
            // Type condition doesn't match - skip these fields
            vec![]
        }
    }

    /// Merge field selections from multiple sources (e.g., fragment spreads).
    ///
    /// Handles:
    /// - Combining fields from multiple fragments
    /// - Deduplicating fields by name/alias
    /// - Merging nested selections
    #[must_use]
    pub fn merge_selections(
        base: &[FieldSelection],
        additional: Vec<FieldSelection>,
    ) -> Vec<FieldSelection> {
        // Build map of existing fields by response key (alias or name)
        let mut by_key: HashMap<String, FieldSelection> = base
            .iter()
            .map(|f| (f.response_key().to_string(), f.clone()))
            .collect();

        // Merge additional fields
        for field in additional {
            let key = field.response_key().to_string();
            if let Some(existing) = by_key.get_mut(&key) {
                // Field already exists - merge nested selections
                if !field.nested_fields.is_empty() {
                    existing.nested_fields.extend(field.nested_fields);
                    // Deduplicate nested fields
                    existing.nested_fields = Self::deduplicate_fields(&existing.nested_fields);
                }
            } else {
                // New field - add it
                by_key.insert(key, field);
            }
        }

        by_key.into_values().collect()
    }

    /// Deduplicate fields in a selection set by response key.
    fn deduplicate_fields(fields: &[FieldSelection]) -> Vec<FieldSelection> {
        let mut seen = HashSet::new();
        fields
            .iter()
            .filter(|f| seen.insert(f.response_key().to_string()))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_field(name: &str, nested: Vec<FieldSelection>) -> FieldSelection {
        FieldSelection {
            name: name.to_string(),
            alias: None,
            arguments: vec![],
            nested_fields: nested,
            directives: vec![],
        }
    }

    fn make_fragment(name: &str, selections: Vec<FieldSelection>) -> FragmentDefinition {
        FragmentDefinition {
            name: name.to_string(),
            type_condition: "User".to_string(),
            selections,
            fragment_spreads: vec![],
        }
    }

    #[test]
    fn test_simple_fragment_spread_resolution() {
        let fragment = make_fragment(
            "UserFields",
            vec![make_field("id", vec![]), make_field("name", vec![])],
        );

        let selections = vec![FieldSelection {
            name: "...UserFields".to_string(),
            alias: None,
            arguments: vec![],
            nested_fields: vec![],
            directives: vec![],
        }];

        let resolver = FragmentResolver::new(&[fragment]);
        let result_selections = resolver.resolve_spreads(&selections).unwrap();

        assert_eq!(result_selections.len(), 2);
        assert_eq!(result_selections[0].name, "id");
        assert_eq!(result_selections[1].name, "name");
    }

    #[test]
    fn test_fragment_not_found() {
        let selections = vec![FieldSelection {
            name: "...NonexistentFragment".to_string(),
            alias: None,
            arguments: vec![],
            nested_fields: vec![],
            directives: vec![],
        }];

        let resolver = FragmentResolver::new(&[]);
        let result = resolver.resolve_spreads(&selections);

        assert!(matches!(result, Err(FragmentError::FragmentNotFound(_))));
    }

    #[test]
    fn test_nested_fragment_spreads() {
        // Fragment A references fields
        let fragment_a = make_fragment("FragmentA", vec![make_field("id", vec![])]);

        // Fragment B spreads Fragment A
        let fragment_b = make_fragment(
            "FragmentB",
            vec![
                FieldSelection {
                    name: "...FragmentA".to_string(),
                    alias: None,
                    arguments: vec![],
                    nested_fields: vec![],
                    directives: vec![],
                },
                make_field("name", vec![]),
            ],
        );

        // Query spreads Fragment B
        let selections = vec![FieldSelection {
            name: "...FragmentB".to_string(),
            alias: None,
            arguments: vec![],
            nested_fields: vec![],
            directives: vec![],
        }];

        let resolver = FragmentResolver::new(&[fragment_a, fragment_b]);
        let result_selections = resolver.resolve_spreads(&selections).unwrap();

        assert_eq!(result_selections.len(), 2);
        assert_eq!(result_selections[0].name, "id");
        assert_eq!(result_selections[1].name, "name");
    }

    #[test]
    fn test_inline_fragment_matching_type() {
        let selections = vec![make_field("id", vec![]), make_field("name", vec![])];

        let result = FragmentResolver::evaluate_inline_fragment(&selections, Some("User"), "User");

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "id");
    }

    #[test]
    fn test_inline_fragment_non_matching_type() {
        let selections = vec![make_field("id", vec![]), make_field("name", vec![])];

        let result = FragmentResolver::evaluate_inline_fragment(&selections, Some("User"), "Post");

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_inline_fragment_without_type_condition() {
        let selections = vec![make_field("id", vec![]), make_field("name", vec![])];

        let result = FragmentResolver::evaluate_inline_fragment(&selections, None, "User");

        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_merge_non_conflicting_fields() {
        let base = vec![make_field("id", vec![]), make_field("name", vec![])];

        let additional = vec![make_field("email", vec![])];

        let merged = FragmentResolver::merge_selections(&base, additional);

        assert_eq!(merged.len(), 3);
        let names: Vec<_> = merged.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"id"));
        assert!(names.contains(&"name"));
        assert!(names.contains(&"email"));
    }

    #[test]
    fn test_merge_conflicting_fields_with_alias() {
        let base = vec![FieldSelection {
            name: "user".to_string(),
            alias: Some("primaryUser".to_string()),
            arguments: vec![],
            nested_fields: vec![make_field("id", vec![])],
            directives: vec![],
        }];

        let additional = vec![FieldSelection {
            name: "user".to_string(),
            alias: Some("primaryUser".to_string()),
            arguments: vec![],
            nested_fields: vec![make_field("name", vec![])],
            directives: vec![],
        }];

        let merged = FragmentResolver::merge_selections(&base, additional);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].nested_fields.len(), 2); // id and name merged
    }

    #[test]
    fn test_depth_limit() {
        // Create deeply nested fragment references
        let mut fragments = vec![];
        for i in 0..12 {
            let name = format!("Fragment{i}");
            let next_spread = if i < 11 {
                FieldSelection {
                    name: format!("...Fragment{}", i + 1),
                    alias: None,
                    arguments: vec![],
                    nested_fields: vec![],
                    directives: vec![],
                }
            } else {
                make_field("field", vec![])
            };

            fragments.push(FragmentDefinition {
                name,
                type_condition: "User".to_string(),
                selections: vec![next_spread],
                fragment_spreads: vec![],
            });
        }

        let selections = vec![FieldSelection {
            name: "...Fragment0".to_string(),
            alias: None,
            arguments: vec![],
            nested_fields: vec![],
            directives: vec![],
        }];

        let resolver = FragmentResolver::new(&fragments);
        let result = resolver.resolve_spreads(&selections);

        assert!(matches!(result, Err(FragmentError::FragmentDepthExceeded(_))));
    }

    #[test]
    fn test_circular_reference_detection() {
        // FragmentA -> FragmentB -> FragmentA (circular)
        let fragment_a = FragmentDefinition {
            name: "FragmentA".to_string(),
            type_condition: "User".to_string(),
            selections: vec![FieldSelection {
                name: "...FragmentB".to_string(),
                alias: None,
                arguments: vec![],
                nested_fields: vec![],
                directives: vec![],
            }],
            fragment_spreads: vec!["FragmentB".to_string()],
        };

        let fragment_b = FragmentDefinition {
            name: "FragmentB".to_string(),
            type_condition: "User".to_string(),
            selections: vec![FieldSelection {
                name: "...FragmentA".to_string(),
                alias: None,
                arguments: vec![],
                nested_fields: vec![],
                directives: vec![],
            }],
            fragment_spreads: vec!["FragmentA".to_string()],
        };

        let selections = vec![FieldSelection {
            name: "...FragmentA".to_string(),
            alias: None,
            arguments: vec![],
            nested_fields: vec![],
            directives: vec![],
        }];

        let resolver = FragmentResolver::new(&[fragment_a, fragment_b]);
        let result = resolver.resolve_spreads(&selections);

        assert!(matches!(
            result,
            Err(FragmentError::CircularFragmentReference)
        ));
    }
}
