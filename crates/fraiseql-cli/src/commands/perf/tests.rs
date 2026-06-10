//! Golden unit tests for the pure `perf` analysis — the CI-covered correctness
//! gate (no Dagger leg runs the CLI against a live DB).

#![allow(clippy::unwrap_used)] // Reason: test code — fixtures are total, a panic is an acceptable failure

use super::{
    analysis::{
        RegressionParams, SkipReason, null_rate, percentile, percentile_sorted, regression_scan,
        slowest, summary,
    },
    reader::ChangeLogSample,
};

const NOW: f64 = 1_000_000_000.0;
const DAY: f64 = 86_400.0;
/// Falls inside the recent window `[NOW-7d, NOW]`.
const RECENT_EPOCH: f64 = NOW - DAY;
/// Falls inside the baseline window `[NOW-14d, NOW-7d)`.
const BASELINE_EPOCH: f64 = NOW - 10.0 * DAY;

fn params() -> RegressionParams {
    RegressionParams {
        recent_days:   7,
        baseline_days: 7,
        min_samples:   5,
        threshold_pct: 25.0,
        min_delta_ms:  5.0,
    }
}

fn sample(
    object_type: &str,
    modification_type: &str,
    duration_ms: Option<i32>,
    version: Option<i64>,
    epoch: f64,
) -> ChangeLogSample {
    ChangeLogSample {
        object_type: object_type.to_string(),
        modification_type: modification_type.to_string(),
        duration_ms,
        duration_calc_version: version,
        created_at_epoch: epoch,
        object_id: None,
        trace_id: None,
    }
}

/// `n` comparable (v2) rows of a fixed duration at one epoch.
fn many(
    object_type: &str,
    modification_type: &str,
    duration_ms: i32,
    epoch: f64,
    n: usize,
) -> Vec<ChangeLogSample> {
    (0..n)
        .map(|_| sample(object_type, modification_type, Some(duration_ms), Some(2), epoch))
        .collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn stable_latency_produces_no_finding() {
    let mut s = many("User", "UPDATE", 100, BASELINE_EPOCH, 10);
    s.extend(many("User", "UPDATE", 105, RECENT_EPOCH, 10)); // +5% < 25%

    let report = regression_scan(&s, &params(), NOW);

    assert!(report.findings.is_empty(), "5% drift is below threshold");
    assert!(report.skipped.is_empty(), "enough samples on both sides");
    assert_eq!(report.summary.groups_analyzed, 1);
    assert_eq!(report.summary.regressions, 0);
}

#[test]
fn real_regression_is_flagged() {
    let mut s = many("User", "UPDATE", 100, BASELINE_EPOCH, 10);
    s.extend(many("User", "UPDATE", 150, RECENT_EPOCH, 10)); // +50%

    let report = regression_scan(&s, &params(), NOW);

    assert_eq!(report.findings.len(), 1);
    let f = &report.findings[0];
    assert_eq!(f.object_type, "User");
    assert_eq!(f.modification_type, "UPDATE");
    assert!(close(f.baseline_p50, 100.0), "baseline p50 {}", f.baseline_p50);
    assert!(close(f.recent_p50, 150.0), "recent p50 {}", f.recent_p50);
    assert!(close(f.pct_change, 50.0), "pct {}", f.pct_change);
    assert_eq!(f.baseline_samples, 10);
    assert_eq!(f.recent_samples, 10);
    assert_eq!(report.summary.regressions, 1);
}

#[test]
fn thin_recent_window_is_skipped_not_flagged() {
    let mut s = many("User", "UPDATE", 100, BASELINE_EPOCH, 10);
    s.extend(many("User", "UPDATE", 300, RECENT_EPOCH, 2)); // only 2 recent (< 5)

    let report = regression_scan(&s, &params(), NOW);

    assert!(report.findings.is_empty(), "must not flag on thin data");
    assert_eq!(report.skipped.len(), 1);
    let sk = &report.skipped[0];
    assert_eq!(sk.reason, SkipReason::InsufficientSamples);
    assert_eq!(sk.recent_samples, 2);
    assert_eq!(sk.baseline_samples, 10);
}

#[test]
fn pre_fix_rows_are_gated_out_and_group_skipped() {
    // All rows carry the legacy marker (version 1) → none are comparable.
    let mut s: Vec<ChangeLogSample> = (0..10)
        .map(|_| sample("User", "UPDATE", Some(100), Some(1), BASELINE_EPOCH))
        .collect();
    s.extend((0..10).map(|_| sample("User", "UPDATE", Some(900), Some(1), RECENT_EPOCH)));

    let report = regression_scan(&s, &params(), NOW);

    assert!(report.findings.is_empty(), "pre-fix data must never produce a finding");
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].reason, SkipReason::NoComparableV2Data);
    assert_eq!(report.skipped[0].excluded_samples, 20);
    assert_eq!(report.summary.excluded_samples, 20);
}

