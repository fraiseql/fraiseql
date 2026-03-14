#!/usr/bin/env python3
"""
Reads cargo-mutants output and fails if survival rate exceeds threshold.

Usage:
    python3 tools/check-mutation-rate.py mutants.out --max-rate 0.30

The report file is the `mutants.out` file produced by cargo-mutants in the
output directory (e.g. `mutants-report/mutants.out`).

Exit codes:
    0 — survival rate is within threshold (gate passes)
    1 — survival rate exceeds threshold (gate fails) or configuration error
"""

import argparse
import sys


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Check cargo-mutants survival rate against a threshold."
    )
    parser.add_argument("report_file", help="Path to cargo-mutants mutants.out file")
    parser.add_argument(
        "--max-rate",
        type=float,
        default=0.30,
        help="Maximum allowed survival rate (default: 0.30 = 30%%)",
    )
    args = parser.parse_args()

    try:
        with open(args.report_file) as f:
            lines = f.readlines()
    except FileNotFoundError:
        print(f"Error: report file not found: {args.report_file}", file=sys.stderr)
        sys.exit(1)

    caught = sum(1 for line in lines if line.startswith("caught "))
    survived = sum(1 for line in lines if line.startswith("survived "))
    timeout = sum(1 for line in lines if line.startswith("timeout "))
    unviable = sum(1 for line in lines if line.startswith("unviable "))

    total = caught + survived
    if total == 0:
        print(
            "No mutants evaluated — check cargo-mutants configuration and output format.",
            file=sys.stderr,
        )
        sys.exit(1)

    rate = survived / total
    print(
        f"Mutation results: {caught} caught, {survived} survived, "
        f"{timeout} timeout, {unviable} unviable"
    )
    print(f"Survival rate: {rate:.1%} (threshold: {args.max_rate:.1%})")

    if survived > 0:
        print("\nSurviving mutants:")
        for line in lines:
            if line.startswith("survived "):
                print(f"  {line.rstrip()}")

    if rate > args.max_rate:
        print(f"\n\u274c GATE FAILED: {rate:.1%} > {args.max_rate:.1%}")
        sys.exit(1)
    else:
        print(f"\n\u2705 GATE PASSED: {rate:.1%} \u2264 {args.max_rate:.1%}")
        sys.exit(0)


if __name__ == "__main__":
    main()
