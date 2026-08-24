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

    /// Export a sequence of `RecordBatches` as a single document.
    ///
    /// Returns one byte chunk per batch; concatenating the chunks in order yields
    /// exactly **one** well-formed document in `format`. Callers stream the chunks
    /// in order and must not reorder or drop any.
    ///
    /// Exporting each batch independently — as this did before #1036 — instead
    /// produces N self-contained documents on one stream: N CSV headers, or N
    /// Parquet files each with its own footer. That is invisible below the
    /// 10 000-row batch size and silently corrupts every export above it.
    ///
    /// Row-oriented formats (CSV, JSON Lines) stream one chunk per batch, so peak
    /// memory holds only the current batch's serialised payload. Parquet writes a
    /// footer describing the whole file, so it cannot be split across chunks: the
    /// entire file is assembled and returned as a single chunk.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying encoder fails.
    pub fn export_batches(
        batches: &[RecordBatch],
        format: ExportFormat,
    ) -> Result<Vec<Vec<u8>>, String> {
        if batches.is_empty() {
            return Ok(Vec::new());
        }

        let mut writer = BatchExportWriter::new(format);
        let mut chunks = Vec::new();

        for batch in batches {
            let bytes = writer.write(batch)?;
            if !bytes.is_empty() {
                chunks.push(bytes);
            }
        }

        let trailer = writer.finish()?;
        if !trailer.is_empty() {
            chunks.push(trailer);
        }

        Ok(chunks)
    }

    /// Write every batch through a single `ArrowWriter` so the result is one
    /// Parquet file with one footer.
    #[cfg(feature = "parquet")]
    fn export_parquet_batches(batches: &[RecordBatch]) -> Result<Vec<u8>, String> {
        use parquet::arrow::ArrowWriter;

        let Some(first) = batches.first() else {
            return Ok(Vec::new());
        };

        let mut buf = Vec::new();

        {
            let mut writer = ArrowWriter::try_new(&mut buf, first.schema(), None)
                .map_err(|e| format!("Failed to create Parquet writer: {}", e))?;

            for batch in batches {
                writer
                    .write(batch)
                    .map_err(|e| format!("Failed to write Parquet data: {}", e))?;
            }

            writer.close().map_err(|e| format!("Failed to close Parquet writer: {}", e))?;
        }

        Ok(buf)
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
        Self::export_csv_with_header(batch, true)
    }

    /// Export one `RecordBatch` to CSV, emitting the header row only when asked.
    ///
    /// A multi-batch export is a single CSV document, so only the first chunk
    /// carries the header (#1036).
    fn export_csv_with_header(
        batch: &RecordBatch,
        include_header: bool,
    ) -> Result<Vec<u8>, String> {
        use arrow::csv::WriterBuilder;

        let mut buf = Vec::new();

        {
            let mut writer = WriterBuilder::new().with_header(include_header).build(&mut buf);

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

/// Incremental writer that turns a sequence of `RecordBatches` into one document.
///
/// A bulk export is chunked into 10 000-row batches and streamed one Flight
/// message per batch. The document-level framing — the CSV header row, the
/// Parquet footer — belongs to the export, not to any single batch, so it cannot
/// be re-emitted per message (#1036).
///
/// Row-oriented formats stay lazy: `write` serialises only the batch it is given,
/// so a caller driving this from a bounded channel holds one batch's payload at a
/// time rather than the whole export (F011). Parquet's footer describes every row
/// group in the file, so it cannot be split across messages; those batches are
/// held and the complete file is returned by [`BatchExportWriter::finish`].
pub struct BatchExportWriter {
    format:    ExportFormat,
    /// Whether any batch has been written, so the CSV header is emitted once.
    wrote_any: bool,
    /// Parquet only: batches awaiting a single-footer assembly in `finish`.
    #[cfg(feature = "parquet")]
    pending:   Vec<RecordBatch>,
}

impl BatchExportWriter {
    /// Start a new document in `format`.
    #[must_use]
    pub const fn new(format: ExportFormat) -> Self {
        Self {
            format,
            wrote_any: false,
            #[cfg(feature = "parquet")]
            pending: Vec::new(),
        }
    }

    /// Serialise one batch, returning the bytes to append to the document.
    ///
    /// Returns an empty vector for formats that can only emit at [`Self::finish`].
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying encoder fails.
    pub fn write(&mut self, batch: &RecordBatch) -> Result<Vec<u8>, String> {
        match self.format {
            #[cfg(feature = "parquet")]
            ExportFormat::Parquet => {
                // Cheap: `RecordBatch` clone is Arc-based.
                self.pending.push(batch.clone());
                Ok(Vec::new())
            },
            ExportFormat::Csv => {
                let include_header = !self.wrote_any;
                self.wrote_any = true;
                BulkExporter::export_csv_with_header(batch, include_header)
            },
            ExportFormat::Json => {
                self.wrote_any = true;
                BulkExporter::export_json(batch)
            },
        }
    }

    /// Finish the document, returning any trailing bytes.
    ///
    /// Empty for the row-oriented formats, which are already complete.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying encoder fails.
    // Reason: const only in the default build, where every arm is a trivial `Ok`.
    // With the `parquet` feature the Parquet arm assembles a file, so the signature
    // cannot be const in every configuration.
    #[allow(clippy::missing_const_for_fn)]
    pub fn finish(self) -> Result<Vec<u8>, String> {
        match self.format {
            #[cfg(feature = "parquet")]
            ExportFormat::Parquet => BulkExporter::export_parquet_batches(&self.pending),
            #[cfg(feature = "parquet")]
            _ => Ok(Vec::new()),
            #[cfg(not(feature = "parquet"))]
            ExportFormat::Csv | ExportFormat::Json => Ok(Vec::new()),
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
