//! Pure analysis over change-log samples — no database, no clock.
//!
//! Every function takes an in-memory `&[ChangeLogSample]` (and, for windowed
//! analyses, an explicit `now_epoch` reference) so correctness is covered by
//! ordinary unit tests; the live [`super::reader`] only moves rows across the
//! wire. This is the layer that holds the two #392 correctness guards:
//!
//! 1. **`duration_calc_version` gating.** Only rows marked with the current
//!    [`fraiseql_db::changelog::DURATION_CALC_VERSION`] enter the duration math, so pre-fix
//!    `EXTRACT(MILLISECONDS)` rows are never averaged against the wall-clock-correct ones.
//! 2. **`(object_type, modification_type)` split.** Latency is *never* aggregated across
//!    modification types. A shift in the operation mix (more cheap `DELETE`s, fewer expensive
//!    `UPDATE`s) can make an `object_type`-only aggregate look *faster* while every individual
//!    operation regressed — the split makes that false improvement impossible.

use std::collections::BTreeMap;

use fraiseql_db::changelog::DURATION_CALC_VERSION;
use serde::Serialize;

use super::reader::ChangeLogSample;

/// Seconds in a day — the window unit.
const DAY_SECS: f64 = 86_400.0;

/// Tunable parameters for [`regression_scan`].
#[derive(Debug, Clone)]
pub struct RegressionParams {
    /// The recent window spans the trailing `recent_days`.
    pub recent_days:   i64,
    /// The baseline window spans the `baseline_days` immediately before that.
    pub baseline_days: i64,
    /// Minimum comparable samples required on *each* side, else the group is
    /// skipped rather than flagged on thin data.
    pub min_samples:   usize,
    /// A regression needs at least this `p50` increase, in percent.
    pub threshold_pct: f64,
    /// …and at least this absolute `p50` increase, in milliseconds — the floor
    /// suppresses noise on already-fast operations (e.g. 1ms → 2ms).
    pub min_delta_ms:  f64,
}

/// Why a `(object_type, modification_type)` group was not evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// Fewer than `min_samples` comparable rows on at least one side.
    InsufficientSamples,
    /// No rows carrying the current `duration_calc_version` on either side.
    NoComparableV2Data,
}

/// A flagged latency regression for one operation.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RegressionFinding {
    /// Entity type (`object_type`).
    pub object_type:       String,
    /// `INSERT` / `UPDATE` / `DELETE` / `CUSTOM`.
    pub modification_type: String,
    /// Median latency over the baseline window, in milliseconds.
    pub baseline_p50:      f64,
    /// 95th-percentile latency over the baseline window, in milliseconds.
    pub baseline_p95:      f64,
    /// Median latency over the recent window, in milliseconds.
    pub recent_p50:        f64,
    /// 95th-percentile latency over the recent window, in milliseconds.
    pub recent_p95:        f64,
    /// `(recent_p50 / max(baseline_p50, 1ms) - 1) * 100`.
    pub pct_change:        f64,
    /// Comparable rows on the baseline side.
    pub baseline_samples:  usize,
    /// Comparable rows on the recent side.
    pub recent_samples:    usize,
}

/// A group that was skipped, with the reason and the counts behind it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SkippedGroup {
    /// Entity type (`object_type`).
    pub object_type:       String,
    /// `INSERT` / `UPDATE` / `DELETE` / `CUSTOM`.
    pub modification_type: String,
    /// Why the group was not evaluated.
    pub reason:            SkipReason,
    /// Comparable rows on the baseline side.
    pub baseline_samples:  usize,
    /// Comparable rows on the recent side.
    pub recent_samples:    usize,
    /// Rows dropped by the `duration_calc_version` gate (NULL duration or a
    /// missing / pre-fix marker).
    pub excluded_samples:  usize,
}

