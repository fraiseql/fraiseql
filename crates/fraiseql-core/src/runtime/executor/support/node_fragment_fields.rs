//! A `node` root's fragments on its Node types.
//!
//! `node` itself answers `Node`'s fields, which `node_selection` holds it to. A fragment on
//! an object type selects that type's fields: they exist on it (GraphQL § 5.3.1), checked
//! here exactly as a field selected on a root returning that type is, with the same
//! refusal. The fragment's type condition was checked before, by
//! `graphql::fragment_conditions`.
//!
//! Which of the fragments apply is known once the id names the node's type (§ 6.3.2), so a
//! named spread at `node`'s level keeps its type condition until then: expanding it in
//! place, as a spread is expanded elsewhere, would give its fields to every Node type.

use std::collections::HashSet;

use crate::{
    error::Result,
    graphql::{FieldSelection, FragmentDefinition, selection_set, validate_selection_set},
    schema::CompiledSchema,
};

/// Refuse a field selected on `root`, a `node` root, through a fragment on an object type
/// that does not declare it.
///
/// # Errors
///
/// [`crate::error::FraiseQLError::Validation`] as `validate_selection_set` refuses the field
/// on the fragment's type, or as fragment expansion refuses the fragment's own spreads.
pub(in super::super) fn check_node_fragment_fields(
    schema: &CompiledSchema,
    root: &FieldSelection,
    fragments: &[FragmentDefinition],
    max_depth: u32,
) -> Result<()> {
    for selection in &root.nested_fields {
        let (condition, selections) = match selection.name.strip_prefix("...") {
            Some(inline) if inline.starts_with("on ") => {
                (&inline[3..], selection.nested_fields.as_slice())
            },
            Some(spread) => match fragments.iter().find(|f| f.name == spread) {
                Some(fragment) => {
                    (fragment.type_condition.as_str(), fragment.selections.as_slice())
                },
                None => continue,
            },
            None => continue,
        };
        if schema.find_type(condition).is_none() {
            continue;
        }
        let fields = selection_set::resolve(selections, fragments, max_depth)?;
        validate_selection_set(schema, condition, &fields)?;
    }
    Ok(())
}

/// `selections`, a `node` root's selection set as written, with each named spread at its
/// level made the inline fragment it stands for, on its type condition. Spreads below that
/// level, inside a field or a fragment on another type, are left to be expanded in place.
pub(in super::super) fn typed_node_spreads(
    selections: &[FieldSelection],
    fragments: &[FragmentDefinition],
) -> Vec<FieldSelection> {
    typed_at(selections, fragments, &mut HashSet::new())
}

fn typed_at<'a>(
    selections: &'a [FieldSelection],
    fragments: &'a [FragmentDefinition],
    spreading: &mut HashSet<&'a str>,
) -> Vec<FieldSelection> {
    selections
        .iter()
        .map(|selection| match selection.name.strip_prefix("...") {
            // `... { }` and `... on Node { }` stand at `node`'s level too.
            Some(inline) if inline == "on " || inline == "on Node" => FieldSelection {
                nested_fields: typed_at(&selection.nested_fields, fragments, spreading),
                ..selection.clone()
            },
            Some(inline) if inline.starts_with("on ") => selection.clone(),
            Some(spread) => {
                // An undefined fragment, or a cycle, is refused where spreads are expanded.
                let Some(fragment) = fragments.iter().find(|f| f.name == spread) else {
                    return selection.clone();
                };
                if !spreading.insert(fragment.name.as_str()) {
                    return selection.clone();
                }
                let nested = if fragment.type_condition == "Node" {
                    typed_at(&fragment.selections, fragments, spreading)
                } else {
                    fragment.selections.clone()
                };
                spreading.remove(fragment.name.as_str());
                FieldSelection {
                    name:          format!("...on {}", fragment.type_condition),
                    alias:         None,
                    arguments:     Vec::new(),
                    nested_fields: nested,
                    directives:    selection.directives.clone(),
                }
            },
            None => selection.clone(),
        })
        .collect()
}
