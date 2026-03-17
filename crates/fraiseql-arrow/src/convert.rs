//! SQL Row → Arrow `RecordBatch` conversion.
//!
//! This module provides the core conversion logic for transforming database rows
//! into Apache Arrow `RecordBatches` for high-performance data transfer.

use std::sync::Arc;

use arrow::{
    array::{
        ArrayBuilder, BooleanBuilder, Date32Builder, Float64Builder, Int32Builder, Int64Builder,
        RecordBatch, StringBuilder, TimestampNanosecondBuilder,
    },
    datatypes::{DataType, Schema, TimeUnit},
    error::ArrowError,
};

/// Configuration for Arrow batch conversion.
#[derive(Debug, Clone, Copy)]
pub struct ConvertConfig {
    /// Number of rows per `RecordBatch` (default: 10,000)
    pub batch_size: usize,

    /// Maximum total rows to convert (default: unlimited)
    pub max_rows: Option<usize>,
}

impl Default for ConvertConfig {
    fn default() -> Self {
        Self {
            batch_size: 10_000,
            max_rows:   None,
        }
    }
}

/// Placeholder for SQL value types.
///
/// In production, this will be replaced with actual database driver types.
/// For, this provides the interface for converting values.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Value {
    /// UTF-8 string value
    String(String),
    /// 64-bit integer (will be converted to i32 for Int32 fields)
    Int(i64),
    /// 64-bit floating point
    Float(f64),
    /// Boolean value
    Bool(bool),
    /// Timestamp as nanoseconds since Unix epoch
    Timestamp(i64),
    /// Date as days since Unix epoch
    Date(i32),
}

/// Convert SQL rows to Arrow `RecordBatches`.
///
/// This is the core conversion logic that powers GraphQL → Arrow streaming.
///
/// # Example
///
/// ```
/// use fraiseql_arrow::convert::{RowToArrowConverter, ConvertConfig, Value};
/// use arrow::datatypes::{DataType, Field, Schema};
/// use std::sync::Arc;
///
/// let schema = Arc::new(Schema::new(vec![
///     Field::new("id", DataType::Int32, false),
///     Field::new("name", DataType::Utf8, true),
/// ]));
///
/// let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
///
/// let rows = vec![
///     vec![Some(Value::Int(1)), Some(Value::String("Alice".to_string()))],
///     vec![Some(Value::Int(2)), None],
/// ];
///
/// let batch = converter.convert_batch(rows).unwrap();
/// assert_eq!(batch.num_rows(), 2);
/// ```
pub struct RowToArrowConverter {
    schema: Arc<Schema>,
    config: ConvertConfig,
}

impl RowToArrowConverter {
    /// Create a new row-to-Arrow converter with the given schema and configuration.
    #[must_use]
    pub const fn new(schema: Arc<Schema>, config: ConvertConfig) -> Self {
        Self { schema, config }
    }

    /// Convert a batch of rows into a single `RecordBatch`.
    ///
    /// Rows are provided as `Vec<Vec<Option<Value>>>` where:
    /// - Outer Vec: rows
    /// - Inner Vec: columns (matching schema field order)
    /// - `Option<Value>`: nullable column values
    ///
    /// # Errors
    ///
    /// Returns `ArrowError` if:
    /// - Row column count doesn't match schema
    /// - Value type doesn't match expected Arrow type
    /// - `RecordBatch` construction fails
    pub fn convert_batch(&self, rows: Vec<Vec<Option<Value>>>) -> Result<RecordBatch, ArrowError> {
        if rows.is_empty() {
            return Ok(RecordBatch::new_empty(self.schema.clone()));
        }

        let num_columns = self.schema.fields().len();
        let mut column_builders = self.create_builders(num_columns)?;

        // Populate builders row by row
        for row in rows {
            if row.len() != num_columns {
                return Err(ArrowError::InvalidArgumentError(format!(
                    "Row has {} columns, expected {}",
                    row.len(),
                    num_columns
                )));
            }

            for (col_idx, value) in row.iter().enumerate() {
                let field = self.schema.field(col_idx);
                self.append_value(&mut column_builders[col_idx], value, field.data_type())?;
            }
        }

        // Finish builders and create RecordBatch
        let columns: Vec<_> =
            column_builders.into_iter().map(|mut builder| builder.finish()).collect();

        RecordBatch::try_new(self.schema.clone(), columns)
    }

