//! Configuration for REST export response formats (CSV, XLSX, Parquet).
//!
//! Export is a runtime concern (response serialization), so it lives in the
//! server crate rather than in `fraiseql-core`'s compilation schema. See
//! `.phases/2026-05-20-sprint/03-export-formats-269/phase-01-streaming-abstraction.md`
//! for the design rationale and the layering rule it enforces.

use std::path::PathBuf;

use serde::Deserialize;

/// User-selectable export response format.
///
/// Distinct from `fraiseql_arrow::ExportFormat`: this is the server-side
/// HTTP content-negotiation enum, not the Arrow exporter's format set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ExportFormat {
    /// RFC 4180 CSV.
    Csv,
    /// Office Open XML spreadsheet (`.xlsx`).
    Xlsx,
    /// Apache Parquet columnar file.
    Parquet,
}

/// Runtime configuration for REST export endpoints.
///
/// Deserialized from the server's TOML config (typically under a
/// `[rest.export]` table). All fields have defaults so an empty TOML table
/// yields a usable config.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ExportConfig {
    /// CSV field delimiter (default `,`).
    pub csv_delimiter:       char,
    /// Emit UTF-8 BOM at start of CSV output (default `true` — Excel needs it).
    pub csv_include_bom:     bool,
    /// Hard cap on rows per XLSX export (default `100_000`).
    pub xlsx_max_rows:       u64,
    /// Hard cap on rows per Parquet export (default `1_000_000`).
    pub parquet_max_rows:    u64,
    /// Override for the XLSX temp-file directory. `None` uses the system temp dir.
    pub xlsx_temp_dir:       Option<PathBuf>,
    /// Max simultaneous in-flight XLSX exports (default 10).
    pub max_concurrent_xlsx: usize,
    /// Formats the server is willing to serve. Empty disables all exports.
    pub export_formats:      Vec<ExportFormat>,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            csv_delimiter:       ',',
            csv_include_bom:     true,
            xlsx_max_rows:       100_000,
            parquet_max_rows:    1_000_000,
            xlsx_temp_dir:       None,
            max_concurrent_xlsx: 10,
            export_formats:      Vec::new(),
        }
    }
}
