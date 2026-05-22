//! Bulk export functionality for multiple data formats.
//!
//! Supports exporting Arrow `RecordBatches` to Parquet, CSV, and JSON formats.

use std::str::FromStr;

use arrow::array::RecordBatch;

/// Supported export formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExportFormat {
    /// Apache Parquet columnar format.
    ///
    /// Available only when the `parquet` feature is enabled.
    #[cfg(feature = "parquet")]
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
            #[cfg(feature = "parquet")]
            "parquet" => Ok(Self::Parquet),
            #[cfg(not(feature = "parquet"))]
            "parquet" => {
                Err("Parquet export requires the `parquet` Cargo feature (disabled by default due \
                 to CVE-2026-43868 in transitive thrift dep)"
                    .into())
            },
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
            #[cfg(feature = "parquet")]
            Self::Parquet => "parquet",
            Self::Csv => "csv",
            Self::Json => "jsonl",
        }
    }

    /// Get MIME type for this format.
    #[must_use]
    pub const fn mime_type(&self) -> &'static str {
        match self {
            #[cfg(feature = "parquet")]
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
            #[cfg(feature = "parquet")]
            ExportFormat::Parquet => Self::export_parquet(batch),
            ExportFormat::Csv => Self::export_csv(batch),
            ExportFormat::Json => Self::export_json(batch),
        }
    }

    /// Export `RecordBatch` to Parquet format.
    ///
    /// Parquet provides efficient columnar storage with compression.
    /// Ideal for large datasets and analytical workloads.
    #[cfg(feature = "parquet")]
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
    #[must_use]
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
mod tests;
