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

    // --- Additional field mapping tests ---

    #[test]
    fn test_non_nullable_int_field_maps_to_required_int32() {
        let fields = vec![("count".to_string(), "Int".to_string(), false)];
        let schema = generate_arrow_schema(&fields);
        assert_eq!(schema.field(0).data_type(), &DataType::Int32);
        assert!(!schema.field(0).is_nullable());
    }

    #[test]
    fn test_nullable_string_field_maps_to_nullable_utf8() {
        let fields = vec![("description".to_string(), "String".to_string(), true)];
        let schema = generate_arrow_schema(&fields);
        assert_eq!(schema.field(0).data_type(), &DataType::Utf8);
        assert!(schema.field(0).is_nullable());
    }

    #[test]
    fn test_non_nullable_boolean_maps_to_required_boolean() {
        let fields = vec![("active".to_string(), "Boolean".to_string(), false)];
        let schema = generate_arrow_schema(&fields);
        assert_eq!(schema.field(0).data_type(), &DataType::Boolean);
        assert!(!schema.field(0).is_nullable());
    }

    #[test]
    fn test_float_scalar_maps_to_float64() {
        let fields = vec![("price".to_string(), "Float".to_string(), true)];
        let schema = generate_arrow_schema(&fields);
        assert_eq!(schema.field(0).data_type(), &DataType::Float64);
    }

    #[test]
    fn test_id_scalar_maps_to_utf8() {
        let fields = vec![("id".to_string(), "ID".to_string(), false)];
        let schema = generate_arrow_schema(&fields);
        assert_eq!(schema.field(0).data_type(), &DataType::Utf8);
        assert!(!schema.field(0).is_nullable());
    }

    #[test]
    fn test_datetime_scalar_maps_to_timestamp_microsecond_utc() {
        let fields = vec![("created_at".to_string(), "DateTime".to_string(), false)];
        let schema = generate_arrow_schema(&fields);
        match schema.field(0).data_type() {
            DataType::Timestamp(TimeUnit::Nanosecond, Some(tz)) => {
                assert_eq!(tz.as_ref(), "UTC");
            },
            other => panic!("Expected Timestamp(Nanosecond, UTC), got {:?}", other),
        }
    }

    #[test]
    fn test_date_scalar_maps_to_date32() {
        let fields = vec![("birth_date".to_string(), "Date".to_string(), false)];
        let schema = generate_arrow_schema(&fields);
        assert_eq!(schema.field(0).data_type(), &DataType::Date32);
    }

    #[test]
    fn test_uuid_scalar_maps_to_utf8() {
        let fields = vec![("user_uuid".to_string(), "UUID".to_string(), false)];
        let schema = generate_arrow_schema(&fields);
        assert_eq!(schema.field(0).data_type(), &DataType::Utf8);
    }

    #[test]
    fn test_json_scalar_maps_to_utf8() {
        let fields = vec![("metadata".to_string(), "JSON".to_string(), true)];
        let schema = generate_arrow_schema(&fields);
        assert_eq!(schema.field(0).data_type(), &DataType::Utf8);
    }

    #[test]
    fn test_decimal_scalar_maps_to_decimal128() {
        let fields = vec![("amount".to_string(), "Decimal".to_string(), false)];
        let schema = generate_arrow_schema(&fields);
        assert_eq!(schema.field(0).data_type(), &DataType::Decimal128(38, 10));
    }

    #[test]
    fn test_unknown_scalar_type_falls_back_to_utf8() {
        let fields = vec![("custom".to_string(), "MyCustomScalar".to_string(), true)];
        let schema = generate_arrow_schema(&fields);
        assert_eq!(schema.field(0).data_type(), &DataType::Utf8);
    }

    #[test]
    fn test_schema_with_one_field() {
        let fields = vec![("only".to_string(), "Int".to_string(), false)];
        let schema = generate_arrow_schema(&fields);
        assert_eq!(schema.fields().len(), 1);
        assert_eq!(schema.field(0).name(), "only");
    }

    #[test]
    fn test_schema_determinism_same_input_same_output() {
        let fields = vec![
            ("id".to_string(), "ID".to_string(), false),
            ("name".to_string(), "String".to_string(), true),
            ("score".to_string(), "Float".to_string(), true),
            ("active".to_string(), "Boolean".to_string(), false),
        ];
        let schema1 = generate_arrow_schema(&fields);
        let schema2 = generate_arrow_schema(&fields);
        assert_eq!(schema1.fields().len(), schema2.fields().len());
        for (f1, f2) in schema1.fields().iter().zip(schema2.fields().iter()) {
            assert_eq!(f1.name(), f2.name());
            assert_eq!(f1.data_type(), f2.data_type());
            assert_eq!(f1.is_nullable(), f2.is_nullable());
        }
    }

    #[test]
    fn test_nullable_flag_propagated_correctly() {
        let fields = vec![
            ("required_field".to_string(), "String".to_string(), false),
            ("optional_field".to_string(), "String".to_string(), true),
        ];
        let schema = generate_arrow_schema(&fields);
        assert!(!schema.field(0).is_nullable(), "required_field must not be nullable");
        assert!(schema.field(1).is_nullable(), "optional_field must be nullable");
    }

    #[test]
    fn test_field_names_are_preserved() {
        let fields = vec![
            ("user_id".to_string(), "ID".to_string(), false),
            ("email_address".to_string(), "String".to_string(), true),
            ("created_at_timestamp".to_string(), "DateTime".to_string(), false),
        ];
        let schema = generate_arrow_schema(&fields);
        assert_eq!(schema.field(0).name(), "user_id");
        assert_eq!(schema.field(1).name(), "email_address");
        assert_eq!(schema.field(2).name(), "created_at_timestamp");
    }

    #[test]
    fn test_infer_schema_from_empty_rows_returns_error() {
        let rows: Vec<HashMap<String, Value>> = vec![];
        let result = infer_schema_from_rows(&rows);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cannot infer schema from empty rows"));
    }

    #[test]
    fn test_infer_schema_from_rows_with_integer() {
        let mut row = HashMap::new();
        row.insert("count".to_string(), Value::from(42i64));
        let rows = vec![row];
        let schema = infer_schema_from_rows(&rows).unwrap();
        assert_eq!(schema.fields().len(), 1);
        let field = schema.field(0);
        assert_eq!(field.name(), "count");
        assert_eq!(field.data_type(), &DataType::Int64);
    }

    #[test]
    fn test_infer_schema_from_rows_with_float() {
        let mut row = HashMap::new();
        row.insert("price".to_string(), Value::from(9.99f64));
        let rows = vec![row];
        let schema = infer_schema_from_rows(&rows).unwrap();
        let field = schema.field(0);
        assert_eq!(field.data_type(), &DataType::Float64);
    }

    #[test]
    fn test_infer_schema_from_rows_with_string() {
        let mut row = HashMap::new();
        row.insert("name".to_string(), Value::from("Alice"));
        let rows = vec![row];
        let schema = infer_schema_from_rows(&rows).unwrap();
        let field = schema.field(0);
        assert_eq!(field.data_type(), &DataType::Utf8);
    }

    #[test]
    fn test_infer_schema_from_rows_uses_first_row_only() {
        let mut row1 = HashMap::new();
        row1.insert("id".to_string(), Value::from(1i64));

        let mut row2 = HashMap::new();
        row2.insert("extra_column".to_string(), Value::from("extra"));

        // Second row has a different key; schema should only reflect first row
        let rows = vec![row1, row2];
        let schema = infer_schema_from_rows(&rows).unwrap();
        assert_eq!(schema.fields().len(), 1);
        assert_eq!(schema.field(0).name(), "id");
    }

    #[test]
    fn test_infer_schema_all_fields_are_nullable() {
        let mut row = HashMap::new();
        row.insert("a".to_string(), Value::from(1i64));
        row.insert("b".to_string(), Value::from("test"));
        let rows = vec![row];
        let schema = infer_schema_from_rows(&rows).unwrap();
        for field in schema.fields() {
            assert!(field.is_nullable(), "inferred fields must be nullable");
        }
    }

    #[test]
    fn test_infer_schema_from_rows_null_value_gives_null_type() {
        let mut row = HashMap::new();
        row.insert("unknown".to_string(), Value::Null);
        let rows = vec![row];
        let schema = infer_schema_from_rows(&rows).unwrap();
        assert_eq!(schema.field(0).data_type(), &DataType::Null);
    }

    #[test]
    fn test_infer_schema_from_rows_array_value_gives_utf8() {
        let mut row = HashMap::new();
        row.insert("tags".to_string(), Value::Array(vec![Value::from("a"), Value::from("b")]));
        let rows = vec![row];
        let schema = infer_schema_from_rows(&rows).unwrap();
        assert_eq!(schema.field(0).data_type(), &DataType::Utf8);
    }

    #[test]
    fn test_infer_schema_from_rows_object_value_gives_utf8() {
        use serde_json::json;
        let mut row = HashMap::new();
        row.insert("meta".to_string(), json!({"key": "value"}));
        let rows = vec![row];
        let schema = infer_schema_from_rows(&rows).unwrap();
        assert_eq!(schema.field(0).data_type(), &DataType::Utf8);
    }
}