/// Roll-up counts for the whole scan — the stable seam summary.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScanSummary {
    /// Distinct `(object_type, modification_type)` groups seen.
    pub groups_analyzed:  usize,
    /// Number of groups flagged as regressed (`findings.len()`).
    pub regressions:      usize,
    /// Total rows scanned (before gating).
    pub total_samples:    usize,
    /// Rows dropped by the `duration_calc_version` gate across all groups.
    pub excluded_samples: usize,
}

/// The full regression-scan result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RegressionReport {
    /// Operations whose latency regressed past both thresholds.
    pub findings: Vec<RegressionFinding>,
    /// Groups that could not be evaluated, with the reason.
    pub skipped:  Vec<SkippedGroup>,
    /// Roll-up counts.
    pub summary:  ScanSummary,
}

/// Scan `samples` for per-operation latency regressions between a baseline and a
/// recent window, both measured back from `now_epoch` (the database clock, so
/// there is no app↔DB skew).
///
/// Findings and skips are returned sorted by `(object_type, modification_type)`
/// for deterministic output.
#[must_use]
#[allow(clippy::cast_precision_loss)] // Reason: window day counts are tiny; i64→f64 is lossless here
pub fn regression_scan(
    samples: &[ChangeLogSample],
    params: &RegressionParams,
    now_epoch: f64,
) -> RegressionReport {
    let recent_start = (params.recent_days as f64).mul_add(-DAY_SECS, now_epoch);
    let baseline_start =
        ((params.recent_days + params.baseline_days) as f64).mul_add(-DAY_SECS, now_epoch);

    // Group by the operation pair — the split that defeats the false-improvement
    // hazard. BTreeMap keeps the output deterministic.
    let mut groups: BTreeMap<(&str, &str), Vec<&ChangeLogSample>> = BTreeMap::new();
    for s in samples {
        groups
            .entry((s.object_type.as_str(), s.modification_type.as_str()))
            .or_default()
            .push(s);
    }

    let groups_analyzed = groups.len();
    let mut findings = Vec::new();
    let mut skipped = Vec::new();
    let mut total_excluded = 0usize;

    for ((object_type, modification_type), rows) in groups {
        let mut baseline = Vec::new();
        let mut recent = Vec::new();
        let mut excluded = 0usize;

        for s in rows {
            // duration_calc_version gate.
            let comparable =
                s.duration_ms.is_some() && s.duration_calc_version == Some(DURATION_CALC_VERSION);
            let Some(duration) = s.duration_ms.filter(|_| comparable).map(f64::from) else {
                excluded += 1;
                continue;
            };
            if s.created_at_epoch >= recent_start {
                recent.push(duration);
            } else if s.created_at_epoch >= baseline_start {
                baseline.push(duration);
            }
        }
        total_excluded += excluded;

        let (object_type, modification_type) =
            (object_type.to_string(), modification_type.to_string());

        if baseline.is_empty() && recent.is_empty() {
            skipped.push(SkippedGroup {
                object_type,
                modification_type,
                reason: SkipReason::NoComparableV2Data,
                baseline_samples: 0,
                recent_samples: 0,
                excluded_samples: excluded,
            });
            continue;
        }

        if baseline.len() < params.min_samples || recent.len() < params.min_samples {
            skipped.push(SkippedGroup {
                object_type,
                modification_type,
                reason: SkipReason::InsufficientSamples,
                baseline_samples: baseline.len(),
                recent_samples: recent.len(),
                excluded_samples: excluded,
            });
            continue;
        }

        baseline.sort_by(f64::total_cmp);
        recent.sort_by(f64::total_cmp);
        let baseline_p50 = percentile_sorted(&baseline, 50.0);
        let recent_p50 = percentile_sorted(&recent, 50.0);
        // Floor the denominator at 1ms so a near-zero baseline can't divide by
        // zero (the JSON seam must stay finite).
        let pct_change = (recent_p50 / baseline_p50.max(1.0) - 1.0) * 100.0;

        if pct_change >= params.threshold_pct && (recent_p50 - baseline_p50) >= params.min_delta_ms
        {
            findings.push(RegressionFinding {
                object_type,
                modification_type,
                baseline_p50,
                baseline_p95: percentile_sorted(&baseline, 95.0),
                recent_p50,
                recent_p95: percentile_sorted(&recent, 95.0),
                pct_change,
                baseline_samples: baseline.len(),
                recent_samples: recent.len(),
            });
        }
    }

    let summary = ScanSummary {
        groups_analyzed,
        regressions: findings.len(),
        total_samples: samples.len(),
        excluded_samples: total_excluded,
    };
    RegressionReport {
        findings,
        skipped,
        summary,
    }
}

