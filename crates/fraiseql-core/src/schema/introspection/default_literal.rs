//! Default values as GraphQL literals.
//!
//! A compiled schema stores a default as JSON: an input field's as JSON text, an
//! argument's as a [`GraphQLValue`](crate::schema::GraphQLValue). JSON is not GraphQL:
//! an enum value is a JSON string but a bare GraphQL name, and an object's keys are
//! quoted in JSON and bare in GraphQL. `__InputValue.defaultValue` is specified as a
//! GraphQL literal, and the SDL prints that same value, so both are rendered here, from
//! the declared type, in one place.
//!
//! A default that has no literal of its type (an enum value the enum lacks, a key the
//! input object lacks, an object key that is not a GraphQL name) is refused when the
//! schema loads: dropping it would turn an optional field into a required one.

use serde_json::Value;

use super::{
    super::{ArgumentDefinition, CompiledSchema, InputFieldDefinition},
    field_resolver::arg_type_ref,
    type_resolver::input_field_type_ref,
    types::{IntrospectionType, TypeKind},
};

/// An input field's default as a GraphQL literal, `None` when it has none.
///
/// # Errors
///
/// Why the stored default has no literal of the field's type.
pub fn input_field_default(
    field: &InputFieldDefinition,
    schema: &CompiledSchema,
) -> Result<Option<String>, String> {
    let Some(text) = field.default_value.as_deref() else {
        return Ok(None);
    };
    let value: Value =
        serde_json::from_str(text).map_err(|e| format!("`{text}` is not JSON text: {e}"))?;
    literal(&value, &input_field_type_ref(field, schema), schema).map(Some)
}

/// An argument's default as a GraphQL literal, `None` when it has none.
///
/// # Errors
///
/// Why the stored default has no literal of the argument's type.
pub fn argument_default(
    arg: &ArgumentDefinition,
    schema: &CompiledSchema,
) -> Result<Option<String>, String> {
    arg.default_value
        .as_ref()
        .map(|v| literal(&v.to_json(), &arg_type_ref(arg), schema))
        .transpose()
}

/// `value` as a literal of the type `ty` references.
fn literal(
    value: &Value,
    ty: &IntrospectionType,
    schema: &CompiledSchema,
) -> Result<String, String> {
    let inner = || {
        ty.of_type
            .as_deref()
            .ok_or_else(|| "a wrapper type names no inner type".to_string())
    };
    match ty.kind {
        TypeKind::NonNull if value.is_null() => {
            Err("`null` is not a value of a non-null type".to_string())
        },
        TypeKind::NonNull => literal(value, inner()?, schema),
        TypeKind::List => match value {
            Value::Null => Ok("null".to_string()),
            Value::Array(items) => {
                let item_type = inner()?;
                let items = items
                    .iter()
                    .map(|v| literal(v, item_type, schema))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(format!("[{}]", items.join(", ")))
            },
            // Input coercion accepts one item where a list is expected (§3.11).
            one => literal(one, inner()?, schema),
        },
        _ => named(value, ty.name.as_deref().unwrap_or_default(), schema),
    }
}

/// `value` as a literal of the named type `name`, resolved against the schema rather
/// than the reference's kind: a written type string carries only the name.
fn named(value: &Value, name: &str, schema: &CompiledSchema) -> Result<String, String> {
    if value.is_null() {
        return Ok("null".to_string());
    }
    if let Some(enum_def) = schema.find_enum(name) {
        return match value {
            Value::String(v) if enum_def.values.iter().any(|e| &e.name == v) => Ok(v.clone()),
            Value::String(v) => Err(format!("`{v}` is not a value of enum `{name}`")),
            other => Err(format!("enum `{name}` takes a value name, not `{other}`")),
        };
    }
    if let Some(input) = schema.find_input_type(name) {
        let Value::Object(entries) = value else {
            return Err(format!("input `{name}` takes an object, not `{value}`"));
        };
        let fields = entries
            .iter()
            .map(|(key, v)| {
                let field = input
                    .fields
                    .iter()
                    .find(|f| &f.name == key)
                    .ok_or_else(|| format!("`{key}` is not a field of input `{name}`"))?;
                Ok(format!("{key}: {}", literal(v, &input_field_type_ref(field, schema), schema)?))
            })
            .collect::<Result<Vec<_>, String>>()?;
        return Ok(format!("{{{}}}", fields.join(", ")));
    }
    scalar(value)
}

/// A scalar's value, whose GraphQL literal follows its JSON shape. A custom scalar
/// (`JSON`) can hold an object, whose keys must then be GraphQL names.
fn scalar(value: &Value) -> Result<String, String> {
    match value {
        Value::Null => Ok("null".to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        // JSON's string escapes (`\"`, `\\`, `\n`, `\uXXXX`, …) are all GraphQL escapes.
        Value::String(s) => serde_json::to_string(s).map_err(|e| e.to_string()),
        Value::Array(items) => {
            let items = items.iter().map(scalar).collect::<Result<Vec<_>, _>>()?;
            Ok(format!("[{}]", items.join(", ")))
        },
        Value::Object(entries) => {
            let fields = entries
                .iter()
                .map(|(key, v)| {
                    if !is_name(key) {
                        return Err(format!(
                            "key `{key}` is not a GraphQL name, so it has no literal"
                        ));
                    }
                    Ok(format!("{key}: {}", scalar(v)?))
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(format!("{{{}}}", fields.join(", ")))
        },
    }
}

/// `/[_A-Za-z][_0-9A-Za-z]*/` (§2.1.9).
fn is_name(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}
