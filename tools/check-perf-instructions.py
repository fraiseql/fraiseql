#!/usr/bin/env python3
"""The performance gate: each engine bench's instruction count against a committed baseline.

`crates/fraiseql-core/benches/executor_instructions.rs` (`Executor::execute`) and
`crates/fraiseql-server/benches/rest_instructions.rs` (a REST GET) run under callgrind
(iai-callgrind) and write one `summary.json` per bench under the iai home directory. This
script reads the instruction count (`Ir`) of each and compares it with
`benchmarks/instructions-baseline.json` (#1636).

Counts are not exactly repeatable: `std`'s `HashMap` seeds its hasher per process, and
the engine's maps hash differently on every run. One REST bench is bimodal, its runs
falling in two clusters 2.6% apart. So the leg runs the benches several times, each run
in its own subdirectory of the home, and the count compared is a bench's mean across the
runs. Measured over 16 runs of that bench, the mean of 9 stays within 0.83% of the
long-run mean in 99% of draws; the median of 9 strays 1.36%, since it jumps between
the clusters.

* A count more than THRESHOLD percent above its baseline fails: the change made the
  engine do more work per request.
* A count more than THRESHOLD percent below fails too, with the line to commit. A
  baseline left high hides the next regression up to the size of the improvement.
* A bench with no baseline entry, a baseline entry no bench measured, a count of 0
  (callgrind found no symbol to measure, as with a stripped binary) or no summary at
  all fails: each is a gate measuring nothing.

Counts depend on the binary, so the baseline is only meaningful in the environment that
recorded it: the CI leg's container (`dagger call perf-instructions`). `--print` writes
the measured counts as a baseline file, which is how it is refreshed.

Usage: check-perf-instructions.py [--home DIR] [--baseline FILE] [--threshold PCT] [--print]
"""

from __future__ import annotations

import argparse
import json
import statistics
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def instruction_count(summary: dict) -> int:
    """The `Ir` total of one iai-callgrind summary (the new run's value)."""
    ir = summary["profiles"][0]["summaries"]["total"]["summary"]["Callgrind"]["Ir"]["metrics"]
    (side,) = ir.values()  # "Left" (no previous run) or "Both" ([new, old])
    first = side[0] if isinstance(side, list) else side
    return int(first["Int"])


def measure(home: Path) -> dict[str, int]:
    """Each bench's mean instruction count over every run found under `home`."""
    runs: dict[str, list[int]] = {}
    for path in sorted(home.rglob("summary.json")):
        summary = json.loads(path.read_text(encoding="utf-8"))
        key = f"{summary['module_path']}.{summary['id']}"
        runs.setdefault(key, []).append(instruction_count(summary))
    return {key: round(statistics.fmean(counts)) for key, counts in runs.items()}


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--home", type=Path, default=ROOT / "target/iai")
    ap.add_argument("--baseline", type=Path, default=ROOT / "benchmarks/instructions-baseline.json")
    ap.add_argument("--threshold", type=float, default=3.0)
    ap.add_argument("--print", action="store_true", help="print the measured counts as a baseline")
    args = ap.parse_args()

    counts = measure(args.home)
    if not counts:
        print(f"perf gate: no summary.json under {args.home}; nothing was measured", file=sys.stderr)
        return 1
    zeros = [name for name, n in counts.items() if n == 0]
    if zeros:
        print("perf gate: callgrind measured nothing (0 instructions; a stripped binary?) for:", file=sys.stderr)
        for name in zeros:
            print(f"  {name}", file=sys.stderr)
        return 1
    if args.print:
        print(json.dumps({"benchmarks": counts}, indent=2, sort_keys=True))
        return 0
    if not args.baseline.is_file():
        print(f"perf gate: {args.baseline} not found", file=sys.stderr)
        return 1
    baseline = json.loads(args.baseline.read_text(encoding="utf-8"))["benchmarks"]

    failures = []
    for name, n in counts.items():
        if name not in baseline:
            failures.append(f"{name}: no baseline entry; add \"{name}\": {n}")
            continue
        base = baseline[name]
        change = (n - base) / base * 100
        print(f"  {name}: {n} instructions, baseline {base} ({change:+.2f}%)")
        if change > args.threshold:
            failures.append(f"{name}: {change:+.2f}% instructions, above the {args.threshold}% limit")
        elif change < -args.threshold:
            failures.append(f"{name}: {change:+.2f}%; lock it in: \"{name}\": {n}")
    failures += [f"{name}: in the baseline but not measured" for name in baseline if name not in counts]

    if failures:
        print("perf gate: FAIL", file=sys.stderr)
        for f in failures:
            print(f"  {f}", file=sys.stderr)
        return 1
    print(f"perf gate: ok ({len(counts)} benches within {args.threshold}% of the baseline)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
