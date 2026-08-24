#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
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

/// Reassemble a multi-batch export the way a Flight consumer must: the stream
/// carries one message per batch and the export is their concatenation.
fn concat_export(batches: &[RecordBatch], format: ExportFormat) -> Vec<u8> {
    BulkExporter::export_batches(batches, format).unwrap().concat()
}

#[test]
fn test_export_format_from_str() {
    assert_eq!(ExportFormat::from_str("csv").unwrap(), ExportFormat::Csv);
    assert_eq!(ExportFormat::from_str("json").unwrap(), ExportFormat::Json);

    #[cfg(feature = "parquet")]
    {
        assert_eq!(ExportFormat::from_str("parquet").unwrap(), ExportFormat::Parquet);
        // Case-insensitive
        assert_eq!(ExportFormat::from_str("PARQUET").unwrap(), ExportFormat::Parquet);
    }

    #[cfg(not(feature = "parquet"))]
    {
        // Without the `parquet` feature, "parquet" should yield a feature-gated error.
        let err = ExportFormat::from_str("parquet")
            .expect_err("expected Err for parquet without feature");
        assert!(err.contains("parquet"), "unexpected error: {err}");
    }

    // Invalid format
    assert!(
        ExportFormat::from_str("invalid").is_err(),
        "expected Err for unrecognised format string"
    );
}

#[test]
fn test_export_format_extension() {
    assert_eq!(ExportFormat::Csv.extension(), "csv");
    assert_eq!(ExportFormat::Json.extension(), "jsonl");
    #[cfg(feature = "parquet")]
    assert_eq!(ExportFormat::Parquet.extension(), "parquet");
}

#[test]
fn test_export_format_mime_type() {
    assert_eq!(ExportFormat::Csv.mime_type(), "text/csv");
    assert_eq!(ExportFormat::Json.mime_type(), "application/x-ndjson");
    #[cfg(feature = "parquet")]
    assert_eq!(ExportFormat::Parquet.mime_type(), "application/octet-stream");
}

#[test]
fn test_export_csv() {
    let batch = create_test_batch();
    let exported = BulkExporter::export_batch(&batch, ExportFormat::Csv);

    let bytes = exported.unwrap_or_else(|e| panic!("expected Ok for CSV export: {e}"));
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

    let bytes = exported.unwrap_or_else(|e| panic!("expected Ok for JSON export: {e}"));
    assert!(!bytes.is_empty());

    // JSON Lines should contain JSON objects
    let json_str = String::from_utf8(bytes).unwrap();
    assert!(json_str.contains("\"name\""));
    assert!(json_str.contains("\"age\""));
    assert!(json_str.contains("Alice"));
}

#[cfg(feature = "parquet")]
#[test]
fn test_export_parquet() {
    let batch = create_test_batch();
    let exported = BulkExporter::export_batch(&batch, ExportFormat::Parquet);

    let bytes = exported.unwrap_or_else(|e| panic!("expected Ok for Parquet export: {e}"));
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

    let csv = BulkExporter::export_batch(&batch, ExportFormat::Csv);
    let json = BulkExporter::export_batch(&batch, ExportFormat::Json);

    csv.unwrap_or_else(|e| panic!("expected Ok for empty-batch CSV export: {e}"));
    json.unwrap_or_else(|e| panic!("expected Ok for empty-batch JSON export: {e}"));

    #[cfg(feature = "parquet")]
    {
        let parquet = BulkExporter::export_batch(&batch, ExportFormat::Parquet);
        parquet.unwrap_or_else(|e| panic!("expected Ok for empty-batch Parquet export: {e}"));
    }
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

#[cfg(feature = "parquet")]
#[test]
fn test_export_format_parse_trait_mixed_case_parquet() {
    let fmt: ExportFormat = "Parquet".parse().unwrap();
    assert_eq!(fmt, ExportFormat::Parquet);
}

#[test]
fn test_export_format_parse_unknown_returns_err() {
    let result: Result<ExportFormat, _> = "avro".parse();
    let err = result.expect_err("expected Err for unknown format 'avro'");
    assert!(err.contains("Unsupported export format"), "unexpected error message: {err}");
}

#[test]
fn test_export_format_clone_and_eq() {
    let fmt = ExportFormat::Csv;
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

#[cfg(feature = "parquet")]
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
    let non_empty_lines: Vec<&str> = json_str.lines().filter(|l| !l.trim().is_empty()).collect();
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

/// #1036 — a multi-batch export must be ONE document, not N concatenated ones.
///
/// `execute_bulk_export` chunks rows at `batch_size: 10_000` and calls
/// `export_batch` once per chunk, putting each result on the same Flight stream.
/// Each call builds a fresh `arrow::csv::Writer`, which emits the header on its
/// first write — so an export of 10 001 rows carries two header rows, and a
/// consumer that concatenates the stream reads `name,age` as a data row.
///
/// Every other test in this file exports a single 3-row batch, which is exactly
/// why this stayed invisible: the defect cannot appear below the batch size.
#[test]
fn multi_batch_csv_export_carries_exactly_one_header() {
    let batches = [create_test_batch(), create_test_batch()];

    let document: Vec<u8> = concat_export(&batches, ExportFormat::Csv);

    let text = String::from_utf8(document).unwrap();
    let header_rows = text.lines().filter(|line| line.starts_with("name,age")).count();
    assert_eq!(
        header_rows, 1,
        "a multi-batch CSV export must carry exactly one header row, got {header_rows}:\n{text}"
    );
}

/// The row payload must survive the multi-batch path intact — one header, but
/// still every data row from every batch.
#[test]
fn multi_batch_csv_export_keeps_every_data_row() {
    let batches = [create_test_batch(), create_test_batch()];

    let text = String::from_utf8(concat_export(&batches, ExportFormat::Csv)).unwrap();

    let alice_rows = text.lines().filter(|line| line.starts_with("Alice,")).count();
    assert_eq!(alice_rows, 2, "both batches' rows must appear:\n{text}");
}

/// A multi-batch Parquet export must be ONE file.
///
/// Every Parquet file opens and closes with the `PAR1` magic, so a single file
/// contains exactly two markers. Exporting each batch independently produced two
/// complete files — four markers — whose concatenation a reader either rejects or
/// silently reads as only the last file.
///
/// ⚠ No CI leg enables the `parquet` feature, so this test compiles under
/// preflight's `--all-features` but is never executed by a gate.
#[cfg(feature = "parquet")]
#[test]
fn multi_batch_parquet_export_is_a_single_file() {
    let batches = [create_test_batch(), create_test_batch()];

    let document = concat_export(&batches, ExportFormat::Parquet);

    let magic_markers = document.windows(4).filter(|w| *w == b"PAR1".as_slice()).count();
    assert_eq!(
        magic_markers, 2,
        "one Parquet file carries exactly two PAR1 markers (header + footer), got {magic_markers}"
    );
}

/// JSON Lines has no document wrapper, so concatenation was already correct —
/// pin that so the #1036 fix does not regress it into a single-batch export.
#[test]
fn multi_batch_json_export_keeps_every_data_row() {
    let batches = [create_test_batch(), create_test_batch()];

    let text = String::from_utf8(concat_export(&batches, ExportFormat::Json)).unwrap();

    let lines = text.lines().filter(|l| !l.trim().is_empty()).count();
    assert_eq!(lines, 6, "two 3-row batches must yield six NDJSON lines:\n{text}");
}