#[test]
fn null_durations_are_excluded() {
    let mut s = many("User", "UPDATE", 100, BASELINE_EPOCH, 10);
    s.extend(many("User", "UPDATE", 150, RECENT_EPOCH, 10));
    // Five cooperative-producer rows with NULL duration are dropped, not counted.
    s.extend((0..5).map(|_| sample("User", "UPDATE", None, None, RECENT_EPOCH)));

    let report = regression_scan(&s, &params(), NOW);

    assert_eq!(report.findings.len(), 1, "the v2 rows still regress");
    assert_eq!(report.summary.excluded_samples, 5, "NULL-duration rows are excluded");
}

#[test]
fn noise_below_absolute_floor_is_not_flagged() {
    // +100% relative, but only +1ms absolute — below min_delta_ms (5).
    let mut s = many("Token", "UPDATE", 1, BASELINE_EPOCH, 10);
    s.extend(many("Token", "UPDATE", 2, RECENT_EPOCH, 10));

    let report = regression_scan(&s, &params(), NOW);

    assert!(report.findings.is_empty(), "1ms→2ms is noise, not a regression");
    assert!(report.skipped.is_empty(), "analyzed, just not flagged");
    assert_eq!(report.summary.groups_analyzed, 1);
}

#[test]
fn modification_type_split_defeats_false_improvement() {
    // The mix shifts from expensive UPDATEs to cheap DELETEs between windows,
    // and each operation gets *slower*. A naive object_type-only aggregate would
    // look faster; the per-(type) scan must flag both.
    let mut s = Vec::new();
    // Baseline: 20 expensive UPDATEs + 5 cheap DELETEs.
    s.extend(many("Order", "UPDATE", 100, BASELINE_EPOCH, 20));
    s.extend(many("Order", "DELETE", 10, BASELINE_EPOCH, 5));
    // Recent: 5 even-more-expensive UPDATEs + 20 slower DELETEs.
    s.extend(many("Order", "UPDATE", 200, RECENT_EPOCH, 5));
    s.extend(many("Order", "DELETE", 20, RECENT_EPOCH, 20));

    let report = regression_scan(&s, &params(), NOW);

    // Both operations are correctly flagged as regressed.
    assert_eq!(
        report.findings.len(),
        2,
        "both UPDATE and DELETE regressed: {:?}",
        report.findings
    );
    assert!(report.findings.iter().any(|f| f.modification_type == "UPDATE"));
    assert!(report.findings.iter().any(|f| f.modification_type == "DELETE"));

    // Demonstrate the trap the split avoids: the object_type-only median
    // *improves* (100ms → 20ms) because the operation mix shifted.
    let mut baseline_all: Vec<f64> = s
        .iter()
        .filter(|x| close(x.created_at_epoch, BASELINE_EPOCH))
        .map(|x| f64::from(x.duration_ms.unwrap()))
        .collect();
    let mut recent_all: Vec<f64> = s
        .iter()
        .filter(|x| close(x.created_at_epoch, RECENT_EPOCH))
        .map(|x| f64::from(x.duration_ms.unwrap()))
        .collect();
    let naive_baseline = percentile(&mut baseline_all, 50.0);
    let naive_recent = percentile(&mut recent_all, 50.0);
    assert!(
        naive_recent < naive_baseline,
        "the naive aggregate falsely improves ({naive_baseline} → {naive_recent}) — exactly what the split guards against"
    );
}

