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
            max_rows: None,
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
mod tests;
