//! Configuration for REST export response formats (CSV, XLSX, Parquet).
//!
//! Export is a runtime concern (response serialization), so it lives in the
//! server crate rather than in `fraiseql-core`'s compilation schema. See
//! `.phases/2026-05-20-sprint/03-export-formats-269/phase-01-streaming-abstraction.md`
//! for the design rationale and the layering rule it enforces.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// User-selectable export response format.
///
/// Distinct from `fraiseql_arrow::ExportFormat`: this is the server-side
/// HTTP content-negotiation enum, not the Arrow exporter's format set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
/// Deserialized from the server's TOML config under `[export]` in `fraiseql.toml`, and
/// reachable as [`ServerConfig::export`](crate::server_config::ServerConfig::export).
/// All fields have defaults, so an absent table yields a usable config.
///
/// **This type had no deserialization site at all until #917.** Every production
/// consumer called `ExportConfig::default()` — one of them with a comment conceding
/// that "TOML-driven `ExportConfig` loading is a later phase" — so all seven fields
/// were inert: a configured CSV delimiter, BOM setting, row cap, temp directory,
/// concurrency limit and format allow-list were each accepted by the config parser and
/// then ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Formats the server is willing to serve.
    ///
    /// An **explicitly empty** list disables all exports; that is the documented
    /// kill-switch and it is honoured by [`ExportConfig::serves`].
    ///
    /// ⚠ The default is therefore *all* formats, not the empty vector. It used to be
    /// empty, which was harmless only for as long as nothing read the field: wiring the
    /// kill-switch up without changing the default would have turned every export off in
    /// every deployment that had not written the key — a silent outage delivered by a
    /// bug fix. "Not configured" and "configured to serve nothing" have to be different
    /// values, and only the second may disable anything.
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
            export_formats:      vec![ExportFormat::Csv, ExportFormat::Xlsx, ExportFormat::Parquet],
        }
    }
}

impl ExportConfig {
    /// Whether the server is configured to serve `format`.
    ///
    /// The one reader of [`export_formats`](ExportConfig::export_formats), so the
    /// kill-switch cannot be honoured on one negotiation path and forgotten on another.
    #[must_use]
    pub fn serves(&self, format: ExportFormat) -> bool {
        self.export_formats.contains(&format)
    }
}
