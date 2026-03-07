//! Bulk export functionality for multiple data formats.
//!
//! Supports exporting Arrow `RecordBatches` to Parquet, CSV, and JSON formats.

use std::str::FromStr;

use arrow::array::RecordBatch;

/// Supported export formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// Apache Parquet columnar format
    Parquet,
    /// Comma-separated values
    Csv,
    /// JSON Lines (one JSON object per line)
    Json,
}

impl FromStr for ExportFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "parquet" => Ok(Self::Parquet),
            "csv" => Ok(Self::Csv),
            "json" => Ok(Self::Json),
            _ => Err(format!("Unsupported export format: {}", s)),
        }
    }
}

impl ExportFormat {
    /// Parse export format from string (case-insensitive).
    ///
    /// # Errors
    ///
    /// Returns error if format string is not recognized.
    ///
    /// # Note
    ///
    /// This method is a convenience wrapper around the `FromStr` trait impl.
    /// Prefer using `.parse()` for idiomatic Rust code.
    #[allow(clippy::should_implement_trait)] // Reason: from_* naming is intentional for builder ergonomics; From trait would consume self
    pub fn from_str(s: &str) -> Result<Self, String> {
        <Self as FromStr>::from_str(s)
    }

    /// Get file extension for this format.
    #[must_use]
    pub const fn extension(&self) -> &'static str {
        match self {
            Self::Parquet => "parquet",
            Self::Csv => "csv",
            Self::Json => "jsonl",
        }
    }

    /// Get MIME type for this format.
    #[must_use]
    pub const fn mime_type(&self) -> &'static str {
        match self {
            Self::Parquet => "application/octet-stream",
            Self::Csv => "text/csv",
            Self::Json => "application/x-ndjson",
        }
    }
}

/// Bulk exporter for converting Arrow `RecordBatches` to various formats.
pub struct BulkExporter;

impl BulkExporter {
    /// Export a `RecordBatch` to the specified format.
    ///
    /// # Arguments
    ///
    /// * `batch` - Arrow `RecordBatch` to export
    /// * `format` - Target export format
    ///
    /// # Returns
    ///
    /// Byte vector containing the exported data
    ///
    /// # Errors
    ///
    /// Returns error if export fails (e.g., Parquet encoding error)
    pub fn export_batch(batch: &RecordBatch, format: ExportFormat) -> Result<Vec<u8>, String> {
        match format {
            ExportFormat::Parquet => Self::export_parquet(batch),
            ExportFormat::Csv => Self::export_csv(batch),
            ExportFormat::Json => Self::export_json(batch),
        }
    }

    /// Export `RecordBatch` to Parquet format.
    ///
    /// Parquet provides efficient columnar storage with compression.
    /// Ideal for large datasets and analytical workloads.
    fn export_parquet(batch: &RecordBatch) -> Result<Vec<u8>, String> {
        use parquet::arrow::ArrowWriter;

        let mut buf = Vec::new();

        {
            let mut writer = ArrowWriter::try_new(&mut buf, batch.schema(), None)
                .map_err(|e| format!("Failed to create Parquet writer: {}", e))?;

            writer
                .write(batch)
                .map_err(|e| format!("Failed to write Parquet data: {}", e))?;

            writer.close().map_err(|e| format!("Failed to close Parquet writer: {}", e))?;
        }

        Ok(buf)
    }

    /// Export `RecordBatch` to CSV format.
    ///
    /// CSV is widely compatible and human-readable.
    /// Good for data interchange and spreadsheet applications.
    fn export_csv(batch: &RecordBatch) -> Result<Vec<u8>, String> {
        use arrow::csv::Writer;

        let mut buf = Vec::new();

        {
            let mut writer = Writer::new(&mut buf);

            writer.write(batch).map_err(|e| format!("Failed to write CSV data: {}", e))?;
        }

        Ok(buf)
    }

    /// Export `RecordBatch` to JSON Lines format (NDJSON).
    ///
    /// Each row is a separate JSON object (one per line).
    /// Good for streaming and log-based consumption.
    fn export_json(batch: &RecordBatch) -> Result<Vec<u8>, String> {
        use arrow::json::LineDelimitedWriter;

        let mut buf = Vec::new();

        {
            let mut writer = LineDelimitedWriter::new(&mut buf);

            writer.write(batch).map_err(|e| format!("Failed to write JSON data: {}", e))?;

            writer.finish().map_err(|e| format!("Failed to finish JSON writer: {}", e))?;
        }

        Ok(buf)
    }

