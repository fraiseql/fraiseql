//! Arrow/Flight conversion and SQL utility functions.

use std::sync::Arc;

use arrow::{
    array::RecordBatch,
    ipc::writer::{DictionaryTracker, IpcDataGenerator, IpcWriteOptions},
};
use arrow_flight::FlightData;
use tonic::Status;
use tracing::warn;

/// Convert RecordBatch to FlightData using Arrow IPC encoding.
///
/// # Arguments
///
/// * `batch` - Arrow RecordBatch to encode
///
/// # Returns
///
/// FlightData message with IPC-encoded batch
///
/// # Errors
///
/// Returns error if IPC encoding fails
#[allow(clippy::result_large_err)]
pub(crate) fn record_batch_to_flight_data(
    batch: &RecordBatch,
) -> std::result::Result<FlightData, Status> {
    use arrow::ipc::writer::CompressionContext;

    let options = IpcWriteOptions::default();
    let data_gen = IpcDataGenerator::default();
    let mut dict_tracker = DictionaryTracker::new(false);
    let mut compression = CompressionContext::default();

    let (_, encoded_data) =
        data_gen
            .encode(batch, &mut dict_tracker, &options, &mut compression)
            .map_err(|e| Status::internal(format!("Failed to encode RecordBatch: {e}")))?;

    Ok(FlightData {
        data_header: encoded_data.ipc_message.into(),
        data_body: encoded_data.arrow_data.into(),
        ..Default::default()
    })
}

/// Convert schema to FlightData for initial message.
///
/// # Arguments
///
/// * `schema` - Arrow schema to encode
///
/// # Returns
///
/// FlightData message with IPC-encoded schema
///
/// # Errors
///
/// Returns error if IPC encoding fails
#[allow(clippy::result_large_err)]
pub(crate) fn schema_to_flight_data(
    schema: &Arc<arrow::datatypes::Schema>,
) -> std::result::Result<FlightData, Status> {
    let options = IpcWriteOptions::default();
    let data_gen = IpcDataGenerator::default();
    let mut dict_tracker = DictionaryTracker::new(false);

    let encoded_data =
        data_gen.schema_to_bytes_with_dictionary_tracker(schema, &mut dict_tracker, &options);

    Ok(FlightData {
        data_header: encoded_data.ipc_message.into(),
        data_body: vec![].into(),
        ..Default::default()
    })
}

