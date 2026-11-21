//! Type coercion for REST query parameters.

use fraiseql_core::schema::FieldType;
use fraiseql_error::FraiseQLError;

use crate::routes::rest::params::helpers::validation_error;

/// Coerce the raw string written for `name` to a JSON value of its `FieldType`.
///
/// A temporal value is held to its scalar's format here, as an argument is on `/graphql`:
/// a REST path parameter is never validated as a GraphQL argument, and a filter operand
/// is refused at the edge, naming the field, before the `where` generator sees it.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the value cannot be parsed as the
/// expected type.
pub fn coerce_to_type(
    name: &str,
    raw: &str,
    field_type: &FieldType,
) -> Result<serde_json::Value, FraiseQLError> {
    if let Some(temporal) = field_type.temporal() {
        let value = serde_json::Value::String(raw.to_string());
        temporal.check_operand(name, &value)?;
        return Ok(value);
    }
    match field_type {
        FieldType::Int => {
            let v: i64 = raw
                .parse()
                .map_err(|_| validation_error(format!("Expected integer value, got '{raw}'.")))?;
            Ok(serde_json::Value::Number(v.into()))
        },
        // The digits travel as the string they were written as: a JSON number past 2^53
        // is not read exactly by every client of the value.
        FieldType::BigInt => fraiseql_core::runtime::parse_big_int(raw)
            .map(|_| serde_json::Value::String(raw.to_string()))
            .ok_or_else(|| {
                validation_error(format!("Expected a signed 64-bit integer value, got '{raw}'."))
            }),
        FieldType::Float | FieldType::Decimal => {
            let v: f64 = raw
                .parse()
                .map_err(|_| validation_error(format!("Expected numeric value, got '{raw}'.")))?;
            Ok(serde_json::Number::from_f64(v).map_or_else(
                || serde_json::Value::String(raw.to_string()),
                serde_json::Value::Number,
            ))
        },
        FieldType::Boolean => {
            let v = match raw {
                "true" | "1" | "yes" => true,
                "false" | "0" | "no" => false,
                _ => {
                    return Err(validation_error(format!(
                        "Expected boolean value (true/false/1/0), got '{raw}'."
                    )));
                },
            };
            Ok(serde_json::Value::Bool(v))
        },
        FieldType::Json => serde_json::from_str(raw)
            .map_err(|e| validation_error(format!("Expected JSON value, got '{raw}': {e}"))),
        FieldType::List { item: _, .. } => {
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
        // Id, Uuid, String, Scalar, Enum, Object, etc. — pass through as
        // string.
        _ => Ok(serde_json::Value::String(raw.to_string())),
    }
}
