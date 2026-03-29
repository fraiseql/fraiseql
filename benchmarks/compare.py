#!/usr/bin/env python3
"""Compare k6 load test results against a saved baseline.

Reads two k6 summary-export JSON files and checks whether key latency and
throughput metrics have regressed beyond configurable thresholds.

Exit codes:
    0  No regression detected (or no baseline available).
    1  Regression detected — at least one metric exceeds the threshold.

Usage:
    python3 benchmarks/compare.py \
        --baseline /tmp/baseline/summary-basic.json \
        --current  /tmp/current/summary-basic.json \
        --threshold-p99 10 \
        --threshold-throughput 5
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def load_summary(path: Path) -> dict:
    with path.open() as f:
        return json.load(f)


def get_metric_value(summary: dict, metric: str, stat: str) -> float | None:
    """Extract a specific statistic from a k6 summary metric."""
    metrics = summary.get("metrics", {})
    m = metrics.get(metric, {})
    values = m.get("values", {})
    return values.get(stat)


def pct_change(baseline: float, current: float) -> float:
    if baseline == 0:
        return 0.0
    return ((current - baseline) / baseline) * 100.0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True, help="Baseline summary JSON")
    parser.add_argument("--current", type=Path, required=True, help="Current summary JSON")
    parser.add_argument(
        "--threshold-p99",
        type=float,
        default=10.0,
        help="Max allowed P99 latency increase (%%)",
    )
    parser.add_argument(
        "--threshold-throughput",
        type=float,
        default=5.0,
        help="Max allowed throughput decrease (%%)",
    )
    args = parser.parse_args()

    if not args.baseline.exists():
        print("No baseline found — skipping comparison (first run).")
        return 0

    if not args.current.exists():
        print(f"ERROR: current results not found: {args.current}", file=sys.stderr)
        return 1

    baseline = load_summary(args.baseline)
    current = load_summary(args.current)

    regressions: list[str] = []

    # --- Latency checks (higher is worse) ---
    for stat in ("p(95)", "p(99)"):
        b = get_metric_value(baseline, "http_req_duration", stat)
        c = get_metric_value(current, "http_req_duration", stat)
        if b is None or c is None:
            continue
        delta = pct_change(b, c)
        label = f"http_req_duration {stat}"
        threshold = args.threshold_p99
        status = "REGRESSION" if delta > threshold else "ok"
        print(f"  {label}: {b:.2f}ms -> {c:.2f}ms ({delta:+.1f}%) [{status}]")
        if delta > threshold:
            regressions.append(f"{label} regressed {delta:+.1f}% (threshold: {threshold}%)")

    # --- Throughput check (lower is worse) ---
    b_rps = get_metric_value(baseline, "http_reqs", "rate")
    c_rps = get_metric_value(current, "http_reqs", "rate")
    if b_rps is not None and c_rps is not None:
        delta = pct_change(b_rps, c_rps)
        status = "REGRESSION" if delta < -args.threshold_throughput else "ok"
        print(f"  http_reqs rate: {b_rps:.1f}/s -> {c_rps:.1f}/s ({delta:+.1f}%) [{status}]")
        if delta < -args.threshold_throughput:
            regressions.append(
                f"throughput dropped {delta:+.1f}% (threshold: -{args.threshold_throughput}%)"
            )

    # --- Error rate check (higher is worse) ---
    b_err = get_metric_value(baseline, "graphql_errors", "rate")
    c_err = get_metric_value(current, "graphql_errors", "rate")
    if b_err is not None and c_err is not None:
        print(f"  graphql_errors rate: {b_err:.4f} -> {c_err:.4f}")
        if c_err > 0.01 and b_err <= 0.01:
            regressions.append(f"error rate crossed 1% threshold: {c_err:.4f}")

    if regressions:
        print(f"\nFAILED — {len(regressions)} regression(s) detected:")
        for r in regressions:
            print(f"  - {r}")
        return 1

    print("\nPASSED — no regressions detected.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
