//! `fraiseql perf` — change-log performance observability (#392).
//!
//! The first consumer of the Change-Spine change-log contract. It reads
//! `core.v_entity_change_log` (the framework-owned outbox view) and turns it into
//! operator-facing forensics. `perf regression-scan` flags mutations whose
//! latency regressed between a baseline and a recent time window, per
//! `(object_type, modification_type)`. `perf explore` offers ad-hoc reads — the
//! slowest mutations, the `duration_ms` completeness rate, and per-operation
//! latency percentiles.
//!
//! Architecture: this crate builds the *capability* (read + analyze + a stable
//! machine-readable seam); the `fraisier` orchestrator schedules it on a cadence.
//! The DB [`reader`] is a thin PostgreSQL projection; all analysis lives in pure,
//! unit-tested functions so correctness is covered without a live database.

pub mod analysis;
pub mod reader;

#[cfg(test)]
mod tests;

use anyhow::Result;

use self::{
    analysis::{NullRateReport, NullRateRow, RegressionParams, RegressionReport, SkipReason},
    reader::{ChangeLogSample, PerfReader},
};
use crate::commands::migrate::resolve_database_url;

/// Arguments for `perf regression-scan`.
pub struct RegressionScanArgs {
    /// Explicit PostgreSQL URL; falls back to `fraiseql.toml` / `DATABASE_URL`.
    pub database:           Option<String>,
    /// Recent window size, in days.
    pub recent_days:        i64,
    /// Baseline window size (immediately before the recent window), in days.
    pub baseline_days:      i64,
    /// Minimum comparable samples required on each side.
    pub min_samples:        usize,
    /// Minimum `p50` increase to flag, in percent.
    pub threshold_pct:      f64,
    /// Minimum absolute `p50` increase to flag, in milliseconds.
    pub min_delta_ms:       f64,
    /// Restrict the scan to a single `object_type`.
    pub object_type:        Option<String>,
    /// Exit non-zero when any regression is found.
    pub fail_on_regression: bool,
    /// Emit machine-readable JSON instead of the human report.
    pub json:               bool,
    /// Suppress the human report (no effect in JSON mode).
    pub quiet:              bool,
}

/// Run `perf regression-scan`: read the change-log window, analyze it, render the
/// report, and report whether the process should exit successfully.
///
/// Returns `Ok(true)` for a normal exit (code 0) and `Ok(false)` only when
/// `--fail-on-regression` was passed *and* at least one regression was found.
/// A scan that finds regressions without that flag still exits 0 — it is a
/// report, not a gate.
///
/// # Errors
///
/// Returns an error if no database URL can be resolved or a query fails.
pub async fn run_regression_scan(args: RegressionScanArgs) -> Result<bool> {
    let url = resolve_database_url(args.database.as_deref())?;
    let reader = PerfReader::connect(&url)?;

    let now_epoch = reader.db_now_epoch().await?;
    let window_days = i32::try_from(args.recent_days + args.baseline_days).unwrap_or(i32::MAX);
    let samples = reader.load_samples(window_days, args.object_type.as_deref()).await?;

    let params = RegressionParams {
        recent_days:   args.recent_days,
        baseline_days: args.baseline_days,
        min_samples:   args.min_samples,
        threshold_pct: args.threshold_pct,
        min_delta_ms:  args.min_delta_ms,
    };
    let report = analysis::regression_scan(&samples, &params, now_epoch);

    render_regression(&report, args.json, args.quiet)?;

    let has_regressions = !report.findings.is_empty();
    Ok(!(args.fail_on_regression && has_regressions))
}

