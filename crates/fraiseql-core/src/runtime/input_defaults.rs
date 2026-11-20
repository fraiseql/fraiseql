//! Input object field defaults (GraphQL, Input Objects → Input Coercion).
//!
//! A field the request omits takes its declared default; a field the request sends keeps
//! what was sent, null included. A null sent for a non-null field that declares a default
//! is refused: the default stands in for an absent field only, never for a null.
//!
//! Applied to an operation's arguments before they are validated and enum-coerced, so a
//! default is checked and stored like a sent value. The walk follows input objects at any
//! depth, and the lists of them, under the field's surface or stored key. An object carrying
//! a key its type does not declare is not an instance of that type and is left as sent.

use std::collections::HashMap;

use serde_json::Value;

use crate::{
    error::{FraiseQLError, Result},
    schema::{ArgumentDefinition, CompiledSchema, FieldType},
};

/// How deep the walk follows a value; a request is depth-limited upstream.
const MAX_DEPTH: usize = 64;

/// Whether an argument of `declared` can carry an input object, whose omitted fields could
/// take a default: one typed as an input object, directly or as the item of a list. Known per
/// operation once its arguments are, so a read asks it once, not per request.
#[must_use]
pub fn takes_input_objects(declared: &[ArgumentDefinition]) -> bool {
    fn input_object(declared: &FieldType) -> bool {
        match declared {
            FieldType::List { item, .. } => input_object(item),
            // An input-type reference compiles as `Object`.
            FieldType::Object(_) | FieldType::Input(_) => true,
            _ => false,
        }
    }
    declared.iter().any(|arg| input_object(&arg.arg_type))
}

/// Apply input field defaults to a mutation's argument object: the coerced copy, or `None`
/// when no input type of `schema` declares a default or no argument takes an input object
/// (the caller keeps its own). `operation` names it in a refusal, built only when the
/// arguments are walked.
///
/// # Errors
///
/// [`FraiseQLError::Validation`] for a null sent at a non-null field that declares a
/// default, or a default that is not JSON.
pub fn apply_input_defaults_to_object(
    schema: &CompiledSchema,
    operation: impl FnOnce() -> String,
    declared: &[ArgumentDefinition],
    arguments: Option<&Value>,
) -> Result<Option<Value>> {
    if !schema.declares_input_defaults() || !takes_input_objects(declared) {
        return Ok(None);
    }
    let Some(mut arguments) = arguments.filter(|a| a.is_object()).cloned() else {
        return Ok(None);
    };
    let operation = operation();
    let operation = operation.as_str();
    if let Some(map) = arguments.as_object_mut() {
        for arg in declared {
            if let Some(value) = map.get_mut(&arg.name) {
                walk_field_type(schema, operation, &arg.name, &arg.arg_type, value, 0)?;
            }
        }
    }
    Ok(Some(arguments))
}

/// Apply input field defaults to a read's merged `arguments`, in place. The caller has
/// established that the schema declares a default and that `declared` takes an input
/// object ([`takes_input_objects`]), once per query rather than per request.
///
/// # Errors
///
/// As [`apply_input_defaults_to_object`].
pub fn apply_input_defaults(
    schema: &CompiledSchema,
    operation: &str,
    declared: &[ArgumentDefinition],
    arguments: &mut HashMap<String, Value>,
) -> Result<()> {
    for arg in declared {
        if let Some(value) = arguments.get_mut(&arg.name) {
            walk_field_type(schema, operation, &arg.name, &arg.arg_type, value, 0)?;
        }
    }
    Ok(())
}