/// One of the slowest individual mutations (`perf explore slowest`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SlowSample {
    /// Entity type (`object_type`).
    pub object_type:       String,
    /// `INSERT` / `UPDATE` / `DELETE` / `CUSTOM`.
    pub modification_type: String,
    /// Measured wall-clock duration, in milliseconds.
    pub duration_ms:       i32,
    /// The changed entity's id, when present.
    pub object_id:         Option<String>,
    /// W3C trace id, when populated — the handle for deeper investigation.
    pub trace_id:          Option<String>,
}

/// Return the `limit` slowest comparable (v2) mutations, slowest first.
///
/// Gated on the current `duration_calc_version`: pre-fix rows under-report long
/// durations, so ranking them would mislead. Use `null-rate` to see how much
/// data that excludes.
#[must_use]
pub fn slowest(samples: &[ChangeLogSample], limit: usize) -> Vec<SlowSample> {
    let mut rows: Vec<SlowSample> = samples
        .iter()
        .filter_map(|s| {
            let comparable = s.duration_calc_version == Some(DURATION_CALC_VERSION);
            s.duration_ms.filter(|_| comparable).map(|duration_ms| SlowSample {
                object_type: s.object_type.clone(),
                modification_type: s.modification_type.clone(),
                duration_ms,
                object_id: s.object_id.clone(),
                trace_id: s.trace_id.clone(),
            })
        })
        .collect();
    rows.sort_by(|a, b| b.duration_ms.cmp(&a.duration_ms));
    rows.truncate(limit);
    rows
}

/// `duration_ms` completeness for one operation.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NullRateRow {
    /// Entity type (`object_type`); `"*"` on the overall row.
    pub object_type:       String,
    /// Modification type; `"*"` on the overall row.
    pub modification_type: String,
    /// Total rows seen.
    pub total:             usize,
    /// Rows with a `NULL` duration (cooperative producers; no request clock).
    pub null_duration:     usize,
    /// Rows with a duration but a missing / pre-fix `duration_calc_version`.
    pub non_v2:            usize,
    /// `null_duration / total * 100`.
    pub null_rate_pct:     f64,
}

/// Per-operation and overall `duration_ms` completeness (`perf explore null-rate`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NullRateReport {
    /// One row per `(object_type, modification_type)`.
    pub rows:    Vec<NullRateRow>,
    /// The aggregate across every row.
    pub overall: NullRateRow,
}

/// Measure how complete `duration_ms` is, per operation and overall.
///
/// `null_duration` counts cooperative-producer rows (legitimately `NULL`); `non_v2`
/// counts rows with a duration but no current marker. Both are *expected*, not
/// drift — this is the lens that quantifies them.
#[must_use]
pub fn null_rate(samples: &[ChangeLogSample]) -> NullRateReport {
    // (total, null_duration, non_v2) accumulator.
    let mut groups: BTreeMap<(&str, &str), [usize; 3]> = BTreeMap::new();
    let mut overall = [0usize; 3];

    for s in samples {
        let acc = groups
            .entry((s.object_type.as_str(), s.modification_type.as_str()))
            .or_default();
        acc[0] += 1;
        overall[0] += 1;
        if s.duration_ms.is_none() {
            acc[1] += 1;
            overall[1] += 1;
        } else if s.duration_calc_version != Some(DURATION_CALC_VERSION) {
            acc[2] += 1;
            overall[2] += 1;
        }
    }

    let rows = groups
        .into_iter()
        .map(|((object_type, modification_type), c)| {
            null_rate_row(object_type.to_string(), modification_type.to_string(), c)
        })
        .collect();

    NullRateReport {
        rows,
        overall: null_rate_row("*".to_string(), "*".to_string(), overall),
    }
}

