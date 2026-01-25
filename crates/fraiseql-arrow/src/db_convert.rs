//! Database row to Arrow Value conversion.
//!
//! This module bridges database-agnostic row data (HashMap<String, serde_json::Value>)
//! to Arrow Values for RecordBatch construction.

use std::{collections::HashMap, sync::Arc};

use arrow::datatypes::{DataType, Schema};

use crate::{
    convert::Value,
    error::{ArrowFlightError, Result},
};

/// Convert database rows to Arrow Values.
///
/// Takes rows returned from `DatabaseAdapter::execute_raw_query()` and converts
/// them to the Arrow Value enum for RecordBatch construction.
///
/// # Arguments
///
/// * `rows` - Database rows as HashMap<column_name, json_value>
/// * `schema` - Arrow schema defining expected column types and order
///
/// # Returns
///
/// Vec of rows, where each row is Vec<Option<Value>> matching schema field order
///
/// # Errors
///
/// Returns error if:
/// - Missing required column in row data
/// - Type conversion fails
///
/// # Example
///
/// ```
/// use fraiseql_arrow::db_convert::convert_db_rows_to_arrow;
/// use arrow::datatypes::{DataType, Field, Schema};
/// use std::collections::HashMap;
/// use std::sync::Arc;
/// use serde_json::json;
///
/// let schema = Arc::new(Schema::new(vec![
///     Field::new("id", DataType::Int64, false),
///     Field::new("name", DataType::Utf8, true),
/// ]));
///
/// let mut row1 = HashMap::new();
/// row1.insert("id".to_string(), json!(1));
/// row1.insert("name".to_string(), json!("Alice"));
///
/// let mut row2 = HashMap::new();
/// row2.insert("id".to_string(), json!(2));
/// row2.insert("name".to_string(), json!(null));
///
/// let rows = vec![row1, row2];
/// let arrow_rows = convert_db_rows_to_arrow(&rows, &schema).unwrap();
///
/// assert_eq!(arrow_rows.len(), 2);
/// assert_eq!(arrow_rows[0].len(), 2); // 2 columns
/// ```
pub fn convert_db_rows_to_arrow(
    rows: &[HashMap<String, serde_json::Value>],
    schema: &Arc<Schema>,
) -> Result<Vec<Vec<Option<Value>>>> {
    let mut arrow_rows = Vec::with_capacity(rows.len());

    for row in rows {
        let mut arrow_row = Vec::with_capacity(schema.fields().len());

        for field in schema.fields() {
            let column_name = field.name();
            let value = row.get(column_name);

            let arrow_value = match value {
                Some(json_val) if !json_val.is_null() => {
                    Some(json_to_arrow_value(json_val, field.data_type())?)
                },
                _ => None, // NULL or missing column
            };

            arrow_row.push(arrow_value);
        }

        arrow_rows.push(arrow_row);
    }

    Ok(arrow_rows)
}

/// Convert JSON value to Arrow Value based on expected data type.
fn json_to_arrow_value(json_val: &serde_json::Value, data_type: &DataType) -> Result<Value> {
    use serde_json::Value as JsonValue;

    match data_type {
        DataType::Utf8 => match json_val {
            JsonValue::String(s) => Ok(Value::String(s.clone())),
            JsonValue::Number(n) => Ok(Value::String(n.to_string())),
            JsonValue::Bool(b) => Ok(Value::String(b.to_string())),
            _ => Ok(Value::String(json_val.to_string())),
        },
        DataType::Int32 => match json_val {
            JsonValue::Number(n) => n
                .as_i64()
                .and_then(|i| i32::try_from(i).ok())
                .map(|i| Value::Int(i64::from(i)))
                .ok_or_else(|| {
                    ArrowFlightError::InvalidTicket(format!("Cannot convert {n} to Int32"))
                }),
            _ => Err(ArrowFlightError::InvalidTicket(format!(
                "Expected number for Int32, got {json_val}"
            ))),
        },
        DataType::Int64 => match json_val {
            JsonValue::Number(n) => n.as_i64().map(Value::Int).ok_or_else(|| {
                ArrowFlightError::InvalidTicket(format!("Cannot convert {n} to Int64"))
            }),
            _ => Err(ArrowFlightError::InvalidTicket(format!(
                "Expected number for Int64, got {json_val}"
            ))),
        },
        DataType::Float64 => match json_val {
            JsonValue::Number(n) => n.as_f64().map(Value::Float).ok_or_else(|| {
                ArrowFlightError::InvalidTicket(format!("Cannot convert {n} to Float64"))
            }),
            _ => Err(ArrowFlightError::InvalidTicket(format!(
                "Expected number for Float64, got {json_val}"
            ))),
        },
        DataType::Boolean => match json_val {
            JsonValue::Bool(b) => Ok(Value::Bool(*b)),
            _ => Err(ArrowFlightError::InvalidTicket(format!("Expected boolean, got {json_val}"))),
        },
        DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, _) => {
            // Expect ISO 8601 string or Unix timestamp (microseconds)
            match json_val {
                JsonValue::String(s) => {
                    // Parse ISO 8601 timestamp
                    // TODO: Proper chrono parsing
                    // For now, return placeholder
                    let _ = s; // Use variable to avoid warning
                    Ok(Value::Timestamp(1_700_000_000_000_000)) // Placeholder
                },
                JsonValue::Number(n) => n.as_i64().map(Value::Timestamp).ok_or_else(|| {
                    ArrowFlightError::InvalidTicket(format!("Cannot convert {n} to Timestamp"))
                }),
                _ => Err(ArrowFlightError::InvalidTicket(format!(
                    "Expected string or number for Timestamp, got {json_val}"
                ))),
            }
        },
        DataType::Date32 => match json_val {
            JsonValue::String(_s) => {
                // TODO: Parse date string
                Ok(Value::Date(18_500)) // Placeholder
            },
            JsonValue::Number(n) => {
                n.as_i64().and_then(|i| i32::try_from(i).ok()).map(Value::Date).ok_or_else(|| {
                    ArrowFlightError::InvalidTicket(format!("Cannot convert {n} to Date32"))
                })
            },
            _ => Err(ArrowFlightError::InvalidTicket(format!(
                "Expected string or number for Date32, got {json_val}"
            ))),
        },
        _ => Err(ArrowFlightError::InvalidTicket(format!("Unsupported data type: {data_type:?}"))),
    }
}

