//! Field-existence validation for a resolved selection set.
//!
//! GraphQL § 5.3.1 (Field Selections on Objects): *"The target field of a field
//! selection must be defined on the scoped type of the selection set. There are
//! no limitations on alias names."* A document that selects an undefined field is
//! **invalid**, and an invalid document must not execute.
//!
//! Before this, an undeclared name was lowered straight into the SQL projection,
//! where `data->>'phantom_field'` evaluates to NULL and serialises as a
//! legitimate-looking `null` under HTTP 200 with no `errors` array (#939). A
//! client typo — `emial` for `email` — rendered every row with a blank value and
//! left no trace in the response or the logs.
//!
//! # What this does not reject
//!
//! Every unknown is a *pass*, deliberately: the validator's job is to catch a
//! field that the schema positively says is not there, and a rejection it cannot
//! justify would break a working query. It therefore skips
//!
//! * a type the compiled schema does not carry,
//! * a type whose field list is empty — an object type must have at least one field, so an empty
//!   list means the compiler emitted no field information rather than a type with no fields,
//! * meta-fields (`__typename` and the introspection entry points), valid on any selection set,
//! * an inline fragment whose type condition names a type the schema does not carry,
//! * and everything below the module's depth cap, which the parser's own depth limits already
//!   bound.

use crate::{
    error::{FraiseQLError, Result},
    graphql::types::FieldSelection,
    schema::CompiledSchema,
};

/// Deepest selection level validated. Documents are depth-limited upstream by
/// GATE 1; this is a second bound so a hand-built selection tree cannot recurse
/// without end.
const MAX_VALIDATION_DEPTH: usize = 16;

/// Check that every field in `selections` is defined on `type_name`.
///
/// `selections` must already have had its fragment spreads expanded
/// ([`crate::graphql::selection_set::resolve`]): a spread contributes fields to
/// the parent set, and they validate exactly like fields written there directly.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] naming the first undeclared field and
/// the type it is not defined on.
pub fn validate_selection_set(
    schema: &CompiledSchema,
    type_name: &str,
    selections: &[FieldSelection],
) -> Result<()> {
    validate_at(schema, type_name, selections, 0)
}

fn validate_at(
    schema: &CompiledSchema,
    type_name: &str,
    selections: &[FieldSelection],
    depth: usize,
) -> Result<()> {
    if depth >= MAX_VALIDATION_DEPTH {
        return Ok(());
    }

    // A union has no fields of its own, so there is nothing to score a bare key
    // against — but each inline fragment names a concrete variant, and that
    // variant's fields are checkable. This is the scoping rule mutation payloads
    // need (#1005): a payload type may be a union of success and error variants
    // resolved per result (#212, #450/#451, #698's synthesized cascade envelope),
    // and validating a member's fields against the *wrong* variant would reject a
    // working mutation — strictly worse than the silent null being fixed.
    //
    // Bare non-meta fields directly on a union pass unadjudicated rather than
    // being refused. § 5.3.1 does forbid them, but rejecting here would make this
    // routine the arbiter of union modelling, and #939's guards are deliberately
    // the other way: an absence of evidence is not evidence of a defect.
    if schema.find_type(type_name).is_none() && schema.find_union(type_name).is_some() {
        for sel in selections {
            if let Some(condition) = sel.name.strip_prefix("...on ") {
                let condition = condition.trim();
                if schema.find_type(condition).is_some() {
                    validate_at(schema, condition, &sel.nested_fields, depth + 1)?;
                }
            }
        }
        return Ok(());
    }

    let Some(type_def) = schema.find_type(type_name) else {
        return Ok(());
    };
    if type_def.fields.is_empty() {
        return Ok(());
    }

    for sel in selections {
        // An inline fragment: its selection set is scoped to the type condition,
        // not to the parent type.
        if let Some(condition) = sel.name.strip_prefix("...on ") {
            let condition = condition.trim();
            if schema.find_type(condition).is_some() {
                validate_at(schema, condition, &sel.nested_fields, depth + 1)?;
            }
            continue;
        }
        // A spread this far down means expansion did not run; it is not this
        // routine's job to fail the request over that.
        if sel.name.starts_with("...") {
            continue;
        }
        // `__typename` and the introspection entry points are meta-fields: valid
        // on every selection set, and never present in a type's field list.
        if sel.name.starts_with("__") {
            continue;
        }

        let Some(field_def) = type_def.fields.iter().find(|f| f.name == sel.name) else {
            return Err(FraiseQLError::Validation {
                message: format!("Cannot query field '{}' on type '{type_name}'.", sel.name),
                path:    Some(format!("{type_name}.{}", sel.name)),
            });
        };

        if sel.nested_fields.is_empty() {
            continue;
        }
        // Recurse into object and list-of-object fields alike; `inner_type`
        // unwraps the list so the element type is what the sub-selection is
        // scoped to.
        let child_type = field_def
            .field_type
            .inner_type()
            .and_then(crate::schema::FieldType::type_name)
            .or_else(|| field_def.field_type.type_name());
        if let Some(child_type) = child_type {
            validate_at(schema, child_type, &sel.nested_fields, depth + 1)?;
        }
    }

    Ok(())
}
