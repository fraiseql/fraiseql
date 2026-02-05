//! SQL Row → Arrow RecordBatch conversion.
//!
//! This module provides the core conversion logic for transforming database rows
//! into Apache Arrow RecordBatches for high-performance data transfer.

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
    /// Number of rows per RecordBatch (default: 10,000)
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

/// Convert SQL rows to Arrow RecordBatches.
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
    pub fn new(schema: Arc<Schema>, config: ConvertConfig) -> Self {
        Self { schema, config }
    }

    /// Convert a batch of rows into a single RecordBatch.
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
    /// - RecordBatch construction fails
    pub fn convert_batch(&self, rows: Vec<Vec<Option<Value>>>) -> Result<RecordBatch, ArrowError> {
        if rows.is_empty() {
            return Ok(RecordBatch::new_empty(self.schema.clone()));
        }

        let num_columns = self.schema.fields().len();
        let mut column_builders = self.create_builders(num_columns);

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
    fn create_builders(&self, num_columns: usize) -> Vec<Box<dyn ArrayBuilder>> {
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
                let builder = builder
                    .as_any_mut()
                    .downcast_mut::<StringBuilder>()
                    .expect("Builder type mismatch");
                match value {
                    Some(Value::String(s)) => builder.append_value(s),
                    None => builder.append_null(),
                    _ => {
                        return Err(ArrowError::InvalidArgumentError(
                            "Expected string value".into(),
                        ));
                    },
                }
            },
            DataType::Int32 => {
                let builder = builder
                    .as_any_mut()
                    .downcast_mut::<Int32Builder>()
                    .expect("Builder type mismatch");
                match value {
                    Some(Value::Int(i)) => builder
                        .append_value(i32::try_from(*i).map_err(|_| {
                            ArrowError::InvalidArgumentError("Int overflow".into())
                        })?),
                    None => builder.append_null(),
                    _ => return Err(ArrowError::InvalidArgumentError("Expected int value".into())),
                }
            },
            DataType::Int64 => {
                let builder = builder
                    .as_any_mut()
                    .downcast_mut::<Int64Builder>()
                    .expect("Builder type mismatch");
                match value {
                    Some(Value::Int(i)) => builder.append_value(*i),
                    None => builder.append_null(),
                    _ => {
                        return Err(ArrowError::InvalidArgumentError(
                            "Expected int64 value".into(),
                        ));
                    },
                }
            },
            DataType::Float64 => {
                let builder = builder
                    .as_any_mut()
                    .downcast_mut::<Float64Builder>()
                    .expect("Builder type mismatch");
                match value {
                    Some(Value::Float(f)) => builder.append_value(*f),
                    None => builder.append_null(),
                    _ => {
                        return Err(ArrowError::InvalidArgumentError(
                            "Expected float value".into(),
                        ));
                    },
                }
            },
            DataType::Boolean => {
                let builder = builder
                    .as_any_mut()
                    .downcast_mut::<BooleanBuilder>()
                    .expect("Builder type mismatch");
                match value {
                    Some(Value::Bool(b)) => builder.append_value(*b),
                    None => builder.append_null(),
                    _ => {
                        return Err(ArrowError::InvalidArgumentError("Expected bool value".into()));
                    },
                }
            },
            DataType::Timestamp(TimeUnit::Nanosecond, _) => {
                let builder = builder
                    .as_any_mut()
                    .downcast_mut::<TimestampNanosecondBuilder>()
                    .expect("Builder type mismatch");
                match value {
                    Some(Value::Timestamp(nanos)) => builder.append_value(*nanos),
                    None => builder.append_null(),
                    _ => {
                        return Err(ArrowError::InvalidArgumentError(
                            "Expected timestamp value".into(),
                        ));
                    },
                }
            },
            DataType::Date32 => {
                let builder = builder
                    .as_any_mut()
                    .downcast_mut::<Date32Builder>()
                    .expect("Builder type mismatch");
                match value {
                    Some(Value::Date(days)) => builder.append_value(*days),
                    None => builder.append_null(),
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

/// Create an array builder for a given Arrow data type.
///
/// # Panics
///
/// Panics if the data type is not supported. This is intentional as it indicates
/// a schema generation bug rather than a runtime issue.
fn create_builder_for_type(data_type: &DataType, capacity: usize) -> Box<dyn ArrayBuilder> {
    match data_type {
        DataType::Utf8 => {
            // Estimate 50 bytes per string on average
            Box::new(StringBuilder::with_capacity(capacity, capacity * 50))
        },
        DataType::Int32 => Box::new(Int32Builder::with_capacity(capacity)),
        DataType::Int64 => Box::new(Int64Builder::with_capacity(capacity)),
        DataType::Float64 => Box::new(Float64Builder::with_capacity(capacity)),
        DataType::Boolean => Box::new(BooleanBuilder::with_capacity(capacity)),
        DataType::Timestamp(TimeUnit::Nanosecond, tz) => Box::new(
            TimestampNanosecondBuilder::with_capacity(capacity).with_timezone_opt(tz.clone()),
        ),
        DataType::Date32 => Box::new(Date32Builder::with_capacity(capacity)),
        _ => panic!("Unsupported data type in create_builder_for_type: {data_type:?}"),
    }
}

#[cfg(test)]
mod tests {
    use arrow::datatypes::Field;

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
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("expected 2"));
    }

    #[test]
    fn test_wrong_value_type() {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int32, false)]));

        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());

        // Providing string instead of int
        let rows = vec![vec![Some(Value::String("not an int".to_string()))]];

        let result = converter.convert_batch(rows);
        assert!(result.is_err());
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
}