#[cfg(test)]
mod tests {
    use arrow::datatypes::Field;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_convert_simple_rows() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, true),
        ]));

        let mut row1 = HashMap::new();
        row1.insert("id".to_string(), json!(1));
        row1.insert("name".to_string(), json!("Alice"));

        let mut row2 = HashMap::new();
        row2.insert("id".to_string(), json!(2));
        row2.insert("name".to_string(), json!("Bob"));

        let rows = vec![row1, row2];
        let arrow_rows = convert_db_rows_to_arrow(&rows, &schema).unwrap();

        assert_eq!(arrow_rows.len(), 2);
        assert_eq!(arrow_rows[0].len(), 2);

        // Check first row
        match &arrow_rows[0][0] {
            Some(Value::Int(i)) => assert_eq!(*i, 1),
            _ => panic!("Expected Int"),
        }
        match &arrow_rows[0][1] {
            Some(Value::String(s)) => assert_eq!(s, "Alice"),
            _ => panic!("Expected String"),
        }
    }

    #[test]
    fn test_null_handling() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, true),
        ]));

        let mut row = HashMap::new();
        row.insert("id".to_string(), json!(1));
        row.insert("name".to_string(), json!(null));

        let rows = vec![row];
        let arrow_rows = convert_db_rows_to_arrow(&rows, &schema).unwrap();

        assert_eq!(arrow_rows[0][1], None); // name is NULL
    }

    #[test]
    fn test_missing_column() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, true),
        ]));

        let mut row = HashMap::new();
        row.insert("id".to_string(), json!(1));
        // name column missing

        let rows = vec![row];
        let arrow_rows = convert_db_rows_to_arrow(&rows, &schema).unwrap();

        assert_eq!(arrow_rows[0][1], None); // missing treated as NULL
    }

    #[test]
    fn test_type_conversions() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("int_field", DataType::Int64, false),
            Field::new("float_field", DataType::Float64, false),
            Field::new("bool_field", DataType::Boolean, false),
            Field::new("string_field", DataType::Utf8, false),
        ]));

        let mut row = HashMap::new();
        row.insert("int_field".to_string(), json!(42));
        row.insert("float_field".to_string(), json!(2.5));
        row.insert("bool_field".to_string(), json!(true));
        row.insert("string_field".to_string(), json!("test"));

        let rows = vec![row];
        let arrow_rows = convert_db_rows_to_arrow(&rows, &schema).unwrap();

        match &arrow_rows[0][0] {
            Some(Value::Int(i)) => assert_eq!(*i, 42),
            _ => panic!("Expected Int"),
        }
        match &arrow_rows[0][1] {
            Some(Value::Float(f)) => assert!((f - 2.5).abs() < 0.001),
            _ => panic!("Expected Float"),
        }
        match &arrow_rows[0][2] {
            Some(Value::Bool(b)) => assert!(b),
            _ => panic!("Expected Bool"),
        }
        match &arrow_rows[0][3] {
            Some(Value::String(s)) => assert_eq!(s, "test"),
            _ => panic!("Expected String"),
        }
    }
}
