//! Type coercion for REST query parameters.

use fraiseql_core::schema::FieldType;
use fraiseql_error::FraiseQLError;

use crate::routes::rest::params::helpers::validation_error;

/// Coerce the raw string written for `name` to a JSON value of its `FieldType`.
///
/// A built-in scalar's value is read as the JSON a client would have written for it and
/// held to the rule every built-in scalar's value is held to, as an argument and a `where`
/// operand are on `/graphql`: a REST path parameter is never validated as a GraphQL
/// argument, and a filter operand is refused at the edge, naming the field and its type and
/// never the value, before the `where` generator sees it.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the value is not one of the expected type.
pub fn coerce_to_type(
    name: &str,
    raw: &str,
    field_type: &FieldType,
) -> Result<serde_json::Value, FraiseQLError> {
    let text = || serde_json::Value::String(raw.to_string());
    let scalar = match field_type {
        FieldType::Int => Some(raw.parse::<i64>().map_or_else(|_| text(), Into::into)),
        FieldType::Float => Some(
            raw.parse::<f64>()
                .ok()
                .and_then(serde_json::Number::from_f64)
                .map_or_else(text, serde_json::Value::Number),
        ),
        FieldType::Boolean => Some(match raw {
            "true" | "1" | "yes" => serde_json::Value::Bool(true),
            "false" | "0" | "no" => serde_json::Value::Bool(false),
            _ => text(),
        }),
        // The digits travel as the string they were written as: a JSON number is not read
        // exactly past 2^53, nor a decimal fraction at all.
        FieldType::BigInt
        | FieldType::Decimal
        | FieldType::Uuid
        | FieldType::DateTime
        | FieldType::LocalDateTime
        | FieldType::Date
        | FieldType::Time => Some(text()),
        _ => None,
    };
    if let Some(value) = scalar {
        fraiseql_core::runtime::check_scalar_value(name, field_type, &value)?;
        return Ok(value);
    }
    match field_type {
        FieldType::Json => serde_json::from_str(raw)
            .map_err(|_| validation_error(format!("The value of `{name}` is not a JSON value."))),
        FieldType::List { .. } => {
            // Try JSON array first, then comma-separated.
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
                if v.is_array() {
                    return Ok(v);
                }
            }
            // Comma-separated string values.
            let items: Vec<serde_json::Value> = raw
                .split(',')
                .map(|s| serde_json::Value::String(s.trim().to_string()))
                .collect();
            Ok(serde_json::Value::Array(items))
        },
        // Id, String, Scalar, Enum, Object, etc. — pass through as string.
        _ => Ok(serde_json::Value::String(raw.to_string())),
    }
}
