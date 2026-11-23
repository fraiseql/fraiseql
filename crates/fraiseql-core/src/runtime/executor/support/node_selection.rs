//! What may be selected on `node` itself: the fields of the `Node` interface (GraphQL
//! § 5.3.1, Field Selections).
//!
//! `Node` declares `id`, and every type answers `__typename`. Any other field is selected
//! through a fragment on a type that has it. A fragment with no type condition, or one on
//! `Node`, selects on `Node` too, so its fields are held to the same rule; a fragment on
//! any other type is checked later, against the type the id names.
//!
//! The check runs on the document as written: once named spreads are expanded, a
//! fragment's fields can no longer be told from fields written on `node` directly.

use std::collections::HashSet;

use crate::{
    error::{FraiseQLError, Result},
    graphql::{FieldSelection, FragmentDefinition},
    schema::CompiledSchema,
};

/// The fields `Node` itself answers.
const NODE_FIELDS: [&str; 2] = ["id", "__typename"];

/// Refuse a field selected on the `node` root `root` that `Node` does not declare,
/// naming it and suggesting a fragment on each Node type that has it.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation`, at the field's path, for the first such field.
pub(in super::super) fn check_node_selection(
    schema: &CompiledSchema,
    root: &FieldSelection,
    fragments: &[FragmentDefinition],
) -> Result<()> {
    let mut expanded = HashSet::new();
    match first_undeclared(&root.nested_fields, fragments, &mut expanded) {
        Some(field) => Err(FraiseQLError::Validation {
            message: refusal(schema, &field.name),
            path:    Some(format!("{}.{}", root.response_key(), field.response_key())),
        }),
        None => Ok(()),
    }
}

/// The first field `Node` does not declare among `selections`, looking into the
/// fragments that select on `Node`.
fn first_undeclared<'a>(
    selections: &'a [FieldSelection],
    fragments: &'a [FragmentDefinition],
    expanded: &mut HashSet<&'a str>,
) -> Option<&'a FieldSelection> {
    for selection in selections {
        let found = match selection.name.strip_prefix("...") {
            Some(inline) if inline.starts_with("on ") => {
                let condition = &inline[3..];
                if condition.is_empty() || condition == "Node" {
                    first_undeclared(&selection.nested_fields, fragments, expanded)
                } else {
                    None
                }
            },
            // A named spread; one the document does not define is refused where spreads
            // are expanded.
            Some(spread) => fragments
                .iter()
                .find(|fragment| fragment.name == spread)
                .filter(|fragment| fragment.type_condition == "Node")
                .filter(|fragment| expanded.insert(fragment.name.as_str()))
                .and_then(|fragment| first_undeclared(&fragment.selections, fragments, expanded)),
            None if NODE_FIELDS.contains(&selection.name.as_str()) => None,
            None => Some(selection),
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

/// The refusal of `field` on `node`, with a fragment on each Node type that declares it.
fn refusal(schema: &CompiledSchema, field: &str) -> String {
    let mut holders: Vec<&str> = schema
        .types
        .iter()
        .filter(|ty| {
            schema.is_node_type(ty.name.as_str()) && ty.fields.iter().any(|f| f.name == field)
        })
        .map(|ty| ty.name.as_str())
        .collect();
    holders.sort_unstable();
    let head = format!(
        "Cannot query field `{field}` on `node`: its type is the `Node` interface, which \
         declares only `id`"
    );
    if holders.is_empty() {
        return format!("{head}, and no Node type declares `{field}`");
    }
    let fragments: Vec<String> =
        holders.iter().map(|ty| format!("`... on {ty} {{ {field} }}`")).collect();
    format!(
        "{head}. Select it in a fragment on the type that has it: {}",
        fragments.join(" or ")
    )
}