#[test]
fn percentile_interpolates_like_numpy() {
    assert!(close(percentile_sorted(&[1.0, 2.0, 3.0], 50.0), 2.0));
    assert!(close(percentile_sorted(&[1.0, 2.0, 3.0, 4.0], 50.0), 2.5));
    assert!(close(percentile_sorted(&[10.0, 20.0, 30.0], 0.0), 10.0));
    assert!(close(percentile_sorted(&[10.0, 20.0, 30.0], 100.0), 30.0));
    assert!(close(percentile_sorted(&[42.0], 95.0), 42.0));
    assert!(close(percentile_sorted(&[], 50.0), 0.0));
}

#[test]
fn slowest_ranks_v2_rows_descending_and_truncates() {
    let s = vec![
        sample("A", "UPDATE", Some(10), Some(2), RECENT_EPOCH),
        sample("B", "INSERT", Some(500), Some(2), RECENT_EPOCH),
        sample("C", "DELETE", Some(100), Some(2), RECENT_EPOCH),
        sample("D", "UPDATE", Some(999), Some(1), RECENT_EPOCH), // v1 → excluded
        sample("E", "UPDATE", None, None, RECENT_EPOCH),         // NULL → excluded
    ];

    let top = slowest(&s, 2);

    assert_eq!(top.len(), 2, "limit truncates");
    assert_eq!(top[0].duration_ms, 500);
    assert_eq!(top[0].object_type, "B");
    assert_eq!(top[1].duration_ms, 100);
    assert!(top.iter().all(|r| r.object_type != "D"), "pre-fix row is excluded from ranking");
}

#[test]
fn null_rate_partitions_total_into_null_and_non_v2() {
    let s = vec![
        sample("U", "UPDATE", Some(10), Some(2), RECENT_EPOCH), // comparable
        sample("U", "UPDATE", Some(20), Some(2), RECENT_EPOCH), // comparable
        sample("U", "UPDATE", None, None, RECENT_EPOCH),        // NULL duration
        sample("U", "UPDATE", Some(30), Some(1), RECENT_EPOCH), // present but pre-fix
        sample("U", "UPDATE", Some(40), None, RECENT_EPOCH),    // present but unmarked
    ];

    let report = null_rate(&s);

    assert_eq!(report.overall.total, 5);
    assert_eq!(report.overall.null_duration, 1);
    assert_eq!(report.overall.non_v2, 2);
    assert!(close(report.overall.null_rate_pct, 20.0));
    assert_eq!(report.rows.len(), 1, "one operation group");
    assert_eq!(report.overall.object_type, "*");
}

#[test]
fn summary_computes_percentiles_over_v2_rows_only() {
    let mut s: Vec<ChangeLogSample> = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100]
        .iter()
        .map(|&d| sample("Q", "UPDATE", Some(d), Some(2), RECENT_EPOCH))
        .collect();
    s.push(sample("Q", "UPDATE", Some(9999), Some(1), RECENT_EPOCH)); // v1 → excluded

    let out = summary(&s);

    assert_eq!(out.len(), 1);
    let o = &out[0];
    assert_eq!(o.count, 10, "the pre-fix outlier is excluded");
    assert!(close(o.p50, 55.0), "p50 {}", o.p50);
    assert!(close(o.max, 100.0), "max {}", o.max);
    assert!(o.p99 > o.p95 && o.p95 > o.p50, "percentiles are monotonic");
}

#[test]
fn empty_input_is_an_empty_report() {
    let report = regression_scan(&[], &params(), NOW);
    assert!(report.findings.is_empty());
    assert!(report.skipped.is_empty());
    assert_eq!(report.summary.groups_analyzed, 0);
    assert_eq!(report.summary.total_samples, 0);
}