    /// Create array builders for each column in the schema.
    ///
    /// # Errors
    ///
    /// Returns `ArrowError::InvalidArgumentError` if any column has an unsupported data type.
    fn create_builders(
        &self,
        num_columns: usize,
    ) -> Result<Vec<Box<dyn ArrayBuilder>>, ArrowError> {
        (0..num_columns)
            .map(|i| {
                let field = self.schema.field(i);
                create_builder_for_type(field.data_type(), self.config.batch_size)
            })
            .collect()
    }

    /// Append a value to the appropriate builder based on data type.
    fn append_value(
        &self,
        builder: &mut Box<dyn ArrayBuilder>,
        value: &Option<Value>,
        data_type: &DataType,
    ) -> Result<(), ArrowError> {
        match data_type {
            DataType::Utf8 => {
                let b = downcast_builder::<StringBuilder>(builder, "StringBuilder", "Utf8")?;
                match value {
                    Some(Value::String(s)) => b.append_value(s),
                    None => b.append_null(),
                    _ => {
                        return Err(ArrowError::InvalidArgumentError(
                            "Expected string value".into(),
                        ));
                    },
                }
            },
            DataType::Int32 => {
                let b = downcast_builder::<Int32Builder>(builder, "Int32Builder", "Int32")?;
                match value {
                    Some(Value::Int(i)) => b
                        .append_value(i32::try_from(*i).map_err(|_| {
                            ArrowError::InvalidArgumentError("Int overflow".into())
                        })?),
                    None => b.append_null(),
                    _ => return Err(ArrowError::InvalidArgumentError("Expected int value".into())),
                }
            },
            DataType::Int64 => {
                let b = downcast_builder::<Int64Builder>(builder, "Int64Builder", "Int64")?;
                match value {
                    Some(Value::Int(i)) => b.append_value(*i),
                    None => b.append_null(),
                    _ => {
                        return Err(ArrowError::InvalidArgumentError(
                            "Expected int64 value".into(),
                        ));
                    },
                }
            },
            DataType::Float64 => {
                let b = downcast_builder::<Float64Builder>(builder, "Float64Builder", "Float64")?;
                match value {
                    Some(Value::Float(f)) => b.append_value(*f),
                    None => b.append_null(),
                    _ => {
                        return Err(ArrowError::InvalidArgumentError(
                            "Expected float value".into(),
                        ));
                    },
                }
            },
            DataType::Boolean => {
                let b = downcast_builder::<BooleanBuilder>(builder, "BooleanBuilder", "Boolean")?;
                match value {
                    Some(Value::Bool(b_val)) => b.append_value(*b_val),
                    None => b.append_null(),
                    _ => {
                        return Err(ArrowError::InvalidArgumentError("Expected bool value".into()));
                    },
                }
            },
            DataType::Timestamp(TimeUnit::Nanosecond, _) => {
                let b = downcast_builder::<TimestampNanosecondBuilder>(
                    builder,
                    "TimestampNanosecondBuilder",
                    "Timestamp(Nanosecond)",
                )?;
                match value {
                    Some(Value::Timestamp(nanos)) => b.append_value(*nanos),
                    None => b.append_null(),
                    _ => {
                        return Err(ArrowError::InvalidArgumentError(
                            "Expected timestamp value".into(),
                        ));
                    },
                }
            },
            DataType::Date32 => {
                let b = downcast_builder::<Date32Builder>(builder, "Date32Builder", "Date32")?;
                match value {
                    Some(Value::Date(days)) => b.append_value(*days),
                    None => b.append_null(),
                    _ => {
                        return Err(ArrowError::InvalidArgumentError("Expected date value".into()));
                    },
                }
            },
            _ => {
                return Err(ArrowError::InvalidArgumentError(format!(
                    "Unsupported data type: {data_type:?}"
                )));
            },
        }
        Ok(())
    }
}

/// Downcast a boxed `ArrayBuilder` to a concrete type, returning `ArrowError` on mismatch.
fn downcast_builder<'a, T: ArrayBuilder + 'static>(
    builder: &'a mut Box<dyn ArrayBuilder>,
    expected_type: &str,
    field_type: &str,
) -> Result<&'a mut T, ArrowError> {
    builder.as_any_mut().downcast_mut::<T>().ok_or_else(|| {
        ArrowError::InvalidArgumentError(format!("Expected {expected_type} for {field_type} field"))
    })
}

