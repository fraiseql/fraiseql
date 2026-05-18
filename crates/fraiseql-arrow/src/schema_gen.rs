//! Dynamic Arrow schema generation from GraphQL types.
//!
//! This module maps GraphQL scalar types to Apache Arrow data types
//! and generates Arrow schemas from GraphQL query result shapes.

use std::{collections::HashMap, sync::Arc};

use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use serde_json::Value;

use crate::error::ArrowFlightError;

/// Map GraphQL scalar types to Arrow types.
///
/// # Arguments
///
/// * `graphql_type` - GraphQL type name (e.g., "String", "Int", "`DateTime`")
/// * `nullable` - Whether the field is nullable
///
/// # Returns
///
/// The corresponding Arrow `DataType`
///
/// # Example
///
/// ```
/// use fraiseql_arrow::schema_gen::graphql_type_to_arrow;
/// use arrow::datatypes::DataType;
///
/// let arrow_type = graphql_type_to_arrow("String", false);
/// assert_eq!(arrow_type, DataType::Utf8);
/// ```
#[must_use] 
pub fn graphql_type_to_arrow(graphql_type: &str, _nullable: bool) -> DataType {
    match graphql_type {
        // GraphQL scalars
        "String" => DataType::Utf8,
        "Int" => DataType::Int32,
        "Float" => DataType::Float64,
        "Boolean" => DataType::Boolean,
        "ID" => DataType::Utf8,

        // Custom scalars (common extensions)
        "DateTime" => DataType::Timestamp(TimeUnit::Nanosecond, Some(Arc::from("UTC"))),
        "Date" => DataType::Date32,
        "Time" => DataType::Time64(TimeUnit::Nanosecond),
        "UUID" => DataType::Utf8,                  // UUIDs as strings
        "JSON" => DataType::Utf8,                  // JSON as string for now
        "Decimal" => DataType::Decimal128(38, 10), // Default precision

        // Unknown types default to JSON strings
        _ => DataType::Utf8,
    }
}

/// Generate Arrow schema from GraphQL query result shape.
///
/// # Arguments
///
/// * `fields` - Vector of (`field_name`, `graphql_type`, nullable) tuples
///
/// # Returns
///
/// Arrow Schema with fields mapped from GraphQL types
///
/// # Example
///
/// ```
/// use fraiseql_arrow::schema_gen::generate_arrow_schema;
///
/// let fields = vec![
///     ("id".to_string(), "ID".to_string(), false),
///     ("name".to_string(), "String".to_string(), true),
///     ("age".to_string(), "Int".to_string(), true),
/// ];
///
/// let schema = generate_arrow_schema(&fields);
/// assert_eq!(schema.fields().len(), 3);
/// ```
#[must_use] 
pub fn generate_arrow_schema(fields: &[(String, String, bool)]) -> Arc<Schema> {
    let arrow_fields: Vec<Field> = fields
        .iter()
        .map(|(name, graphql_type, nullable)| {
            let arrow_type = graphql_type_to_arrow(graphql_type, *nullable);
            Field::new(name, arrow_type, *nullable)
        })
        .collect();

    Arc::new(Schema::new(arrow_fields))
}

/// Infer Arrow schema from raw database rows (JSON objects).
///
/// Examines the first row to determine field names and infers data types
/// from JSON value types. All fields are nullable by default.
///
/// # Arguments
///
/// * `rows` - Vector of `HashMap` representing database rows
///
/// # Returns
///
/// Arrow Schema inferred from the rows
///
/// # Errors
///
/// Returns error if rows are empty or if schema inference fails
pub fn infer_schema_from_rows(
    rows: &[HashMap<String, Value>],
) -> Result<Arc<Schema>, ArrowFlightError> {
    if rows.is_empty() {
        return Err(ArrowFlightError::SchemaNotFound(
            "Cannot infer schema from empty rows".to_string(),
        ));
    }

    let first_row = &rows[0];
    let arrow_fields: Vec<Field> = first_row
        .iter()
        .map(|(name, value)| {
            let arrow_type = infer_type_from_value(value);
            Field::new(name.clone(), arrow_type, true) // All fields nullable
        })
        .collect();

    Ok(Arc::new(Schema::new(arrow_fields)))
}

/// Infer Arrow data type from a JSON value.
///
/// # Arguments
///
/// * `value` - `serde_json::Value` to infer type from
///
/// # Returns
///
/// Corresponding Arrow `DataType`
fn infer_type_from_value(value: &Value) -> DataType {
    match value {
        Value::Null => DataType::Null,
        Value::Bool(_) => DataType::Boolean,
        Value::Number(n) => {
            if n.is_i64() {
                DataType::Int64
            } else {
                DataType::Float64
            }
        },
        Value::String(_) => DataType::Utf8,
        Value::Array(_) => DataType::Utf8, // JSON arrays as strings
        Value::Object(_) => DataType::Utf8, // JSON objects as strings
    }
}

#[cfg(test)]
mod tests;