    /// Get statistics about exported data.
    ///
    /// Useful for logging and monitoring export operations.
    pub fn batch_stats(batch: &RecordBatch) -> BatchStats {
        let num_rows = batch.num_rows();
        let num_cols = batch.num_columns();
        let memory_bytes = batch.get_array_memory_size();

        BatchStats {
            num_rows,
            num_columns: num_cols,
            memory_bytes,
        }
    }
}

/// Statistics about an exported `RecordBatch`
#[derive(Debug, Clone)]
pub struct BatchStats {
    /// Number of rows
    pub num_rows:     usize,
    /// Number of columns
    pub num_columns:  usize,
    /// Approximate memory usage in bytes
    pub memory_bytes: usize,
}

impl BatchStats {
    /// Get human-readable summary
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "Batch: {} rows, {} columns, ~{} MB",
            self.num_rows,
            self.num_columns,
            self.memory_bytes / (1024 * 1024)
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code extensively uses unwrap for test fixture setup
mod tests {
    use std::sync::Arc;

    use arrow::{
        array::{ArrayRef, StringArray},
        datatypes::{DataType, Field, Schema},
    };

    use super::*;

    fn create_test_batch() -> RecordBatch {
        let schema = Schema::new(vec![
            Field::new("name", DataType::Utf8, false),
            Field::new("age", DataType::Utf8, false),
        ]);

        let names = Arc::new(StringArray::from(vec!["Alice", "Bob", "Charlie"]));
        let ages = Arc::new(StringArray::from(vec!["30", "25", "35"]));

        RecordBatch::try_new(Arc::new(schema), vec![names, ages]).expect("should create batch")
    }

    #[test]
    fn test_export_format_from_str() {
        assert_eq!(ExportFormat::from_str("parquet").unwrap(), ExportFormat::Parquet);
        assert_eq!(ExportFormat::from_str("csv").unwrap(), ExportFormat::Csv);
        assert_eq!(ExportFormat::from_str("json").unwrap(), ExportFormat::Json);

        // Case-insensitive
        assert_eq!(ExportFormat::from_str("PARQUET").unwrap(), ExportFormat::Parquet);

        // Invalid format
        assert!(ExportFormat::from_str("invalid").is_err());
    }

    #[test]
    fn test_export_format_extension() {
        assert_eq!(ExportFormat::Parquet.extension(), "parquet");
        assert_eq!(ExportFormat::Csv.extension(), "csv");
        assert_eq!(ExportFormat::Json.extension(), "jsonl");
    }

    #[test]
    fn test_export_format_mime_type() {
        assert_eq!(ExportFormat::Parquet.mime_type(), "application/octet-stream");
        assert_eq!(ExportFormat::Csv.mime_type(), "text/csv");
        assert_eq!(ExportFormat::Json.mime_type(), "application/x-ndjson");
    }

    #[test]
    fn test_export_csv() {
        let batch = create_test_batch();
        let exported = BulkExporter::export_batch(&batch, ExportFormat::Csv);

        assert!(exported.is_ok());
        let bytes = exported.unwrap();
        assert!(!bytes.is_empty());

        // CSV should contain headers
        let csv_str = String::from_utf8(bytes).unwrap();
        assert!(csv_str.contains("name"));
        assert!(csv_str.contains("age"));
        assert!(csv_str.contains("Alice"));
        assert!(csv_str.contains("30"));
    }

    #[test]
    fn test_export_json() {
        let batch = create_test_batch();
        let exported = BulkExporter::export_batch(&batch, ExportFormat::Json);

        assert!(exported.is_ok());
        let bytes = exported.unwrap();
        assert!(!bytes.is_empty());

        // JSON Lines should contain JSON objects
        let json_str = String::from_utf8(bytes).unwrap();
        assert!(json_str.contains("\"name\""));
        assert!(json_str.contains("\"age\""));
        assert!(json_str.contains("Alice"));
    }

    #[test]
    fn test_export_parquet() {
        let batch = create_test_batch();
        let exported = BulkExporter::export_batch(&batch, ExportFormat::Parquet);

        assert!(exported.is_ok());
        let bytes = exported.unwrap();
        assert!(!bytes.is_empty());

        // Parquet files start with "PAR1" magic bytes
        assert_eq!(&bytes[0..4], b"PAR1");
    }

    #[test]
    fn test_batch_stats() {
        let batch = create_test_batch();
        let stats = BulkExporter::batch_stats(&batch);

        assert_eq!(stats.num_rows, 3);
        assert_eq!(stats.num_columns, 2);
        assert!(stats.memory_bytes > 0);

        // Should produce valid summary
        let summary = stats.summary();
        assert!(summary.contains("3 rows"));
        assert!(summary.contains("2 columns"));
    }

