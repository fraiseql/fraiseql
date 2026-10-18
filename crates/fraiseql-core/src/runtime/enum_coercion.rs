//! Enum input coercion: a member name written in a request becomes the value it is stored as
//! (#1580, GraphQL § 3.9 *Input Coercion*).
//!
//! A member may store a value other than its name (`SHIPPED = "shipped"`). The request names
//! the member; SQL compares and writes the stored value. So every enum position of an
//! operation's arguments is rewritten before any statement: an enum argument, an enum field
//! of an input object at any depth, and every enum operand of a `where:` predicate (whose
//! derived input types type them). The walk is the one
//! [`argument_value_validation`](super::argument_value_validation) adjudicates membership
//! with, run after it: a value here has already been found to name a member.
//!
//! Output is the other half, in [`completion`](super::completion): a stored value is served
//! as its member's name.
//!
//! A schema whose enums all store their names (the common case) pays one scan of its enums
//! and nothing else.

use std::collections::HashMap;

use serde_json::Value;

use crate::schema::{ArgumentDefinition, CompiledSchema, EnumDefinition, FieldType};

/// How deep the walk follows a value; a request is depth-limited upstream.
const MAX_COERCION_DEPTH: usize = 64;

/// Whether any enum of `schema` stores a value other than a member's name.
fn has_stored_values(schema: &CompiledSchema) -> bool {
    schema.enums.iter().any(EnumDefinition::has_stored_values)
}

/// Coerce the enum positions of a read's merged `arguments` against the arguments the query
/// publishes, in place.
pub fn coerce_enum_arguments(
    schema: &CompiledSchema,
    declared: &[ArgumentDefinition],
    arguments: &mut HashMap<String, Value>,
) {
    if !has_stored_values(schema) {
        return;
    }
    for arg in declared {
        if let Some(value) = arguments.get_mut(&arg.name) {
            coerce_field_type(schema, &arg.arg_type, value, 0);
        }
    }
}

/// Coerce the enum positions of a mutation's argument object: the coerced copy, or `None`
/// when nothing in `schema` needs it (the caller keeps its own).
#[must_use]
pub fn coerce_enum_argument_object(
    schema: &CompiledSchema,
    declared: &[ArgumentDefinition],
    arguments: Option<&Value>,
) -> Option<Value> {
    if !has_stored_values(schema) {
        return None;
    }
    let mut arguments = arguments.filter(|a| a.is_object())?.clone();
    if let Some(map) = arguments.as_object_mut() {
        for arg in declared {
            if let Some(value) = map.get_mut(&arg.name) {
                coerce_field_type(schema, &arg.arg_type, value, 0);
            }
        }
    }
    Some(arguments)
}

/// Coerce `value` at a position of compiled type `declared`.
fn coerce_field_type(
    schema: &CompiledSchema,
    declared: &FieldType,
    value: &mut Value,
    depth: usize,
) {
    if value.is_null() || depth > MAX_COERCION_DEPTH {
        return;
    }
    match declared {
        FieldType::List(inner) => {
            each_item(value, |item| coerce_field_type(schema, inner, item, depth + 1));
        },
        FieldType::Enum(name) => coerce_member(schema, name, value),
        // An input-type reference compiles as `Object` (see the validator's walk).
        FieldType::Object(name) | FieldType::Input(name) => {
            coerce_input_object(schema, name, value, depth);
        },
        _ => {},
    }
}

/// Coerce `value` at a position of written type `declared` (an input field's or a derived
/// filter operand's: `"[OrderStatus!]"`).
fn coerce_type_ref(schema: &CompiledSchema, declared: &str, value: &mut Value, depth: usize) {
    if depth > MAX_COERCION_DEPTH {
        return;
    }
    let declared = declared.trim();
    if let Some(inner) = declared.strip_suffix('!') {
        return coerce_type_ref(schema, inner, value, depth);
    }
    if let Some(inner) = declared.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        return each_item(value, |item| coerce_type_ref(schema, inner, item, depth + 1));
    }
    if value.is_null() {
        return;
    }
    if schema.find_enum(declared).is_some() {
        return coerce_member(schema, declared, value);
    }
    coerce_input_object(schema, declared, value, depth);
}

/// Apply `coerce` to each item of a list, or to a bare value standing for a one-item list
/// (§ 3.11, `in: PENDING`), as the validator walks it.
fn each_item(value: &mut Value, mut coerce: impl FnMut(&mut Value)) {
    match value.as_array_mut() {
        Some(items) => items.iter_mut().for_each(coerce),
        None => coerce(value),
    }
}

/// Coerce the fields of the input object `name`, under the surface or the stored key.
fn coerce_input_object(schema: &CompiledSchema, name: &str, value: &mut Value, depth: usize) {
    let (Some(input_type), Some(object)) = (schema.find_input_type(name), value.as_object_mut())
    else {
        return;
    };
    for field in &input_type.fields {
        let surface = schema.display_name(&field.name);
        for key in [surface.as_str(), field.name.as_str()] {
            if let Some(value) = object.get_mut(key) {
                coerce_type_ref(schema, &field.field_type, value, depth + 1);
            }
            if surface == field.name {
                break;
            }
        }
    }
}

/// A member name becomes the value it is stored as.
fn coerce_member(schema: &CompiledSchema, enum_name: &str, value: &mut Value) {
    let Some(def) = schema.find_enum(enum_name) else {
        return;
    };
    let Some(stored) = value.as_str().and_then(|name| def.stored_value_of(name)) else {
        return;
    };
    if value.as_str() != Some(stored) {
        *value = Value::String(stored.to_string());
    }
}

#[cfg(test)]
#[path = "enum_coercion_tests.rs"]
mod enum_coercion_tests;
