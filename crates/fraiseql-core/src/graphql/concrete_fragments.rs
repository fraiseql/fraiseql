//! Inline fragments that apply wherever they stand, read into their selection set.
//!
//! An inline fragment `... on T` at a position holding the object type `O` selects on it
//! when `T` is `O` or an interface `O` implements (§ 6.3.2, `DoesFragmentTypeApply`): every
//! value there is an `O`, so the schema alone decides it, once per document. Its fields
//! are then the set's own, each under the fragment's directives, as a named spread's are
//! when it is expanded. Read into the set here, before the fields are collected, every
//! reader of them (field access, masks, the projection, completion) meets the fields the
//! document selects, as it would with the fragment written out.
//!
//! A fragment at an abstract position (an interface or a union) is left as written: which
//! of them applies depends on each value's type, which the read decides. So is one that
//! cannot apply, which the validation refuses.

use super::FieldSelection;
use crate::schema::{CompiledSchema, FieldType};

/// `roots`, the root fields of a query, with every inline fragment that applies where it
/// stands read into its selection set, below each root reading a query that returns objects.
///
/// A Relay connection's selection is its connection type's, and a count's is no object's, so
/// both are left as written.
#[must_use]
pub fn splice_query_roots(
    mut roots: Vec<FieldSelection>,
    schema: &CompiledSchema,
) -> Vec<FieldSelection> {
    for root in &mut roots {
        let Some(query) = schema.find_query(&root.name) else {
            continue;
        };
        if query.relay || query.returns_count {
            continue;
        }
        root.nested_fields =
            splice(std::mem::take(&mut root.nested_fields), &query.return_type, schema);
    }
    roots
}

/// `selections`, a set at a position holding `type_name`, with every inline fragment that
/// applies there read into it, at every level below whose type is an object type.
fn splice(
    selections: Vec<FieldSelection>,
    type_name: &str,
    schema: &CompiledSchema,
) -> Vec<FieldSelection> {
    let Some(object) = schema.find_type(type_name) else {
        return selections;
    };
    let mut fields = Vec::with_capacity(selections.len());
    for mut selection in selections {
        if let Some(condition) = selection.name.strip_prefix("...on ") {
            let applies = condition.is_empty()
                || condition == type_name
                || object.implements.iter().any(|i| i == condition);
            if applies {
                let start = fields.len();
                fields.extend(splice(selection.nested_fields, type_name, schema));
                for field in &mut fields[start..] {
                    field.inherit_directives(&selection.directives);
                }
                continue;
            }
        } else if let Some(below) = object
            .find_field(&selection.name)
            .and_then(|field| object_type(&field.field_type))
        {
            selection.nested_fields = splice(selection.nested_fields, below, schema);
        }
        fields.push(selection);
    }
    fields
}

/// The object type a field of `field_type` holds, itself or as a list's elements.
fn object_type(field_type: &FieldType) -> Option<&str> {
    let held = field_type.inner_type().unwrap_or(field_type);
    if held.is_object() {
        held.type_name()
    } else {
        None
    }
}

#[cfg(test)]
#[path = "concrete_fragments_tests.rs"]
mod tests;
