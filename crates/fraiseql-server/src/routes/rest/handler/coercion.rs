//! Path parameter type coercion.

use fraiseql_core::schema::{CompiledSchema, FieldType};
use serde_json::json;

/// Coerce a URL path segment to the JSON type the schema declares for it.
///
/// A path segment is textually a string; whether it *means* an integer is a
/// property of the mutation argument it binds to, not of how it happens to be
/// spelled. Coercing by parse-ability instead turned the string ID `"0123"` into
/// the integer `123` and the string `"true"` into a boolean regardless of the
/// declared type (#731) — a silent corruption on the way in, invisible to the
/// client that sent a perfectly valid ID.
///
/// `declared = None` means the schema has no argument by that name, in which case
/// the value stays a string: inventing a type for an argument nothing declares is
/// exactly the guess that caused the defect.
#[must_use]
pub(super) fn coerce_path_param_value(
    value: &str,
    declared: Option<&FieldType>,
) -> serde_json::Value {
    match declared {
        Some(FieldType::Int) => value.parse::<i64>().map_or_else(|_| json!(value), |n| json!(n)),
        Some(FieldType::Float) => value.parse::<f64>().map_or_else(|_| json!(value), |n| json!(n)),
        Some(FieldType::Boolean) => match value {
            "true" => json!(true),
            "false" => json!(false),
            // Not a boolean literal: hand the raw text to the input validator so
            // the client gets "expected Boolean", not a silently wrong value.
            other => json!(other),
        },
        // String, ID, UUID, Date/DateTime/Time, Decimal, JSON, enums, custom
        // scalars — and unknown arguments — all stay exactly as the URL spelled
        // them. `Decimal` in particular is serialized as a string for precision,
        // so parsing it to a float here would lose digits.
        _ => json!(value),
    }
}

/// The declared type of `arg_name` on `mutation_name`, if the schema has one.
#[must_use]
pub(super) fn declared_arg_type<'a>(
    schema: &'a CompiledSchema,
    mutation_name: &str,
    arg_name: &str,
) -> Option<&'a FieldType> {
    schema
        .find_mutation(mutation_name)?
        .arguments
        .iter()
        .find(|a| a.name == arg_name)
        .map(|a| &a.arg_type)
}
