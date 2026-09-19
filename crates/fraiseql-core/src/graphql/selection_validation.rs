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
//! # § 5.3.3 Leaf Field Selections
//!
//! The same routine carries the converse rule: a field whose type is **composite**
//! — an object, an interface or a union — must have a non-empty selection set, and
//! a document that omits one is invalid.
//!
//! It was unenforced until #1357, and on the write path an absent selection set is
//! not merely under-specified, it is *permissive*: `project_entity` returns the
//! stored entity unchanged for an empty slice, and
//! `selection_set_selects_gated_field` is false for one, so the #423 field
//! authorizer takes zero calls. `mutation { createUser }` therefore answered with
//! every `authorize`-gated field of the stored row, to a caller the same document
//! with braces would have been refused. The read path returned `{}` per row
//! instead (#1076) — same invalid document, different half of the engine, which is
//! why it read as a formatting quirk rather than a bypass for two releases.
//!
//! Both halves of the rule are refused here, at the one site the mutation path
//! reaches **before** the write.
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

/// The named **composite** type this field resolves to, list wrappers removed.
///
/// GraphQL's composite types are objects, interfaces and unions — exactly the types
/// § 5.3.3 requires a selection set on. Enums and scalars are *leaf* types and are
/// deliberately excluded: `status` with no sub-selection is correct for an enum, so
/// recursing into one would refuse a working document.
///
/// Not [`FieldType::type_name`], which also answers for `Enum` and `Input`.
///
/// ⚠ This exclusion is **redundancy, not the operative guard**: enums and input
/// types live in `schema.enums` / the input registry rather than `schema.types`, so
/// `requires_selection_set` already answers `false` for them and admitting them here
/// changes no outcome — mutating this match alone leaves every test green. It is
/// kept because it states the rule at the point the rule is about, and because it
/// stops holding only if `requires_selection_set` grows a branch that can see them.
fn composite_type_of(field_type: &crate::schema::FieldType) -> Option<&str> {
    use crate::schema::FieldType;
    match field_type.inner_type().unwrap_or(field_type) {
        FieldType::Object(name) | FieldType::Interface(name) | FieldType::Union(name) => Some(name),
        _ => None,
    }
}

/// Whether § 5.3.3 requires a selection set on a field of type `type_name`.
///
/// One predicate, consulted at both sites that can meet an absent selection set —
/// the operation's root set and every composite field below it — so the two cannot
/// drift into disagreeing about what "composite" means.
///
/// **Every unknown is a `false`**, matching this module's governing rule that a
/// rejection it cannot justify would break a working query: a type the compiled
/// schema does not carry, and an object whose field list the compiler did not emit,
/// both pass. A union is `true` — it is composite, and unlike the bare-field case
/// below there is no field to score against the wrong variant, so refusing an
/// absent set does not make this routine the arbiter of union modelling.
fn requires_selection_set(schema: &CompiledSchema, type_name: &str) -> bool {
    if let Some(type_def) = schema.find_type(type_name) {
        return !type_def.fields.is_empty();
    }
    schema.find_union(type_name).is_some()
}

/// § 5.3.3, as raised for an operation's **root** selection set.
fn root_needs_selection_set(type_name: &str) -> FraiseQLError {
    FraiseQLError::Validation {
        message: format!("Field of composite type '{type_name}' must have a selection set."),
        path:    Some(type_name.to_string()),
    }
}

/// § 5.3.3, as raised for a **field** whose composite type was named without one.
fn field_needs_selection_set(parent: &str, field: &str, child_type: &str) -> FraiseQLError {
    FraiseQLError::Validation {
        message: format!(
            "Field '{field}' of composite type '{child_type}' must have a selection set."
        ),
        path:    Some(format!("{parent}.{field}")),
    }
}