/// Create an array builder for a given Arrow data type.
///
/// # Errors
///
/// Returns `ArrowError::InvalidArgumentError` if the data type is not supported.
fn create_builder_for_type(
    data_type: &DataType,
    capacity: usize,
) -> Result<Box<dyn ArrayBuilder>, ArrowError> {
    match data_type {
        DataType::Utf8 => {
            // Estimate 50 bytes per string on average
            Ok(Box::new(StringBuilder::with_capacity(capacity, capacity * 50)))
        },
        DataType::Int32 => Ok(Box::new(Int32Builder::with_capacity(capacity))),
        DataType::Int64 => Ok(Box::new(Int64Builder::with_capacity(capacity))),
        DataType::Float64 => Ok(Box::new(Float64Builder::with_capacity(capacity))),
        DataType::Boolean => Ok(Box::new(BooleanBuilder::with_capacity(capacity))),
        DataType::Timestamp(TimeUnit::Nanosecond, tz) => Ok(Box::new(
            TimestampNanosecondBuilder::with_capacity(capacity).with_timezone_opt(tz.clone()),
        )),
        DataType::Date32 => Ok(Box::new(Date32Builder::with_capacity(capacity))),
        _ => Err(ArrowError::InvalidArgumentError(format!(
            "Unsupported data type: {data_type:?}"
        ))),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code extensively uses unwrap for test fixture setup
#[allow(clippy::cast_possible_wrap)] // Reason: test data uses small integers that cannot wrap
mod tests {
    use arrow::{array::Array, datatypes::Field};

    use super::*;

    #[test]
    fn test_convert_simple_batch() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int32, false),
            Field::new("name", DataType::Utf8, true),
        ]));

        let converter = RowToArrowConverter::new(schema.clone(), ConvertConfig::default());

        let rows = vec![
            vec![
                Some(Value::Int(1)),
                Some(Value::String("Alice".to_string())),
            ],
            vec![Some(Value::Int(2)), Some(Value::String("Bob".to_string()))],
            vec![Some(Value::Int(3)), None],
        ];

        let batch = converter.convert_batch(rows).unwrap();

        assert_eq!(batch.num_rows(), 3);
        assert_eq!(batch.num_columns(), 2);
        assert_eq!(batch.schema(), schema);
    }

    #[test]
    fn test_null_handling() {
        let schema =
            Arc::new(Schema::new(vec![Field::new("nullable_field", DataType::Utf8, true)]));

        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());

        let rows = vec![
            vec![Some(Value::String("present".to_string()))],
            vec![None],
            vec![Some(Value::String("also present".to_string()))],
        ];

        let batch = converter.convert_batch(rows).unwrap();
        assert_eq!(batch.num_rows(), 3);
        assert_eq!(batch.num_columns(), 1);
    }

    #[test]
    fn test_empty_batch() {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int32, false)]));

        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let rows: Vec<Vec<Option<Value>>> = vec![];

        let batch = converter.convert_batch(rows).unwrap();
        assert_eq!(batch.num_rows(), 0);
        assert_eq!(batch.num_columns(), 1);
    }

    #[test]
    fn test_multiple_types() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("str_field", DataType::Utf8, false),
            Field::new("int_field", DataType::Int32, false),
            Field::new("float_field", DataType::Float64, false),
            Field::new("bool_field", DataType::Boolean, false),
        ]));

        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());

        let rows = vec![vec![
            Some(Value::String("test".to_string())),
            Some(Value::Int(42)),
            Some(Value::Float(42.5)),
            Some(Value::Bool(true)),
        ]];

        let batch = converter.convert_batch(rows).unwrap();
        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 4);
    }

    #[test]
    fn test_timestamp_conversion() {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "created_at",
            DataType::Timestamp(TimeUnit::Nanosecond, Some(Arc::from("UTC"))),
            false,
        )]));

        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());

        let nanos = 1_700_000_000_000_000_000_i64; // Example timestamp
        let rows = vec![vec![Some(Value::Timestamp(nanos))]];

        let batch = converter.convert_batch(rows).unwrap();
        assert_eq!(batch.num_rows(), 1);
    }

    #[test]
    fn test_date_conversion() {
        let schema = Arc::new(Schema::new(vec![Field::new("birth_date", DataType::Date32, true)]));

        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());

        let rows = vec![vec![Some(Value::Date(18_500))], vec![None]]; // ~50 years since epoch

        let batch = converter.convert_batch(rows).unwrap();
        assert_eq!(batch.num_rows(), 2);
    }

    #[test]
    fn test_mismatched_column_count() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("col1", DataType::Int32, false),
            Field::new("col2", DataType::Utf8, false),
        ]));

        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());

        // Row has only 1 column, but schema expects 2
        let rows = vec![vec![Some(Value::Int(1))]];

        let result = converter.convert_batch(rows);
        assert!(
            matches!(result, Err(ArrowError::InvalidArgumentError(_))),
            "expected InvalidArgumentError for column count mismatch, got: {result:?}"
        );
        assert!(result.unwrap_err().to_string().contains("expected 2"));
    }

    #[test]
    fn test_wrong_value_type() {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int32, false)]));

        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());

        // Providing string instead of int
        let rows = vec![vec![Some(Value::String("not an int".to_string()))]];

        let result = converter.convert_batch(rows);
        assert!(
            matches!(result, Err(ArrowError::InvalidArgumentError(_))),
            "expected InvalidArgumentError for wrong value type, got: {result:?}"
        );
        assert!(result.unwrap_err().to_string().contains("Expected int"));
    }

    #[test]
    fn test_config_defaults() {
        let config = ConvertConfig::default();
        assert_eq!(config.batch_size, 10_000);
        assert_eq!(config.max_rows, None);
    }

    #[test]
    fn test_custom_config() {
        let config = ConvertConfig {
            batch_size: 5_000,
            max_rows:   Some(100_000),
        };

        assert_eq!(config.batch_size, 5_000);
        assert_eq!(config.max_rows, Some(100_000));
    }

    #[test]
    fn test_downcast_builder_rejects_mismatched_type() {
        let mut builder: Box<dyn ArrayBuilder> = Box::new(StringBuilder::new());
        let result = downcast_builder::<Int32Builder>(&mut builder, "Int32Builder", "Int32");
        assert!(
            matches!(result, Err(ArrowError::InvalidArgumentError(_))),
            "expected InvalidArgumentError for mismatched builder type, got: {result:?}"
        );
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Expected Int32Builder"));
    }

    #[test]
    fn test_downcast_builder_accepts_correct_type() {
        let mut builder: Box<dyn ArrayBuilder> = Box::new(StringBuilder::new());
        let result = downcast_builder::<StringBuilder>(&mut builder, "StringBuilder", "Utf8");
        result.unwrap_or_else(|e| panic!("downcast to correct type should succeed: {e}"));
    }

    // --- Null handling for each Arrow type ---

    #[test]
    fn test_null_in_int64_column_preserved() {
        let schema = Arc::new(Schema::new(vec![Field::new("val", DataType::Int64, true)]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let rows = vec![vec![None::<Value>]];
        let batch = converter.convert_batch(rows).unwrap();
        assert_eq!(batch.num_rows(), 1);
        use arrow::array::Int64Array;
        let col = batch.column(0).as_any().downcast_ref::<Int64Array>().unwrap();
        assert!(col.is_null(0));
    }

    #[test]
    fn test_null_in_utf8_column_preserved() {
        let schema = Arc::new(Schema::new(vec![Field::new("val", DataType::Utf8, true)]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let rows = vec![vec![None::<Value>]];
        let batch = converter.convert_batch(rows).unwrap();
        use arrow::array::StringArray;
        let col = batch.column(0).as_any().downcast_ref::<StringArray>().unwrap();
        assert!(col.is_null(0));
    }

    #[test]
    fn test_null_in_float64_column_preserved() {
        let schema = Arc::new(Schema::new(vec![Field::new("val", DataType::Float64, true)]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let rows = vec![vec![None::<Value>]];
        let batch = converter.convert_batch(rows).unwrap();
        use arrow::array::Float64Array;
        let col = batch.column(0).as_any().downcast_ref::<Float64Array>().unwrap();
        assert!(col.is_null(0));
    }

    #[test]
    fn test_null_in_boolean_column_preserved() {
        let schema = Arc::new(Schema::new(vec![Field::new("val", DataType::Boolean, true)]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let rows = vec![vec![None::<Value>]];
        let batch = converter.convert_batch(rows).unwrap();
        use arrow::array::BooleanArray;
        let col = batch.column(0).as_any().downcast_ref::<BooleanArray>().unwrap();
        assert!(col.is_null(0));
    }

    #[test]
    fn test_null_in_date32_column_preserved() {
        let schema = Arc::new(Schema::new(vec![Field::new("val", DataType::Date32, true)]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let rows = vec![vec![None::<Value>]];
        let batch = converter.convert_batch(rows).unwrap();
        use arrow::array::Date32Array;
        let col = batch.column(0).as_any().downcast_ref::<Date32Array>().unwrap();
        assert!(col.is_null(0));
    }

    #[test]
    fn test_null_in_timestamp_nanosecond_column_preserved() {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "val",
            DataType::Timestamp(TimeUnit::Nanosecond, None),
            true,
        )]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let rows = vec![vec![None::<Value>]];
        let batch = converter.convert_batch(rows).unwrap();
        use arrow::array::TimestampNanosecondArray;
        let col = batch.column(0).as_any().downcast_ref::<TimestampNanosecondArray>().unwrap();
        assert!(col.is_null(0));
    }

    // --- Type coercion / error tests ---

    #[test]
    fn test_string_in_int64_column_is_error() {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let rows = vec![vec![Some(Value::String("not-an-int".to_string()))]];
        let result = converter.convert_batch(rows);
        assert!(
            matches!(result, Err(ArrowError::InvalidArgumentError(_))),
            "expected InvalidArgumentError for string in Int64 column, got: {result:?}"
        );
        assert!(result.unwrap_err().to_string().contains("Expected int64 value"));
    }

    #[test]
    fn test_float_in_boolean_column_is_error() {
        let schema = Arc::new(Schema::new(vec![Field::new("flag", DataType::Boolean, false)]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let rows = vec![vec![Some(Value::Float(1.0))]];
        let result = converter.convert_batch(rows);
        assert!(
            matches!(result, Err(ArrowError::InvalidArgumentError(_))),
            "expected InvalidArgumentError for float in Boolean column, got: {result:?}"
        );
        assert!(result.unwrap_err().to_string().contains("Expected bool value"));
    }

    #[test]
    fn test_bool_in_utf8_column_is_error() {
        let schema = Arc::new(Schema::new(vec![Field::new("name", DataType::Utf8, false)]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let rows = vec![vec![Some(Value::Bool(true))]];
        let result = converter.convert_batch(rows);
        assert!(
            matches!(result, Err(ArrowError::InvalidArgumentError(_))),
            "expected InvalidArgumentError for bool in Utf8 column, got: {result:?}"
        );
        assert!(result.unwrap_err().to_string().contains("Expected string value"));
    }

    #[test]
    fn test_float_in_int32_column_is_error() {
        let schema = Arc::new(Schema::new(vec![Field::new("count", DataType::Int32, false)]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let rows = vec![vec![Some(Value::Float(std::f64::consts::PI))]];
        let result = converter.convert_batch(rows);
        assert!(
            matches!(result, Err(ArrowError::InvalidArgumentError(_))),
            "expected InvalidArgumentError for float in Int32 column, got: {result:?}"
        );
        assert!(result.unwrap_err().to_string().contains("Expected int"));
    }

    #[test]
    fn test_int_overflow_for_int32_is_error() {
        let schema = Arc::new(Schema::new(vec![Field::new("val", DataType::Int32, false)]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        // i64::MAX cannot be represented as i32
        let rows = vec![vec![Some(Value::Int(i64::MAX))]];
        let result = converter.convert_batch(rows);
        assert!(
            matches!(result, Err(ArrowError::InvalidArgumentError(_))),
            "expected InvalidArgumentError for Int32 overflow, got: {result:?}"
        );
        assert!(result.unwrap_err().to_string().contains("Int overflow"));
    }

    #[test]
    fn test_string_in_timestamp_column_is_error() {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "ts",
            DataType::Timestamp(TimeUnit::Nanosecond, None),
            false,
        )]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let rows = vec![vec![Some(Value::String("2026-01-01".to_string()))]];
        let result = converter.convert_batch(rows);
        assert!(
            matches!(result, Err(ArrowError::InvalidArgumentError(_))),
            "expected InvalidArgumentError for string in Timestamp column, got: {result:?}"
        );
        assert!(result.unwrap_err().to_string().contains("Expected timestamp value"));
    }

    #[test]
    fn test_string_in_date32_column_is_error() {
        let schema = Arc::new(Schema::new(vec![Field::new("d", DataType::Date32, false)]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let rows = vec![vec![Some(Value::String("2026-01-01".to_string()))]];
        let result = converter.convert_batch(rows);
        assert!(
            matches!(result, Err(ArrowError::InvalidArgumentError(_))),
            "expected InvalidArgumentError for string in Date32 column, got: {result:?}"
        );
        assert!(result.unwrap_err().to_string().contains("Expected date value"));
    }

    #[test]
    fn test_unsupported_data_type_is_error() {
        // DataType::LargeUtf8 is not supported
        let schema = Arc::new(Schema::new(vec![Field::new("val", DataType::LargeUtf8, true)]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        // create_builders itself should fail
        let rows = vec![vec![None::<Value>]];
        let result = converter.convert_batch(rows);
        assert!(
            matches!(result, Err(ArrowError::InvalidArgumentError(_))),
            "expected InvalidArgumentError for unsupported data type, got: {result:?}"
        );
        assert!(result.unwrap_err().to_string().contains("Unsupported data type"));
    }

    // --- Batch boundary conditions ---

    #[test]
    fn test_zero_rows_returns_empty_batch_with_correct_schema() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, true),
            Field::new("score", DataType::Float64, true),
        ]));
        let converter = RowToArrowConverter::new(schema.clone(), ConvertConfig::default());
        let rows: Vec<Vec<Option<Value>>> = vec![];
        let batch = converter.convert_batch(rows).unwrap();
        assert_eq!(batch.num_rows(), 0);
        assert_eq!(batch.num_columns(), 3);
        assert_eq!(batch.schema(), schema);
    }

    #[test]
    fn test_exactly_one_row_all_null_fields() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("a", DataType::Int64, true),
            Field::new("b", DataType::Utf8, true),
            Field::new("c", DataType::Float64, true),
            Field::new("d", DataType::Boolean, true),
        ]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let rows = vec![vec![None, None, None, None]];
        let batch = converter.convert_batch(rows).unwrap();
        assert_eq!(batch.num_rows(), 1);
        for col_idx in 0..4 {
            assert!(batch.column(col_idx).is_null(0), "column {col_idx} should be null");
        }
    }

    #[test]
    fn test_exactly_batch_size_rows() {
        let batch_size = 100usize;
        let schema = Arc::new(Schema::new(vec![Field::new("n", DataType::Int32, false)]));
        let config = ConvertConfig {
            batch_size,
            max_rows: None,
        };
        let converter = RowToArrowConverter::new(schema, config);
        let rows: Vec<Vec<Option<Value>>> =
            (0..batch_size as i64).map(|i| vec![Some(Value::Int(i))]).collect();
        let batch = converter.convert_batch(rows).unwrap();
        assert_eq!(batch.num_rows(), batch_size);
    }

    #[test]
    fn test_single_row_only_string_fields() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("first", DataType::Utf8, false),
            Field::new("last", DataType::Utf8, false),
        ]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let rows = vec![vec![
            Some(Value::String("John".to_string())),
            Some(Value::String("Doe".to_string())),
        ]];
        let batch = converter.convert_batch(rows).unwrap();
        assert_eq!(batch.num_rows(), 1);
        use arrow::array::StringArray;
        let first_col = batch.column(0).as_any().downcast_ref::<StringArray>().unwrap();
        assert_eq!(first_col.value(0), "John");
        let last_col = batch.column(1).as_any().downcast_ref::<StringArray>().unwrap();
        assert_eq!(last_col.value(0), "Doe");
    }

    #[test]
    fn test_row_with_too_many_columns_is_error() {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        // Row has 2 columns but schema has 1
        let rows = vec![vec![Some(Value::Int(1)), Some(Value::Int(2))]];
        let result = converter.convert_batch(rows);
        assert!(
            matches!(result, Err(ArrowError::InvalidArgumentError(_))),
            "expected InvalidArgumentError for too many columns, got: {result:?}"
        );
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("expected 1"));
    }

    // --- Unicode / special values ---

    #[test]
    fn test_unicode_string_in_utf8_column() {
        let schema = Arc::new(Schema::new(vec![Field::new("text", DataType::Utf8, false)]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let emoji = "Hello 🌍 こんにちは مرحبا";
        let rows = vec![vec![Some(Value::String(emoji.to_string()))]];
        let batch = converter.convert_batch(rows).unwrap();
        use arrow::array::StringArray;
        let col = batch.column(0).as_any().downcast_ref::<StringArray>().unwrap();
        assert_eq!(col.value(0), emoji);
    }

    #[test]
    fn test_ieee754_nan_in_float64_column() {
        let schema = Arc::new(Schema::new(vec![Field::new("v", DataType::Float64, true)]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let rows = vec![vec![Some(Value::Float(f64::NAN))]];
        let batch = converter.convert_batch(rows).unwrap();
        use arrow::array::Float64Array;
        let col = batch.column(0).as_any().downcast_ref::<Float64Array>().unwrap();
        // NaN is stored; verify value is NaN
        assert!(col.value(0).is_nan());
    }

    #[test]
    fn test_ieee754_positive_infinity_in_float64_column() {
        let schema = Arc::new(Schema::new(vec![Field::new("v", DataType::Float64, true)]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let rows = vec![vec![Some(Value::Float(f64::INFINITY))]];
        let batch = converter.convert_batch(rows).unwrap();
        use arrow::array::Float64Array;
        let col = batch.column(0).as_any().downcast_ref::<Float64Array>().unwrap();
        assert!(col.value(0).is_infinite() && col.value(0) > 0.0);
    }

    #[test]
    fn test_ieee754_negative_infinity_in_float64_column() {
        let schema = Arc::new(Schema::new(vec![Field::new("v", DataType::Float64, true)]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let rows = vec![vec![Some(Value::Float(f64::NEG_INFINITY))]];
        let batch = converter.convert_batch(rows).unwrap();
        use arrow::array::Float64Array;
        let col = batch.column(0).as_any().downcast_ref::<Float64Array>().unwrap();
        assert!(col.value(0).is_infinite() && col.value(0) < 0.0);
    }

    #[test]
    fn test_very_long_string_in_utf8_column() {
        let schema = Arc::new(Schema::new(vec![Field::new("blob", DataType::Utf8, false)]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let long_str = "x".repeat(100_000); // 100 KB
        let rows = vec![vec![Some(Value::String(long_str.clone()))]];
        let batch = converter.convert_batch(rows).unwrap();
        use arrow::array::StringArray;
        let col = batch.column(0).as_any().downcast_ref::<StringArray>().unwrap();
        assert_eq!(col.value(0).len(), 100_000);
        assert_eq!(col.value(0), long_str.as_str());
    }

    // --- Schema enforcement ---

    #[test]
    fn test_multiple_rows_mixed_nulls() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, true),
            Field::new("score", DataType::Float64, true),
        ]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let rows = vec![
            vec![
                Some(Value::Int(1)),
                Some(Value::String("Alice".to_string())),
                Some(Value::Float(9.5)),
            ],
            vec![Some(Value::Int(2)), None, None],
            vec![
                Some(Value::Int(3)),
                Some(Value::String("Carol".to_string())),
                None,
            ],
        ];
        let batch = converter.convert_batch(rows).unwrap();
        assert_eq!(batch.num_rows(), 3);
        use arrow::array::{Float64Array, Int64Array, StringArray};
        let ids = batch.column(0).as_any().downcast_ref::<Int64Array>().unwrap();
        assert_eq!(ids.value(0), 1);
        assert_eq!(ids.value(1), 2);
        assert_eq!(ids.value(2), 3);
        let names = batch.column(1).as_any().downcast_ref::<StringArray>().unwrap();
        assert!(!names.is_null(0));
        assert!(names.is_null(1));
        assert!(!names.is_null(2));
        let scores = batch.column(2).as_any().downcast_ref::<Float64Array>().unwrap();
        assert!(!scores.is_null(0));
        assert!(scores.is_null(1));
        assert!(scores.is_null(2));
    }

    #[test]
    fn test_int64_values_are_preserved() {
        let schema = Arc::new(Schema::new(vec![Field::new("n", DataType::Int64, false)]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let test_values = [0i64, 1, -1, i64::MAX, i64::MIN, 42, -999_999];
        let rows: Vec<Vec<Option<Value>>> =
            test_values.iter().map(|&v| vec![Some(Value::Int(v))]).collect();
        let batch = converter.convert_batch(rows).unwrap();
        use arrow::array::Int64Array;
        let col = batch.column(0).as_any().downcast_ref::<Int64Array>().unwrap();
        for (i, &expected) in test_values.iter().enumerate() {
            assert_eq!(col.value(i), expected, "mismatch at index {i}");
        }
    }

    #[test]
    fn test_int32_boundary_values_are_preserved() {
        let schema = Arc::new(Schema::new(vec![Field::new("n", DataType::Int32, false)]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let test_values = [0i32, 1, -1, i32::MAX, i32::MIN];
        let rows: Vec<Vec<Option<Value>>> =
            test_values.iter().map(|&v| vec![Some(Value::Int(i64::from(v)))]).collect();
        let batch = converter.convert_batch(rows).unwrap();
        use arrow::array::Int32Array;
        let col = batch.column(0).as_any().downcast_ref::<Int32Array>().unwrap();
        for (i, &expected) in test_values.iter().enumerate() {
            assert_eq!(col.value(i), expected, "mismatch at index {i}");
        }
    }

    #[test]
    fn test_boolean_values_true_and_false() {
        let schema = Arc::new(Schema::new(vec![Field::new("flag", DataType::Boolean, false)]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let rows = vec![
            vec![Some(Value::Bool(true))],
            vec![Some(Value::Bool(false))],
            vec![Some(Value::Bool(true))],
        ];
        let batch = converter.convert_batch(rows).unwrap();
        use arrow::array::BooleanArray;
        let col = batch.column(0).as_any().downcast_ref::<BooleanArray>().unwrap();
        assert!(col.value(0));
        assert!(!col.value(1));
        assert!(col.value(2));
    }

    #[test]
    fn test_timestamp_nanosecond_values_are_preserved() {
        let ts_values = [
            0i64,
            1_700_000_000_000_000_000,
            -1_000_000_000,
            i64::MAX / 2,
        ];
        let schema = Arc::new(Schema::new(vec![Field::new(
            "ts",
            DataType::Timestamp(TimeUnit::Nanosecond, None),
            false,
        )]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let rows: Vec<Vec<Option<Value>>> =
            ts_values.iter().map(|&v| vec![Some(Value::Timestamp(v))]).collect();
        let batch = converter.convert_batch(rows).unwrap();
        use arrow::array::TimestampNanosecondArray;
        let col = batch.column(0).as_any().downcast_ref::<TimestampNanosecondArray>().unwrap();
        for (i, &expected) in ts_values.iter().enumerate() {
            assert_eq!(col.value(i), expected, "mismatch at index {i}");
        }
    }

    #[test]
    fn test_date32_values_are_preserved() {
        let date_values = [0i32, 1, -1, 18_500, 20_000, i32::MAX / 2];
        let schema = Arc::new(Schema::new(vec![Field::new("d", DataType::Date32, false)]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let rows: Vec<Vec<Option<Value>>> =
            date_values.iter().map(|&v| vec![Some(Value::Date(v))]).collect();
        let batch = converter.convert_batch(rows).unwrap();
        use arrow::array::Date32Array;
        let col = batch.column(0).as_any().downcast_ref::<Date32Array>().unwrap();
        for (i, &expected) in date_values.iter().enumerate() {
            assert_eq!(col.value(i), expected, "mismatch at index {i}");
        }
    }

    #[test]
    fn test_create_builder_for_unsupported_type_returns_error() {
        // LargeUtf8 is not in our supported set
        let result = create_builder_for_type(&DataType::LargeUtf8, 100);
        match result {
            Ok(_) => panic!("Expected error for unsupported type"),
            Err(e) => {
                assert!(e.to_string().contains("Unsupported data type"));
            },
        }
    }

    #[test]
    fn test_timestamp_with_utc_timezone_preserves_values() {
        let ts = 1_700_000_000_123_456_789i64;
        let schema = Arc::new(Schema::new(vec![Field::new(
            "ts",
            DataType::Timestamp(TimeUnit::Nanosecond, Some(Arc::from("UTC"))),
            false,
        )]));
        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());
        let rows = vec![vec![Some(Value::Timestamp(ts))]];
        let batch = converter.convert_batch(rows).unwrap();
        use arrow::array::TimestampNanosecondArray;
        let col = batch.column(0).as_any().downcast_ref::<TimestampNanosecondArray>().unwrap();
        assert_eq!(col.value(0), ts);
    }

    #[test]
    fn test_schema_field_count_matches_column_count() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("a", DataType::Int64, true),
            Field::new("b", DataType::Utf8, true),
            Field::new("c", DataType::Boolean, true),
            Field::new("d", DataType::Float64, true),
            Field::new("e", DataType::Date32, true),
        ]));
        let converter = RowToArrowConverter::new(schema.clone(), ConvertConfig::default());
        let rows = vec![vec![
            Some(Value::Int(1)),
            Some(Value::String("x".to_string())),
            Some(Value::Bool(false)),
            Some(Value::Float(0.0)),
            Some(Value::Date(100)),
        ]];
        let batch = converter.convert_batch(rows).unwrap();
        assert_eq!(batch.num_columns(), schema.fields().len());
    }

    #[test]
    fn test_downcast_builder_error_message_includes_both_type_names() {
        let mut builder: Box<dyn ArrayBuilder> = Box::new(BooleanBuilder::new());
        let result = downcast_builder::<Float64Builder>(&mut builder, "Float64Builder", "Float64");
        assert!(
            matches!(result, Err(ArrowError::InvalidArgumentError(_))),
            "expected InvalidArgumentError for mismatched builder type, got: {result:?}"
        );
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Float64Builder"));
        assert!(msg.contains("Float64"));
    }
}
