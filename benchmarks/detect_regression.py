#!/usr/bin/env python3
"""Regression detector for FraiseQL benchmarks using velocitybench results.

Compares a fresh velocitybench run against benchmarks/baseline.json.

velocitybench JSON output format (bench_sequential.py --output foo.md → foo.json):
    {"results": [{"framework": "fraiseql-tv", "query": "Q1", "rps": ..., "p95_ms": ...}, ...]}

Exit codes:
    0  No regression detected.
    1  Regression detected — at least one metric exceeds the threshold.

Usage:
    # bench_sequential.py writes foo.md; JSON lands at foo.json automatically
    python benchmarks/detect_regression.py \\
        --results /tmp/bench-results.json \\
        --baseline benchmarks/baseline.json \\
        --output /tmp/regression-report.md

    # Update baseline after a confirmed intentional perf change:
    python benchmarks/detect_regression.py \\
        --results /tmp/bench-results.json \\
        --baseline benchmarks/baseline.json \\
        --update
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def pct_change(before: float, after: float) -> float:
    if before == 0:
        return 0.0
    return (after - before) / before * 100.0


def check_metric(
    query: str,
    metric: str,
    baseline_val: float,
    current_val: float,
    threshold_pct: float,
    *,
    higher_is_better: bool,
) -> tuple[bool, str]:
    """Return (is_regression, markdown_table_row)."""
    delta = pct_change(baseline_val, current_val)
    if higher_is_better:
        regressed = delta < -threshold_pct
    else:
        regressed = delta > threshold_pct
    status = "❌" if regressed else "✅"
    direction = f"+{delta:.1f}%" if delta >= 0 else f"{delta:.1f}%"
    row = f"| {status} | {query} | {metric} | {baseline_val:.1f} | {current_val:.1f} | {direction} |"
    return regressed, row


def parse_velocitybench_results(raw: dict, framework: str = "fraiseql-tv") -> dict[str, dict]:
    """Convert velocitybench list format → {query_name: {rps, p95_ms, ...}} dict.

    velocitybench outputs:
        {"results": [{"framework": "fraiseql-tv", "query": "Q1", "rps": 9547, ...}, ...]}

    We index by query name for the matching framework (ignoring other frameworks
    that may appear in the same run, e.g. fraiseql-tv-cache).
    """
    results_list = raw.get("results", [])
    indexed: dict[str, dict] = {}
    for entry in results_list:
        if entry.get("framework") == framework and not entry.get("skipped"):
            query = entry["query"]
            indexed[query] = {
                "rps": entry.get("rps", 0.0),
                "p50_ms": entry.get("p50_ms", 0.0),
                "p95_ms": entry.get("p95_ms", 0.0),
                "p99_ms": entry.get("p99_ms", 0.0),
            }
    return indexed


def build_report(baseline: dict, results: dict, framework: str) -> tuple[str, list[str]]:
    thresholds = baseline["thresholds"]
    rps_threshold = thresholds["rps_regression_pct"]
    p95_threshold = thresholds["p95_regression_pct"]

    # Handle both velocitybench list format and pre-indexed dict format
    if isinstance(results.get("results"), list):
        current = parse_velocitybench_results(results, framework)
    else:
        current = results.get("results", results)

    rows: list[str] = []
    regressions: list[str] = []

    for query, bvals in baseline["results"].items():
        cvals = current.get(query)
        if not cvals:
            rows.append(f"| ⚠️ | {query} | ALL | — | — | no data |")
            regressions.append(f"{query}: no data in current run for framework '{framework}'")
            continue

        for metric, threshold, higher in [
            ("rps", rps_threshold, True),
            ("p95_ms", p95_threshold, False),
        ]:
            is_reg, row = check_metric(
                query,
                metric,
                bvals[metric],
                cvals[metric],
                threshold,
                higher_is_better=higher,
            )
            rows.append(row)
            if is_reg:
                regressions.append(
                    f"{query}/{metric}: {bvals[metric]:.1f} → {cvals[metric]:.1f}"
                    f" ({pct_change(bvals[metric], cvals[metric]):+.1f}%)"
                )

    header = [
        "## FraiseQL Benchmark Results\n",
        f"Baseline: `{baseline['git_sha']}` (v{baseline['version']}, {baseline['recorded_at'][:10]})\n",
        f"Framework: `{framework}`\n",
        "",
        "| Status | Query | Metric | Baseline | Current | Delta |",
        "|--------|-------|--------|----------|---------|-------|",
    ]
    body = "\n".join(header + rows)

    if regressions:
        body += "\n\n### ❌ Regressions Detected\n"
        for r in regressions:
            body += f"- {r}\n"
        body += (
            f"\nThresholds: RPS >{rps_threshold:.0f}% drop or p95 >{p95_threshold:.0f}% increase.\n"
            "To update baseline after an intentional change: `make bench-update-baseline`\n"
        )
    else:
        body += "\n\n### ✅ No regressions detected\n"

    return body, regressions


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Compare velocitybench results against FraiseQL baseline."
    )
    parser.add_argument("--results", required=True, help="Path to velocitybench JSON output")
    parser.add_argument("--baseline", required=True, help="Path to baseline JSON")
    parser.add_argument("--output", default="-", help="Output path for report (- = stdout)")
    parser.add_argument(
        "--framework",
        default="fraiseql-tv",
        help="Framework name to extract from velocitybench results (default: fraiseql-tv)",
    )
    parser.add_argument(
        "--update",
        action="store_true",
        help="Overwrite baseline with current results instead of comparing",
    )
    args = parser.parse_args()

    baseline_path = Path(args.baseline)
    results_path = Path(args.results)

    baseline = json.loads(baseline_path.read_text())
    raw_results = json.loads(results_path.read_text())

    if args.update:
        import datetime
        import subprocess  # noqa: PLC0415

        sha = subprocess.check_output(
            ["git", "rev-parse", "--short", "HEAD"], text=True
        ).strip()
        # Convert list format to indexed dict for baseline storage
        if isinstance(raw_results.get("results"), list):
            indexed = parse_velocitybench_results(raw_results, args.framework)
        else:
            indexed = raw_results.get("results", raw_results)

        new_baseline = {
            **baseline,
            "git_sha": sha,
            "recorded_at": datetime.datetime.utcnow().isoformat() + "Z",
            "results": indexed,
        }
        baseline_path.write_text(json.dumps(new_baseline, indent=2) + "\n")
        print(f"✅ Baseline updated: {baseline_path}")
        return

    report, regressions = build_report(baseline, raw_results, args.framework)

    if args.output == "-":
        print(report)
    else:
        Path(args.output).write_text(report)
        print(f"Report written to {args.output}")

    if regressions:
        sys.exit(1)


if __name__ == "__main__":
    main()