/// Render a regression report: pretty JSON in `--json` mode, otherwise the
/// human report with `WARN` / `SKIP` lines (a stable seam the `fraisier`
/// orchestrator greps).
fn render_regression(report: &RegressionReport, json: bool, quiet: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }
    if quiet {
        return Ok(());
    }

    let s = &report.summary;
    println!(
        "perf regression-scan — {} regression(s) across {} operation(s) · {} samples · {} \
         excluded (non-v2)",
        s.regressions, s.groups_analyzed, s.total_samples, s.excluded_samples
    );

    if report.findings.is_empty() && report.skipped.is_empty() {
        println!("no regressions detected");
        return Ok(());
    }

    for f in &report.findings {
        println!(
            "WARN  {}/{}  p50 {:.0}ms → {:.0}ms ({:+.1}%)  p95 {:.0}ms → {:.0}ms  [baseline={} \
             recent={}]",
            f.object_type,
            f.modification_type,
            f.baseline_p50,
            f.recent_p50,
            f.pct_change,
            f.baseline_p95,
            f.recent_p95,
            f.baseline_samples,
            f.recent_samples,
        );
    }
    for sk in &report.skipped {
        let reason = match sk.reason {
            SkipReason::InsufficientSamples => {
                format!(
                    "insufficient samples (baseline={} recent={})",
                    sk.baseline_samples, sk.recent_samples
                )
            },
            SkipReason::NoComparableV2Data => {
                format!("no comparable v2 data ({} excluded)", sk.excluded_samples)
            },
        };
        println!("SKIP  {}/{}  {reason}", sk.object_type, sk.modification_type);
    }
    Ok(())
}

/// Connect and load a trailing-`days` window of samples for an `explore` read.
async fn load(
    database: Option<&str>,
    days: i32,
    object_type: Option<&str>,
) -> Result<Vec<ChangeLogSample>> {
    let url = resolve_database_url(database)?;
    let reader = PerfReader::connect(&url)?;
    reader.load_samples(days, object_type).await
}

/// Run `perf explore slowest`: the `limit` slowest comparable mutations.
///
/// # Errors
///
/// Returns an error if no database URL can be resolved or a query fails.
pub async fn run_explore_slowest(
    database: Option<String>,
    days: i32,
    object_type: Option<String>,
    limit: usize,
    json: bool,
    quiet: bool,
) -> Result<()> {
    let samples = load(database.as_deref(), days, object_type.as_deref()).await?;
    let rows = analysis::slowest(&samples, limit);

    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
    } else if !quiet {
        if rows.is_empty() {
            println!("perf explore slowest — no comparable (v2) mutations in the window");
        } else {
            println!("perf explore slowest — top {} by duration", rows.len());
            for (i, r) in rows.iter().enumerate() {
                println!(
                    "{:>3}. {:>7}ms  {}/{}  {}  {}",
                    i + 1,
                    r.duration_ms,
                    r.object_type,
                    r.modification_type,
                    r.object_id.as_deref().unwrap_or("-"),
                    r.trace_id.as_deref().unwrap_or("-"),
                );
            }
        }
    }
    Ok(())
}

/// Run `perf explore null-rate`: `duration_ms` completeness, per operation.
///
/// # Errors
///
/// Returns an error if no database URL can be resolved or a query fails.
pub async fn run_explore_null_rate(
    database: Option<String>,
    days: i32,
    object_type: Option<String>,
    json: bool,
    quiet: bool,
) -> Result<()> {
    let samples = load(database.as_deref(), days, object_type.as_deref()).await?;
    let report = analysis::null_rate(&samples);

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if !quiet {
        render_null_rate(&report);
    }
    Ok(())
}

/// Run `perf explore summary`: latency percentiles, per operation.
///
/// # Errors
///
/// Returns an error if no database URL can be resolved or a query fails.
pub async fn run_explore_summary(
    database: Option<String>,
    days: i32,
    object_type: Option<String>,
    json: bool,
    quiet: bool,
) -> Result<()> {
    let samples = load(database.as_deref(), days, object_type.as_deref()).await?;
    let rows = analysis::summary(&samples);

    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
    } else if !quiet {
        if rows.is_empty() {
            println!("perf explore summary — no comparable (v2) mutations in the window");
        } else {
            println!("perf explore summary — latency percentiles (ms), slowest p95 first");
            for r in &rows {
                println!(
                    "{}/{}  n={}  p50={:.0}  p95={:.0}  p99={:.0}  max={:.0}",
                    r.object_type, r.modification_type, r.count, r.p50, r.p95, r.p99, r.max,
                );
            }
        }
    }
    Ok(())
}

fn render_null_rate(report: &NullRateReport) {
    println!("perf explore null-rate — duration_ms completeness");
    for row in &report.rows {
        print_null_rate_row(row);
    }
    println!("---");
    print_null_rate_row(&report.overall);
}

fn print_null_rate_row(r: &NullRateRow) {
    println!(
        "{}/{}  total={} null={} non_v2={} null_rate={:.1}%",
        r.object_type, r.modification_type, r.total, r.null_duration, r.non_v2, r.null_rate_pct,
    );
}
