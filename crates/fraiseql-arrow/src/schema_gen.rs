//! Dynamic Arrow schema generation from GraphQL types.
//!
//! This module maps GraphQL scalar types to Apache Arrow data types
//! and generates Arrow schemas from GraphQL query result shapes.

use std::{collections::HashMap, sync::Arc};

use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use serde_json::Value;

/// Map GraphQL scalar types to Arrow types.
///
/// # Arguments
///
/// * `graphql_type` - GraphQL type name (e.g., "String", "Int", "DateTime")
/// * `nullable` - Whether the field is nullable
///
/// # Returns
///
/// The corresponding Arrow DataType
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
/// * `fields` - Vector of (field_name, graphql_type, nullable) tuples
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
/// * `rows` - Vector of HashMap representing database rows
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
) -> Result<Arc<Schema>, Box<dyn std::error::Error>> {
    if rows.is_empty() {
        return Err("Cannot infer schema from empty rows".into());
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
/// * `value` - serde_json::Value to infer type from
///
/// # Returns
///
/// Corresponding Arrow DataType
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
mod tests {
    use super::*;

    #[test]
    fn test_graphql_to_arrow_scalars() {
        assert_eq!(graphql_type_to_arrow("String", false), DataType::Utf8);
        assert_eq!(graphql_type_to_arrow("Int", false), DataType::Int32);
        assert_eq!(graphql_type_to_arrow("Float", false), DataType::Float64);
        assert_eq!(graphql_type_to_arrow("Boolean", false), DataType::Boolean);
        assert_eq!(graphql_type_to_arrow("ID", false), DataType::Utf8);
    }

    #[test]
    fn test_graphql_to_arrow_custom_scalars() {
        assert_eq!(graphql_type_to_arrow("UUID", false), DataType::Utf8);
        assert_eq!(graphql_type_to_arrow("JSON", false), DataType::Utf8);
        assert_eq!(graphql_type_to_arrow("Date", false), DataType::Date32);
        assert_eq!(graphql_type_to_arrow("Time", false), DataType::Time64(TimeUnit::Nanosecond));
        assert_eq!(graphql_type_to_arrow("Decimal", false), DataType::Decimal128(38, 10));
    }

    #[test]
    fn test_datetime_mapping() {
        let dt_type = graphql_type_to_arrow("DateTime", false);
        match dt_type {
            DataType::Timestamp(TimeUnit::Nanosecond, Some(tz)) => {
                assert_eq!(tz.as_ref(), "UTC");
            },
            _ => panic!("Expected Timestamp(Nanosecond, UTC), got {:?}", dt_type),
        }
    }

    #[test]
    fn test_unknown_type_defaults_to_string() {
        assert_eq!(graphql_type_to_arrow("UnknownCustomType", false), DataType::Utf8);
    }

    #[test]
    fn test_generate_arrow_schema() {
        let fields = vec![
            ("id".to_string(), "ID".to_string(), false),
            ("name".to_string(), "String".to_string(), true),
            ("age".to_string(), "Int".to_string(), true),
        ];

        let schema = generate_arrow_schema(&fields);

        assert_eq!(schema.fields().len(), 3);

        assert_eq!(schema.field(0).name(), "id");
        assert_eq!(schema.field(0).data_type(), &DataType::Utf8);
        assert!(!schema.field(0).is_nullable());

        assert_eq!(schema.field(1).name(), "name");
        assert_eq!(schema.field(1).data_type(), &DataType::Utf8);
        assert!(schema.field(1).is_nullable());

        assert_eq!(schema.field(2).name(), "age");
        assert_eq!(schema.field(2).data_type(), &DataType::Int32);
        assert!(schema.field(2).is_nullable());
    }

    #[test]
    fn test_generate_schema_with_datetime() {
        let fields = vec![
            ("created_at".to_string(), "DateTime".to_string(), false),
            ("updated_at".to_string(), "DateTime".to_string(), true),
        ];

        let schema = generate_arrow_schema(&fields);

        assert_eq!(schema.fields().len(), 2);
        assert!(!schema.field(0).is_nullable());
        assert!(schema.field(1).is_nullable());

        match schema.field(0).data_type() {
            DataType::Timestamp(TimeUnit::Nanosecond, Some(tz)) => {
                assert_eq!(tz.as_ref(), "UTC");
            },
            _ => panic!("Expected Timestamp type"),
        }
    }

    #[test]
    fn test_empty_schema() {
        let fields: Vec<(String, String, bool)> = vec![];
        let schema = generate_arrow_schema(&fields);
        assert_eq!(schema.fields().len(), 0);
    }
}