/// Check GraphQL § 5.3.3 (Leaf Field Selections): a field whose type is composite
/// must have a non-empty selection set.
///
/// # The input must be the *written* selection set
///
/// Fragment spreads expanded, **`@skip`/`@include` not yet evaluated**. § 5.3.3 is a
/// static rule about the document, and unlike § 5.3.1 next door it is *anti-monotone*
/// under field removal: dropping fields can only reduce the chances of naming an
/// undeclared one, but it can turn a perfectly valid selection set into an empty one.
///
/// `{ users { id @skip(if: true) } }` is a valid document whose `users` selection
/// resolves to nothing, and the spec's answer to it is `{}`, not an error. Handing
/// this function the post-directive set would refuse it. That is why this is a
/// separate entry point from [`validate_selection_set`] rather than another rule
/// inside it: the two need different inputs, and one shared function would leave
/// every caller to remember which.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] naming the first composite field written
/// without a selection set, and the type that needed one.
pub fn validate_leaf_field_selections(
    schema: &CompiledSchema,
    type_name: &str,
    selections: &[FieldSelection],
) -> Result<()> {
    leaf_check_at(schema, type_name, selections, 0)
}

fn leaf_check_at(
    schema: &CompiledSchema,
    type_name: &str,
    selections: &[FieldSelection],
    depth: usize,
) -> Result<()> {
    if depth >= MAX_VALIDATION_DEPTH {
        return Ok(());
    }

    if selections.is_empty() {
        return if requires_selection_set(schema, type_name) {
            Err(root_needs_selection_set(type_name))
        } else {
            Ok(())
        };
    }

    // A union carries its fields only inside its inline fragments; each names a
    // concrete variant, and that variant's sub-selection is what § 5.3.3 scores.
    let Some(type_def) = schema.find_type(type_name) else {
        for sel in selections {
            if let Some(condition) = sel.name.strip_prefix("...on ") {
                let condition = condition.trim();
                if schema.find_type(condition).is_some() {
                    leaf_check_at(schema, condition, &sel.nested_fields, depth + 1)?;
                }
            }
        }
        return Ok(());
    };
    if type_def.fields.is_empty() {
        return Ok(());
    }

    for sel in selections {
        if let Some(condition) = sel.name.strip_prefix("...on ") {
            let condition = condition.trim();
            if schema.find_type(condition).is_some() {
                leaf_check_at(schema, condition, &sel.nested_fields, depth + 1)?;
            }
            continue;
        }
        // An unexpanded spread, and the meta-fields, are § 5.3.1's business; a name
        // this routine cannot resolve to a field is not evidence of a § 5.3.3 defect.
        if sel.name.starts_with("...") || sel.name.starts_with("__") {
            continue;
        }
        let Some(field_def) = type_def.fields.iter().find(|f| f.name == sel.name) else {
            continue;
        };
        // `composite_type_of` unwraps the list wrapper and answers `None` for a leaf
        // type, whose lack of a sub-selection is correct rather than invalid.
        let Some(child_type) = composite_type_of(&field_def.field_type) else {
            continue;
        };
        if sel.nested_fields.is_empty() {
            if requires_selection_set(schema, child_type) {
                return Err(field_needs_selection_set(type_name, &sel.name, child_type));
            }
            // A composite whose type the schema does not carry: a pass, like every
            // other unknown — but only for *this* field. Returning `Ok` here instead
            // of continuing would skip every sibling still to be checked.
            continue;
        }
        leaf_check_at(schema, child_type, &sel.nested_fields, depth + 1)?;
    }

    Ok(())
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
        // Recurse into object and list-of-object fields alike; `composite_type_of`
        // unwraps the list so the element type is what the sub-selection is scoped
        // to.
        if let Some(child_type) = composite_type_of(&field_def.field_type) {
            validate_at(schema, child_type, &sel.nested_fields, depth + 1)?;
        }
    }

    Ok(())
}

#[cfg(test)]
#[path = "selection_validation_tests.rs"]
mod selection_validation_tests;
