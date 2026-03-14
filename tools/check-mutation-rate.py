#!/usr/bin/env python3
"""
Reads cargo-mutants output and fails if survival rate exceeds threshold.

Usage:
    python3 tools/check-mutation-rate.py <output-dir> --max-rate 0.30

The output directory is the `--output` directory passed to cargo-mutants
(defaults to `mutants.out/`). The script reads `outcomes.json` from that
directory.

Exit codes:
    0 — survival rate is within threshold (gate passes)
    1 — survival rate exceeds threshold (gate fails) or configuration error
"""

import argparse
import json
import sys
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Check cargo-mutants survival rate against a threshold."
    )
    parser.add_argument(
        "output_dir",
        help="Path to cargo-mutants output directory (contains outcomes.json)",
    )
    parser.add_argument(
        "--max-rate",
        type=float,
        default=0.30,
        help="Maximum allowed survival rate (default: 0.30 = 30%%)",
    )
    args = parser.parse_args()

    outcomes_path = Path(args.output_dir) / "mutants.out" / "outcomes.json"
    if not outcomes_path.exists():
        # Also accept the path directly (e.g., when output_dir = "mutants.out")
        direct = Path(args.output_dir) / "outcomes.json"
        if direct.exists():
            outcomes_path = direct
        else:
            print(
                f"Error: outcomes.json not found at {outcomes_path} or {direct}",
                file=sys.stderr,
            )
            sys.exit(1)

    try:
        with outcomes_path.open() as f:
            data = json.load(f)
    except (OSError, json.JSONDecodeError) as e:
        print(f"Error reading {outcomes_path}: {e}", file=sys.stderr)
        sys.exit(1)

    caught = data.get("caught", 0)
    missed = data.get("missed", 0)
    timeout = data.get("timeout", 0)
    unviable = data.get("unviable", 0)

    total = caught + missed
    if total == 0:
        print(
            "No mutants evaluated — check cargo-mutants configuration and output.",
            file=sys.stderr,
        )
        sys.exit(1)

    rate = missed / total
    print(
        f"Mutation results: {caught} caught, {missed} survived, "
        f"{timeout} timeout, {unviable} unviable"
    )
    print(f"Survival rate: {rate:.1%} (threshold: {args.max_rate:.1%})")

    if missed > 0:
        print("\nSurviving mutants:")
        for outcome in data.get("outcomes", []):
            scenario = outcome.get("scenario", {})
            summary = outcome.get("summary", "")
            if summary == "MissedMutant":
                name = scenario.get("MutantName", str(scenario)) if isinstance(scenario, dict) else str(scenario)
                print(f"  {name}")

    if rate > args.max_rate:
        print(f"\n\u274c GATE FAILED: {rate:.1%} > {args.max_rate:.1%}")
        sys.exit(1)
    else:
        print(f"\n\u2705 GATE PASSED: {rate:.1%} \u2264 {args.max_rate:.1%}")
        sys.exit(0)


if __name__ == "__main__":
    main()
