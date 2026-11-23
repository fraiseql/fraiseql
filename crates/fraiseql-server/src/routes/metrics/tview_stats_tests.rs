//! How `/metrics` renders `tviews.stats`.

use fraiseql_core::db::{TviewStats, TviewStatsReading};

use super::append_tview_stats;

fn counted(tview: &str) -> TviewStats {
    TviewStats {
        tview:              tview.to_string(),
        view_recomputes:    Some(1),
        noop_skipped:       Some(2),
        patch_captured:     Some(3),
        patch_applied:      Some(4),
        patch_fallbacks:    Some(5),
        propagation_pruned: Some(6),
        rows_written:       Some(7),
        rows_deleted:       Some(8),
        full_refreshes:     Some(9),
        refresh_ms:         Some(1500.0),
        stats_reset:        Some(1_700_000_000.0),
        untracked:          false,
    }
}

fn render(reading: &TviewStatsReading, reads_a_tview: bool) -> String {
    let mut output = String::new();
    append_tview_stats(&mut output, reading, reads_a_tview);
    output
}

#[test]
fn every_column_is_a_counter_labelled_by_its_tview() {
    let out = render(&TviewStatsReading::Read(vec![counted("app.tv_post")]), true);
    for (metric, value) in [
        ("fraiseql_tview_view_recomputes_total", "1"),
        ("fraiseql_tview_noop_skipped_total", "2"),
        ("fraiseql_tview_patch_captured_total", "3"),
        ("fraiseql_tview_patch_applied_total", "4"),
        ("fraiseql_tview_patch_fallbacks_total", "5"),
        ("fraiseql_tview_propagation_pruned_total", "6"),
        ("fraiseql_tview_rows_written_total", "7"),
        ("fraiseql_tview_rows_deleted_total", "8"),
        ("fraiseql_tview_full_refreshes_total", "9"),
    ] {
        assert!(out.contains(&format!("# TYPE {metric} counter\n")), "{metric}: {out}");
        assert!(out.contains(&format!("{metric}{{tview=\"app.tv_post\"}} {value}\n")), "{out}");
    }
    assert!(out.contains("fraiseql_tview_stats_available 1\n"), "{out}");
    assert!(
        out.contains("fraiseql_tview_stats_untracked{tview=\"app.tv_post\"} 0\n"),
        "{out}"
    );
    assert!(
        out.contains(
            "fraiseql_tview_stats_reset_timestamp_seconds{tview=\"app.tv_post\"} 1700000000\n"
        ),
        "{out}"
    );
}

#[test]
fn refresh_time_is_reported_in_seconds() {
    let out = render(&TviewStatsReading::Read(vec![counted("app.tv_post")]), true);
    assert!(
        out.contains("fraiseql_tview_refresh_seconds_total{tview=\"app.tv_post\"} 1.5\n"),
        "{out}"
    );
}

#[test]
fn an_untracked_tview_reports_only_that_it_is_untracked() {
    let untracked = TviewStats {
        tview:              "app.tv_post".to_string(),
        view_recomputes:    None,
        noop_skipped:       None,
        patch_captured:     None,
        patch_applied:      None,
        patch_fallbacks:    None,
        propagation_pruned: None,
        rows_written:       None,
        rows_deleted:       None,
        full_refreshes:     None,
        refresh_ms:         None,
        stats_reset:        None,
        untracked:          true,
    };
    let out = render(&TviewStatsReading::Read(vec![untracked]), true);
    let samples: Vec<&str> = out.lines().filter(|l| l.contains("{tview=")).collect();
    assert_eq!(samples, ["fraiseql_tview_stats_untracked{tview=\"app.tv_post\"} 1"], "{out}");
}

#[test]
fn an_unavailable_view_is_reported_only_when_the_schema_reads_a_tview() {
    let out = render(&TviewStatsReading::Unavailable, true);
    assert!(out.contains("fraiseql_tview_stats_available 0\n"), "{out}");
    assert!(!out.contains("{tview="), "no counter without the view: {out}");

    assert_eq!(render(&TviewStatsReading::Unavailable, false), "", "no TVIEW read, nothing");
    assert_eq!(
        render(&TviewStatsReading::Read(Vec::new()), false),
        "",
        "no TVIEW read, nothing"
    );
}

#[test]
fn a_label_is_escaped() {
    let out = render(&TviewStatsReading::Read(vec![counted("app.\"tv\\x\ny\"")]), true);
    assert!(
        out.contains("fraiseql_tview_rows_written_total{tview=\"app.\\\"tv\\\\x\\ny\\\"\"} 7\n"),
        "{out}"
    );
}