    #[test]
    fn test_export_empty_batch() {
        let schema = Schema::new(vec![Field::new("id", DataType::Utf8, false)]);
        let empty_str_vec: Vec<&str> = vec![];
        let empty_array = Arc::new(StringArray::from(empty_str_vec)) as ArrayRef;
        let batch = RecordBatch::try_new(Arc::new(schema), vec![empty_array])
            .expect("should create empty batch");

        // All formats should handle empty batches
        let csv = BulkExporter::export_batch(&batch, ExportFormat::Csv);
        let json = BulkExporter::export_batch(&batch, ExportFormat::Json);
        let parquet = BulkExporter::export_batch(&batch, ExportFormat::Parquet);

        assert!(csv.is_ok());
        assert!(json.is_ok());
        assert!(parquet.is_ok());
    }

    // --- Additional export format tests ---

    #[test]
    fn test_export_format_parse_trait_lowercase_csv() {
        let fmt: ExportFormat = "csv".parse().unwrap();
        assert_eq!(fmt, ExportFormat::Csv);
    }

    #[test]
    fn test_export_format_parse_trait_uppercase_json() {
        let fmt: ExportFormat = "JSON".parse().unwrap();
        assert_eq!(fmt, ExportFormat::Json);
    }

    #[test]
    fn test_export_format_parse_trait_mixed_case_parquet() {
        let fmt: ExportFormat = "Parquet".parse().unwrap();
        assert_eq!(fmt, ExportFormat::Parquet);
    }

    #[test]
    fn test_export_format_parse_unknown_returns_err() {
        let result: Result<ExportFormat, _> = "avro".parse();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unsupported export format"));
    }

    #[test]
    fn test_export_format_clone_and_eq() {
        let fmt = ExportFormat::Parquet;
        let cloned = fmt;
        assert_eq!(fmt, cloned);
    }

    #[test]
    fn test_csv_export_contains_all_row_data() {
        let batch = create_test_batch();
        let bytes = BulkExporter::export_batch(&batch, ExportFormat::Csv).unwrap();
        let csv_str = String::from_utf8(bytes).unwrap();
        assert!(csv_str.contains("Bob"));
        assert!(csv_str.contains("Charlie"));
    }

    #[test]
    fn test_json_export_contains_all_row_data() {
        let batch = create_test_batch();
        let bytes = BulkExporter::export_batch(&batch, ExportFormat::Json).unwrap();
        let json_str = String::from_utf8(bytes).unwrap();
        assert!(json_str.contains("Bob"));
        assert!(json_str.contains("Charlie"));
    }

    #[test]
    fn test_parquet_export_ends_with_magic_bytes() {
        let batch = create_test_batch();
        let bytes = BulkExporter::export_batch(&batch, ExportFormat::Parquet).unwrap();
        // Parquet files start AND end with "PAR1"
        assert_eq!(&bytes[0..4], b"PAR1");
        let len = bytes.len();
        assert!(len >= 4);
        assert_eq!(&bytes[len - 4..], b"PAR1");
    }

    #[test]
    fn test_batch_stats_empty_batch_has_zero_rows() {
        let schema = Schema::new(vec![Field::new("x", DataType::Utf8, false)]);
        let empty_str_vec: Vec<&str> = vec![];
        let empty_array = Arc::new(StringArray::from(empty_str_vec)) as ArrayRef;
        let batch = RecordBatch::try_new(Arc::new(schema), vec![empty_array])
            .expect("should create empty batch");
        let stats = BulkExporter::batch_stats(&batch);
        assert_eq!(stats.num_rows, 0);
        assert_eq!(stats.num_columns, 1);
    }

    #[test]
    fn test_batch_stats_summary_format() {
        let batch = create_test_batch();
        let stats = BulkExporter::batch_stats(&batch);
        let summary = stats.summary();
        // Should include rows, columns, MB
        assert!(summary.contains("rows"));
        assert!(summary.contains("columns"));
        assert!(summary.contains("MB"));
    }

    #[test]
    fn test_json_export_is_valid_ndjson() {
        let batch = create_test_batch();
        let bytes = BulkExporter::export_batch(&batch, ExportFormat::Json).unwrap();
        let json_str = String::from_utf8(bytes).unwrap();
        // Each non-empty line should be valid JSON
        let non_empty_lines: Vec<&str> =
            json_str.lines().filter(|l| !l.trim().is_empty()).collect();
        assert!(!non_empty_lines.is_empty(), "expected at least one line");
        for line in non_empty_lines {
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(line);
            assert!(parsed.is_ok(), "line is not valid JSON: {line}");
        }
    }

    #[test]
    fn test_export_format_debug_is_nonempty() {
        let fmt = ExportFormat::Csv;
        let s = format!("{fmt:?}");
        assert!(!s.is_empty());
    }
}