/// Build optimized SQL query for va_* view.
///
/// # Arguments
///
/// * `view` - View name (e.g., "va_orders")
/// * `filter` - Optional WHERE clause
/// * `order_by` - Optional ORDER BY clause
/// * `limit` - Optional LIMIT
/// * `offset` - Optional OFFSET
///
/// # Returns
///
/// SQL query string
///
/// # Example
///
/// ```ignore
/// let sql = build_optimized_sql(
///     "va_orders",
///     Some("created_at > '2026-01-01'"),
///     Some("created_at DESC"),
///     Some(100),
///     Some(0)
/// );
/// // Returns: "SELECT * FROM va_orders WHERE created_at > '2026-01-01' ORDER BY created_at DESC LIMIT 100 OFFSET 0"
/// ```
pub(crate) fn build_optimized_sql(
    view: &str,
    filter: Option<String>,
    order_by: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> String {
    let mut sql = format!("SELECT * FROM {view}");

    if let Some(where_clause) = filter {
        sql.push_str(&format!(" WHERE {where_clause}"));
    }

    if let Some(order_clause) = order_by {
        sql.push_str(&format!(" ORDER BY {order_clause}"));
    }

    if let Some(limit_value) = limit {
        sql.push_str(&format!(" LIMIT {limit_value}"));
    }

    if let Some(offset_value) = offset {
        sql.push_str(&format!(" OFFSET {offset_value}"));
    }

    sql
}

/// Generate placeholder database rows for development/testing.
///
/// This function is only called when no database adapter is configured.
/// In production, `execute_optimized_view()` uses the real database adapter.
///
/// This fallback provides consistent test data matching the expected schema:
/// - va_orders, va_users: Real timestamp and numeric types
/// - ta_orders, ta_users: String-based data (ISO 8601 timestamps)
///
/// # Arguments
///
/// * `view` - View name (e.g., "va_orders", "va_users")
/// * `limit` - Optional limit on number of rows
///
/// # Returns
///
/// Vec of rows as HashMap<column_name, json_value>
pub(crate) fn execute_placeholder_query(
    view: &str,
    limit: Option<usize>,
) -> Vec<std::collections::HashMap<String, serde_json::Value>> {
    use std::collections::HashMap;

    use serde_json::json;

    let row_count = limit.unwrap_or(10).min(100); // Cap at 100 for testing
    let mut rows = Vec::with_capacity(row_count);

    match view {
        "va_orders" => {
            // Schema: id (Int64), total (Float64), created_at (Timestamp), customer_name (Utf8)
            for i in 0..row_count {
                let mut row = HashMap::new();
                row.insert("id".to_string(), json!(i64::from(i as i32 + 1)));
                row.insert("total".to_string(), json!((i as f64 + 1.0) * 99.99));
                row.insert(
                    "created_at".to_string(),
                    json!(1_700_000_000_000_000_i64 + i64::from(i as i32) * 86_400_000_000),
                );
                row.insert("customer_name".to_string(), json!(format!("Customer {}", i + 1)));
                rows.push(row);
            }
        },
        "va_users" => {
            // Schema: id (Int64), email (Utf8), name (Utf8), created_at (Timestamp)
            for i in 0..row_count {
                let mut row = HashMap::new();
                row.insert("id".to_string(), json!(i64::from(i as i32 + 1)));
                row.insert("email".to_string(), json!(format!("user{}@example.com", i + 1)));
                row.insert("name".to_string(), json!(format!("User {}", i + 1)));
                row.insert(
                    "created_at".to_string(),
                    json!(1_700_000_000_000_000_i64 + i64::from(i as i32) * 86_400_000_000),
                );
                rows.push(row);
            }
        },
        "ta_orders" => {
            // Schema: id (Utf8), total (Utf8), created_at (Utf8 ISO 8601), customer_name (Utf8)
            for i in 0..row_count {
                let mut row = HashMap::new();
                row.insert("id".to_string(), json!(format!("order-{}", i + 1)));
                row.insert("total".to_string(), json!(format!("{:.2}", (i as f64 + 1.0) * 99.99)));
                // ISO 8601 timestamp format
                row.insert(
                    "created_at".to_string(),
                    json!(format!("2025-11-{:02}T12:00:00Z", (i % 30) + 1)),
                );
                row.insert("customer_name".to_string(), json!(format!("Customer {}", i + 1)));
                rows.push(row);
            }
        },
        "ta_users" => {
            // Schema: id (Utf8), email (Utf8), name (Utf8), created_at (Utf8 ISO 8601)
            for i in 0..row_count {
                let mut row = HashMap::new();
                row.insert("id".to_string(), json!(format!("user-{}", i + 1)));
                row.insert("email".to_string(), json!(format!("user{}@example.com", i + 1)));
                row.insert("name".to_string(), json!(format!("User {}", i + 1)));
                // ISO 8601 timestamp format
                row.insert(
                    "created_at".to_string(),
                    json!(format!("2025-11-{:02}T12:00:00Z", (i % 30) + 1)),
                );
                rows.push(row);
            }
        },
        _ => {
            // Unknown view, return empty rows
            warn!("Unknown view '{}', returning empty result", view);
        },
    }

    rows
}

/// Decode FlightData message into an Arrow RecordBatch.
///
/// Parses the IPC format data contained in FlightData.data_body.
///
/// # Arguments
/// * `flight_data` - FlightData message containing serialized RecordBatch
///
/// # Returns
/// Decoded RecordBatch
///
/// # Errors
/// Returns error if decoding fails
pub(crate) fn decode_flight_data_to_batch(
    flight_data: &FlightData,
) -> std::result::Result<RecordBatch, String> {
    use std::io::Cursor;

    use arrow::ipc::reader::StreamReader;

    if flight_data.data_body.is_empty() {
        return Err("Empty flight data body".to_string());
    }

    let cursor = Cursor::new(&flight_data.data_body);
    let mut reader = StreamReader::try_new(cursor, None)
        .map_err(|e| format!("Failed to create IPC stream reader: {}", e))?;

    // Read first batch from the stream
    reader
        .next()
        .ok_or_else(|| "No batch in flight data message".to_string())?
        .map_err(|e| format!("Failed to read batch: {}", e))
}

/// Quote a PostgreSQL identifier (table name, column name, etc).
///
/// Wraps the identifier in double quotes and escapes internal quotes.
/// This prevents SQL injection and handles reserved keywords.
///
/// # Arguments
/// * `identifier` - Table or column name
///
/// # Returns
/// Quoted identifier safe for SQL
///
/// # Example
/// ```ignore
/// assert_eq!(quote_identifier("order"), "\"order\"");
/// assert_eq!(quote_identifier("my\"table"), "\"my\"\"table\"");
/// ```
fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

/// Convert an Arrow RecordBatch column value to SQL literal.
///
/// Handles type conversion and escaping for SQL INSERT statements.
///
/// # Arguments
/// * `array` - Arrow Array column data
/// * `row` - Row index in the array
///
/// # Returns
/// SQL literal string (e.g., "123", "'text'", "NULL")
///
/// # Errors
/// Returns error message if unsupported Arrow type
fn arrow_value_to_sql(
    array: &std::sync::Arc<dyn arrow::array::Array>,
    row: usize,
) -> std::result::Result<String, String> {
    use arrow::{array::*, datatypes::DataType};

    if array.is_null(row) {
        return Ok("NULL".to_string());
    }

    match array.data_type() {
        DataType::Int8 => {
            let arr = array
                .as_any()
                .downcast_ref::<Int8Array>()
                .ok_or("Failed to cast to Int8Array")?;
            Ok(arr.value(row).to_string())
        },
        DataType::Int16 => {
            let arr = array
                .as_any()
                .downcast_ref::<Int16Array>()
                .ok_or("Failed to cast to Int16Array")?;
            Ok(arr.value(row).to_string())
        },
        DataType::Int32 => {
            let arr = array
                .as_any()
                .downcast_ref::<Int32Array>()
                .ok_or("Failed to cast to Int32Array")?;
            Ok(arr.value(row).to_string())
        },
        DataType::Int64 => {
            let arr = array
                .as_any()
                .downcast_ref::<Int64Array>()
                .ok_or("Failed to cast to Int64Array")?;
            Ok(arr.value(row).to_string())
        },
        DataType::UInt8 => {
            let arr = array
                .as_any()
                .downcast_ref::<UInt8Array>()
                .ok_or("Failed to cast to UInt8Array")?;
            Ok(arr.value(row).to_string())
        },
        DataType::UInt16 => {
            let arr = array
                .as_any()
                .downcast_ref::<UInt16Array>()
                .ok_or("Failed to cast to UInt16Array")?;
            Ok(arr.value(row).to_string())
        },
        DataType::UInt32 => {
            let arr = array
                .as_any()
                .downcast_ref::<UInt32Array>()
                .ok_or("Failed to cast to UInt32Array")?;
            Ok(arr.value(row).to_string())
        },
        DataType::UInt64 => {
            let arr = array
                .as_any()
                .downcast_ref::<UInt64Array>()
                .ok_or("Failed to cast to UInt64Array")?;
            Ok(arr.value(row).to_string())
        },
        DataType::Float32 => {
            let arr = array
                .as_any()
                .downcast_ref::<Float32Array>()
                .ok_or("Failed to cast to Float32Array")?;
            Ok(arr.value(row).to_string())
        },
        DataType::Float64 => {
            let arr = array
                .as_any()
                .downcast_ref::<Float64Array>()
                .ok_or("Failed to cast to Float64Array")?;
            Ok(arr.value(row).to_string())
        },
        DataType::Utf8 => {
            let arr = array
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or("Failed to cast to StringArray")?;
            let val = arr.value(row);
            // Escape single quotes for SQL string literals
            Ok(format!("'{}'", val.replace('\'', "''")))
        },
        DataType::LargeUtf8 => {
            let arr = array
                .as_any()
                .downcast_ref::<LargeStringArray>()
                .ok_or("Failed to cast to LargeStringArray")?;
            let val = arr.value(row);
            Ok(format!("'{}'", val.replace('\'', "''")))
        },
        DataType::Boolean => {
            let arr = array
                .as_any()
                .downcast_ref::<BooleanArray>()
                .ok_or("Failed to cast to BooleanArray")?;
            Ok(if arr.value(row) { "true" } else { "false" }.to_string())
        },
        DataType::Timestamp(_, _) => {
            // Try as microseconds (common format)
            if let Some(arr) = array.as_any().downcast_ref::<TimestampMicrosecondArray>() {
                let ts = arr.value(row);
                let secs = ts / 1_000_000;
                let nanos = (ts % 1_000_000) * 1000;
                return Ok(format!("to_timestamp({}, {})", secs, nanos));
            }
            // Try as nanoseconds
            if let Some(arr) = array.as_any().downcast_ref::<TimestampNanosecondArray>() {
                let ts = arr.value(row);
                let secs = ts / 1_000_000_000;
                let nanos = ts % 1_000_000_000;
                return Ok(format!("to_timestamp({}, {})", secs, nanos));
            }
            // Try as milliseconds
            if let Some(arr) = array.as_any().downcast_ref::<TimestampMillisecondArray>() {
                let ts = arr.value(row);
                let secs = ts / 1_000;
                let millis = ts % 1_000;
                return Ok(format!("to_timestamp({}, {})", secs, millis * 1_000_000));
            }
            // Try as seconds
            if let Some(arr) = array.as_any().downcast_ref::<TimestampSecondArray>() {
                let ts = arr.value(row);
                return Ok(format!("to_timestamp({})", ts));
            }
            Err(format!("Unsupported timestamp precision: {:?}", array.data_type()))
        },
        DataType::Date32 => {
            let arr = array
                .as_any()
                .downcast_ref::<Date32Array>()
                .ok_or("Failed to cast to Date32Array")?;
            let days_since_epoch = arr.value(row);
            // Calculate date from days since epoch (1970-01-01)
            let epoch_date =
                chrono::NaiveDate::from_ymd_opt(1970, 1, 1).ok_or("Failed to create epoch date")?;
            let target_date = epoch_date + chrono::Duration::days(i64::from(days_since_epoch));
            Ok(format!("'{}'", target_date))
        },
        _ => Err(format!("Unsupported Arrow type for SQL conversion: {:?}", array.data_type())),
    }
}

/// Build a SQL INSERT statement from a RecordBatch.
///
/// Generates parameterized INSERT query with proper escaping.
///
/// # Arguments
/// * `table_name` - Target table name
/// * `batch` - Arrow RecordBatch containing rows to insert
///
/// # Returns
/// SQL INSERT statement
///
/// # Errors
/// Returns error if column types are unsupported
pub(crate) fn build_insert_query(
    table_name: &str,
    batch: &RecordBatch,
) -> std::result::Result<String, String> {
    let schema = batch.schema();
    let num_rows = batch.num_rows();
    let num_cols = batch.num_columns();

    if num_rows == 0 || num_cols == 0 {
        return Err("RecordBatch is empty".to_string());
    }

    // Build column list
    let columns: Vec<String> = schema.fields().iter().map(|f| quote_identifier(f.name())).collect();

    // Build VALUES clause for each row
    let mut values_clauses = Vec::new();
    for row_idx in 0..num_rows {
        let mut row_values = Vec::new();
        for col_idx in 0..num_cols {
            let array = batch.column(col_idx);
            let value = arrow_value_to_sql(array, row_idx)?;
            row_values.push(value);
        }
        values_clauses.push(format!("({})", row_values.join(", ")));
    }

    Ok(format!(
        "INSERT INTO {} ({}) VALUES {}",
        quote_identifier(table_name),
        columns.join(", "),
        values_clauses.join(", ")
    ))
}

/// Encode JSON result from GraphQL query into Arrow RecordBatch.
///
/// Converts JSON query result into columnar Arrow format for efficient streaming.
/// As a simple implementation, wraps JSON in a single string column.
///
/// # Arguments
/// * `json_str` - JSON string from GraphQL query result
///
/// # Returns
/// Arrow RecordBatch with single "result" column
///
/// # Errors
/// Returns error if RecordBatch creation fails
pub(crate) fn encode_json_to_arrow_batch(
    json_str: &str,
) -> std::result::Result<RecordBatch, String> {
    use arrow::{
        array::StringArray,
        datatypes::{DataType, Field, Schema},
    };

    // Create a simple single-column batch with JSON result
    let schema = Arc::new(Schema::new(vec![Field::new("result", DataType::Utf8, false)]));
    let string_array = StringArray::from(vec![json_str]);
    let batch = RecordBatch::try_new(schema, vec![Arc::new(string_array)])
        .map_err(|e| format!("Failed to create RecordBatch: {}", e))?;

    Ok(batch)
}

/// Decode serialized Arrow RecordBatch from upload request.
///
/// Deserializes Arrow IPC format batch data.
///
/// # Arguments
/// * `batch_bytes` - Serialized RecordBatch in Arrow IPC format
///
/// # Returns
/// Decoded RecordBatch
///
/// # Errors
/// Returns error if deserialization fails
pub(crate) fn decode_upload_batch(batch_bytes: &[u8]) -> std::result::Result<RecordBatch, String> {
    use std::io::Cursor;

    use arrow::ipc::reader::StreamReader;

    if batch_bytes.is_empty() {
        return Err("Empty batch data".to_string());
    }

    let cursor = Cursor::new(batch_bytes);
    let mut reader = StreamReader::try_new(cursor, None)
        .map_err(|e| format!("Failed to create IPC stream reader: {}", e))?;

    // Read first batch from the stream
    reader
        .next()
        .ok_or_else(|| "No batch in data".to_string())?
        .map_err(|e| format!("Failed to read batch: {}", e))
}
