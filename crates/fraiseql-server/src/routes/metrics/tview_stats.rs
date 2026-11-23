//! The refresh counters of each TVIEW the schema reads, on `/metrics`.
//!
//! `tviews.stats` counts cumulatively, so each column is a Prometheus counter, labelled by
//! the TVIEW's table as the #1392 health gauges are, so the two series join on `tview`.

use std::fmt::Write as _;

use fraiseql_core::db::{TviewStats, TviewStatsReading};

/// One per-TVIEW counter.
type Counter = fn(&TviewStats) -> Option<f64>;

/// Append the counters of `reading`. `reads_a_tview` says whether the schema reads a TVIEW
/// (the health series found one): only then is an unavailable `tviews.stats` reported, as
/// `fraiseql_tview_stats_available 0`, and nothing else of it.
pub(super) fn append_tview_stats(
    output: &mut String,
    reading: &TviewStatsReading,
    reads_a_tview: bool,
) {
    let rows = match reading {
        TviewStatsReading::Read(rows) if reads_a_tview || !rows.is_empty() => rows,
        TviewStatsReading::Unavailable if reads_a_tview => {
            available(output, false);
            return;
        },
        TviewStatsReading::Read(_) | TviewStatsReading::Unavailable => return,
    };
    available(output, true);
    #[allow(clippy::cast_precision_loss)]
    // Reason: counts as Prometheus floats; exact up to 2^53 refreshes.
    let counters: [(&str, &str, Counter); 10] = [
        (
            "fraiseql_tview_view_recomputes_total",
            "Rows of a TVIEW the schema reads recomputed from its backing view",
            |s| s.view_recomputes.map(|n| n as f64),
        ),
        (
            "fraiseql_tview_noop_skipped_total",
            "Refreshes of a TVIEW the schema reads skipped because the row already held the result",
            |s| s.noop_skipped.map(|n| n as f64),
        ),
        (
            "fraiseql_tview_patch_captured_total",
            "Direct patches captured for a TVIEW the schema reads",
            |s| s.patch_captured.map(|n| n as f64),
        ),
        (
            "fraiseql_tview_patch_applied_total",
            "Direct patches applied to a TVIEW the schema reads",
            |s| s.patch_applied.map(|n| n as f64),
        ),
        (
            "fraiseql_tview_patch_fallbacks_total",
            "Direct patches of a TVIEW the schema reads that fell back to a recompute",
            |s| s.patch_fallbacks.map(|n| n as f64),
        ),
        (
            "fraiseql_tview_propagation_pruned_total",
            "Parent lookups skipped for a TVIEW the schema reads because the child row did not change",
            |s| s.propagation_pruned.map(|n| n as f64),
        ),
        (
            "fraiseql_tview_rows_written_total",
            "Rows of a TVIEW the schema reads inserted or updated by a refresh",
            |s| s.rows_written.map(|n| n as f64),
        ),
        (
            "fraiseql_tview_rows_deleted_total",
            "Rows of a TVIEW the schema reads deleted by a refresh",
            |s| s.rows_deleted.map(|n| n as f64),
        ),
        (
            "fraiseql_tview_full_refreshes_total",
            "Whole refreshes of a TVIEW the schema reads",
            |s| s.full_refreshes.map(|n| n as f64),
        ),
        (
            "fraiseql_tview_refresh_seconds_total",
            "Seconds spent refreshing a TVIEW the schema reads",
            |s| s.refresh_ms.map(|ms| ms / 1000.0),
        ),
    ];
    for (name, help, value) in &counters {
        series(output, name, help, "counter", rows, *value);
    }
    series(
        output,
        "fraiseql_tview_stats_reset_timestamp_seconds",
        "When the refresh counters of a TVIEW the schema reads started, in Unix seconds",
        "gauge",
        rows,
        |s| s.stats_reset,
    );
    series(
        output,
        "fraiseql_tview_stats_untracked",
        "1 when pg_tviews counts nothing for a TVIEW the schema reads (its statistics table was full)",
        "gauge",
        rows,
        |s| Some(if s.untracked { 1.0 } else { 0.0 }),
    );
}

/// `fraiseql_tview_stats_available`: whether the database serves `tviews.stats`.
fn available(output: &mut String, yes: bool) {
    let _ = write!(
        output,
        "\n# HELP fraiseql_tview_stats_available 1 when the database serves tviews.stats \
         (pg_tviews preloaded), 0 when the schema reads a TVIEW and it does not\n\
         # TYPE fraiseql_tview_stats_available gauge\nfraiseql_tview_stats_available {}\n",
        u8::from(yes)
    );
}

/// One series: a value per TVIEW that has one.
fn series(
    output: &mut String,
    name: &str,
    help: &str,
    kind: &str,
    rows: &[TviewStats],
    value: Counter,
) {
    let _ = write!(output, "\n# HELP {name} {help}\n# TYPE {name} {kind}\n");
    for row in rows {
        if let Some(v) = value(row) {
            let _ = writeln!(
                output,
                "{name}{{tview=\"{}\"}} {v}",
                super::escape_label_value(&row.tview)
            );
        }
    }
}

#[cfg(test)]
#[path = "tview_stats_tests.rs"]
mod tests;