/// Walk `value` at a position of compiled type `declared`, reached at `path`.
fn walk_field_type(
    schema: &CompiledSchema,
    operation: &str,
    path: &str,
    declared: &FieldType,
    value: &mut Value,
    depth: usize,
) -> Result<()> {
    if value.is_null() || depth > MAX_DEPTH {
        return Ok(());
    }
    match declared {
        FieldType::List { item, .. } => each_item(value, path, |item_path, item_value| {
            walk_field_type(schema, operation, item_path, item, item_value, depth + 1)
        }),
        // An input-type reference compiles as `Object`.
        FieldType::Object(name) | FieldType::Input(name) => {
            walk_input_object(schema, operation, path, name, value, depth)
        },
        _ => Ok(()),
    }
}

/// Walk `value` at a position of written type `declared` (an input field's: `"[LineInput!]"`).
fn walk_type_ref(
    schema: &CompiledSchema,
    operation: &str,
    path: &str,
    declared: &str,
    value: &mut Value,
    depth: usize,
) -> Result<()> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    let declared = declared.trim();
    if let Some(inner) = declared.strip_suffix('!') {
        return walk_type_ref(schema, operation, path, inner, value, depth);
    }
    if let Some(inner) = declared.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        return each_item(value, path, |item_path, item_value| {
            walk_type_ref(schema, operation, item_path, inner, item_value, depth + 1)
        });
    }
    if value.is_null() {
        return Ok(());
    }
    walk_input_object(schema, operation, path, declared, value, depth)
}

/// Apply `walk` to each item of a list, or to a bare value standing for a one-item list.
fn each_item(
    value: &mut Value,
    path: &str,
    mut walk: impl FnMut(&str, &mut Value) -> Result<()>,
) -> Result<()> {
    match value.as_array_mut() {
        Some(items) => {
            for (index, item) in items.iter_mut().enumerate() {
                walk(&format!("{path}.{index}"), item)?;
            }
            Ok(())
        },
        None => walk(path, value),
    }
}

/// Fill the omitted defaulted fields of the input object `name`, and walk the sent ones.
fn walk_input_object(
    schema: &CompiledSchema,
    operation: &str,
    path: &str,
    name: &str,
    value: &mut Value,
    depth: usize,
) -> Result<()> {
    let (Some(input_type), Some(object)) = (schema.find_input_type(name), value.as_object_mut())
    else {
        return Ok(());
    };
    // A key the type does not declare: a shape the type does not describe (the object form
    // of `orderBy`), left to the reader that accepts it, or to the validator that refuses it.
    let describes = |key: &String| {
        input_type
            .fields
            .iter()
            .any(|f| f.name == *key || schema.display_name(&f.name) == *key)
    };
    if !object.keys().all(describes) {
        return Ok(());
    }
    for field in &input_type.fields {
        let surface = schema.display_name(&field.name);
        let field_path = format!("{path}.{surface}");
        let sent_key = [surface.as_str(), field.name.as_str()]
            .into_iter()
            .find(|key| object.contains_key(*key))
            .map(str::to_owned);
        match (sent_key, &field.default_value) {
            (Some(key), default) => {
                let Some(sent) = object.get_mut(&key) else {
                    continue;
                };
                if sent.is_null() && !field.nullable && default.is_some() {
                    return Err(FraiseQLError::Validation {
                        message: format!(
                            "{operation}: input field `{surface}` of `{name}` is non-null; a \
                             null was sent for it. Its default applies only when the field is \
                             omitted"
                        ),
                        path:    Some(field_path),
                    });
                }
                walk_type_ref(schema, operation, &field_path, &field.field_type, sent, depth + 1)?;
            },
            (None, Some(default)) => {
                let mut value: Value =
                    serde_json::from_str(default).map_err(|e| FraiseQLError::Validation {
                        message: format!(
                            "{operation}: the default of input field `{surface}` of `{name}` \
                             is not JSON ({e}): {default}"
                        ),
                        path:    Some(field_path.clone()),
                    })?;
                walk_type_ref(
                    schema,
                    operation,
                    &field_path,
                    &field.field_type,
                    &mut value,
                    depth + 1,
                )?;
                object.insert(surface, value);
            },
            (None, None) => {},
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "input_defaults_tests.rs"]
mod input_defaults_tests;
