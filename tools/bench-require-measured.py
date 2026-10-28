#!/usr/bin/env python3
"""A velocitybench run that measured nothing fails.

bench_sequential.py exits 0 when a framework never comes up: it records every
query as skipped ("service did not become healthy") and writes a results file
that looks like a run. The release benchmark is report-only, so a slower number
never fails it, but a harness that measured nothing must, or a broken benchmark
reads as a quiet one.

A query is measured when it was not skipped and at least one of its requests
succeeded. A query velocitybench skips for one framework is not a failure, as
long as another query was measured.

Usage: bench-require-measured.py RESULTS_JSON

Exit codes: 0 something measured, 1 nothing measured, 2 the file cannot be read.
"""

import json
import sys
from collections import Counter
from pathlib import Path


def measured(row: dict) -> bool:
    return not row.get("skipped", False) and row.get("requests", 0) > row.get(
        "errors", 0
    )


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    path = Path(argv[1])
    try:
        rows = json.loads(path.read_text())["results"]
    except (OSError, ValueError, KeyError, TypeError) as e:
        print(f"cannot read velocitybench results from {path}: {e}")
        return 2

    ok = [r for r in rows if measured(r)]
    if ok:
        print(f"measured {len(ok)} of {len(rows)} queries")
        return 0

    reasons = Counter(
        (r.get("skip_reason") if r.get("skipped") else None) or "every request failed"
        for r in rows
    )
    detail = "; ".join(f"{n} × {why}" for why, n in reasons.most_common())
    print(
        f"nothing was measured: {len(rows)} queries, none with a successful request"
        + (f" ({detail})" if detail else "")
    )
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
