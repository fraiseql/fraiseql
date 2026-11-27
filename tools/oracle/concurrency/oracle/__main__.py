"""python -m oracle --cli PATH --server PATH --admin-url URL --work DIR [options]

Runs every cell group (or `--only` some), compares the outcomes with baseline.json and
exits 0 when they match, 1 when they differ, and 2 when the run could not judge (setup
failed, or a cell's precondition did not hold). `--bless` rewrites the baseline instead.
`--selftest` runs the oracle's own RED proof and needs no server.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import TYPE_CHECKING

from oracle import gate, selftest
from oracle.cells import VacuousError, idem, probe, replica
from oracle.client import ReadError
from oracle.run import HERE, Run, SetupError

if TYPE_CHECKING:
    from collections.abc import Callable

    from oracle.cells import Outcome

GROUPS: dict[str, Callable[[Run], list[Outcome]]] = {
    "probe": probe.run,
    "idem": idem.run,
    "replica": replica.run,
}
BASELINE = HERE / "baseline.json"


def main() -> int:
    p = argparse.ArgumentParser(prog="python -m oracle")
    p.add_argument("--cli", type=Path)
    p.add_argument("--server", type=Path)
    p.add_argument("--admin-url")
    p.add_argument("--work", type=Path)
    p.add_argument("--primary-url", default="", help="superuser URL of the replica cells' primary")
    p.add_argument("--replica-url", default="", help="superuser URL of its hot standby")
    p.add_argument("--only", help="comma-separated groups: " + ",".join(GROUPS))
    p.add_argument("--bless", action="store_true")
    p.add_argument("--selftest", action="store_true")
    a = p.parse_args()

    if a.selftest:
        total, misses = selftest.run()
        for m in misses:
            print(f"✗ {m}")
        print(f"selftest: {total - len(misses)}/{total} cases as expected")
        return 1 if misses else 0

    if not (a.cli and a.server and a.admin_url and a.work):
        p.error("--cli, --server, --admin-url and --work are required")
    groups = a.only.split(",") if a.only else list(GROUPS)
    unknown = [g for g in groups if g not in GROUPS]
    if unknown:
        p.error(f"unknown groups: {unknown}")

    r = Run(
        cli=a.cli,
        server=a.server,
        work=a.work,
        admin_url=a.admin_url,
        primary_url=a.primary_url,
        replica_url=a.replica_url,
    )
    outcomes: list[Outcome] = []
    try:
        r.db_url = r.setup_database()
        for g in groups:
            got = GROUPS[g](r)
            for o in got:
                print(f"  {'ok  ' if o.holds else 'FAIL'} {o.key}  {json.dumps(o.detail)[:200]}")
            outcomes += got
    except (SetupError, VacuousError, ReadError) as e:
        print(f"✗ cannot judge: {e}", file=sys.stderr)
        return 2
    finally:
        r.issuer.close()
    (a.work / "outcomes.json").write_text(
        json.dumps(
            [{"key": o.key, "holds": o.holds, "detail": o.detail} for o in outcomes], indent=1
        )
    )

    cells = gate.load(BASELINE) if BASELINE.exists() else {}
    if a.only:
        # A partial run judges only the groups it ran.
        ran = {f"CONC:{g}:" for g in groups}
        cells = {k: v for k, v in cells.items() if any(k.startswith(p_) for p_ in ran)}
    if a.bless:
        if a.only:
            print("✗ --bless needs a full run", file=sys.stderr)
            return 2
        unnamed = gate.bless(BASELINE, outcomes, cells)
        print(f"blessed {sum(not o.holds for o in outcomes)} cells into {BASELINE}")
        for k in unnamed:
            print(f"  name the issue of {k} before committing")
        return 0
    problems = gate.compare(outcomes, cells)
    for line in problems:
        print(f"✗ {line}")
    held = sum(o.holds for o in outcomes)
    print(
        f"concurrency oracle: {len(outcomes)} cells, {held} hold, "
        f"{len(outcomes) - held} recorded; {'FAIL' if problems else 'OK'}"
    )
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