fn null_rate_row(object_type: String, modification_type: String, c: [usize; 3]) -> NullRateRow {
    let [total, null_duration, non_v2] = c;
    let null_rate_pct = if total == 0 {
        0.0
    } else {
        ratio_pct(null_duration, total)
    };
    NullRateRow {
        object_type,
        modification_type,
        total,
        null_duration,
        non_v2,
        null_rate_pct,
    }
}

/// Latency percentiles for one operation (`perf explore summary`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OperationSummary {
    /// Entity type (`object_type`).
    pub object_type:       String,
    /// `INSERT` / `UPDATE` / `DELETE` / `CUSTOM`.
    pub modification_type: String,
    /// Comparable (v2) rows behind the percentiles.
    pub count:             usize,
    /// 50th / 95th / 99th percentile latency and the max, in milliseconds.
    pub p50:               f64,
    /// 95th-percentile latency, in milliseconds.
    pub p95:               f64,
    /// 99th-percentile latency, in milliseconds.
    pub p99:               f64,
    /// Slowest observed latency, in milliseconds.
    pub max:               f64,
}

/// Per-operation latency percentiles over comparable (v2) rows, slowest `p95`
/// first. Operations with no comparable rows are omitted.
#[must_use]
pub fn summary(samples: &[ChangeLogSample]) -> Vec<OperationSummary> {
    let mut groups: BTreeMap<(&str, &str), Vec<f64>> = BTreeMap::new();
    for s in samples {
        let comparable = s.duration_calc_version == Some(DURATION_CALC_VERSION);
        if let Some(duration) = s.duration_ms.filter(|_| comparable).map(f64::from) {
            groups
                .entry((s.object_type.as_str(), s.modification_type.as_str()))
                .or_default()
                .push(duration);
        }
    }

    let mut out: Vec<OperationSummary> = groups
        .into_iter()
        .map(|((object_type, modification_type), mut durations)| {
            durations.sort_by(f64::total_cmp);
            OperationSummary {
                object_type:       object_type.to_string(),
                modification_type: modification_type.to_string(),
                count:             durations.len(),
                p50:               percentile_sorted(&durations, 50.0),
                p95:               percentile_sorted(&durations, 95.0),
                p99:               percentile_sorted(&durations, 99.0),
                max:               durations.last().copied().unwrap_or(0.0),
            }
        })
        .collect();
    out.sort_by(|a, b| b.p95.total_cmp(&a.p95));
    out
}

/// `numerator / denominator * 100`, guarding the zero denominator.
#[allow(clippy::cast_precision_loss)] // Reason: completeness counts are small; usize→f64 is lossless here
fn ratio_pct(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64 * 100.0
    }
}

/// Linear-interpolation percentile (the NumPy default) over an **already-sorted**
/// slice. `pct` is in `[0, 100]`. An empty slice yields `0.0`.
#[must_use]
// Reason: percentile index math — `rank` is non-negative and bounded by the slice
// length, so the f64↔usize conversions cannot truncate or lose sign meaningfully.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn percentile_sorted(sorted: &[f64], pct: f64) -> f64 {
    match sorted {
        [] => 0.0,
        [only] => *only,
        _ => {
            let rank = (pct / 100.0) * (sorted.len() - 1) as f64;
            let lo = rank.floor() as usize;
            let hi = rank.ceil() as usize;
            if lo == hi {
                sorted[lo]
            } else {
                let frac = rank - lo as f64;
                (sorted[hi] - sorted[lo]).mul_add(frac, sorted[lo])
            }
        },
    }
}

/// Sort `values` and return its `pct` percentile. Convenience wrapper over
/// [`percentile_sorted`] for callers holding an unsorted buffer.
#[must_use]
pub fn percentile(values: &mut [f64], pct: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    percentile_sorted(values, pct)
}
