"""Concurrent register runs: every read is judged by the linearizability-lite checker.

4 writers bump accounts through the writer's principal while 12 readers read them back,
half as that principal and half as others of its tenant, spread over six read shapes.
One server, so the documented promise applies to every read: none is staler than a write
whose response had already arrived when the read was sent.

A cell is one (config, contended rows, shape). It holds when the checker finds no stale,
fabricated or failed read in its history.
"""

from __future__ import annotations

import os
from typing import TYPE_CHECKING

from oracle import checker, db, history
from oracle.cells import Outcome, VacuousError
from oracle.client import account_id

if TYPE_CHECKING:
    from oracle.run import Run

SHAPES = ["direct", "list", "nested", "nestedcol", "fn", "summary"]
# name -> (cache enabled, TTL)
CONFIGS: dict[str, tuple[bool, int | None]] = {
    "cache_off": (False, 0),
    "cache_ttl0": (True, 0),
    "cache_ttl5": (True, 5),
}
KEYS = [1, 20]
SECS = float(os.environ.get("ORACLE_MATRIX_SECS", "10"))
MIN_READS = 20


def _initial(url: str, keys: list[int]) -> dict[int, int]:
    return {
        k: int(db.scalar(url, "select version from tb_account where id = %s", account_id(k)))
        for k in keys
    }


def run(r: Run) -> list[Outcome]:
    out: list[Outcome] = []
    for name, (cache, ttl) in CONFIGS.items():
        for n_keys in KEYS:
            keys = list(range(1, n_keys + 1))
            path = r.work / f"history.{name}.k{n_keys}.jsonl"
            with r.serve(f"matrix-{name}", cache=cache, ttl=ttl) as base:
                w = history.Workload(keys=keys, shapes=SHAPES, secs=SECS, seed=n_keys)
                history.record(
                    path,
                    write_url=base,
                    read_urls=[base],
                    issuer=r.issuer,
                    w=w,
                    initial=_initial(r.db_url, keys),
                )
            meta, ops = checker.load(path)
            writes = sum(1 for o in ops if o["op"] == "w" and o["ok"])
            if writes == 0:
                msg = f"matrix {name} k{n_keys}: no write succeeded ({path})"
                raise VacuousError(msg)
            stats = checker.check(meta, ops)
            for shape in SHAPES:
                s = stats.get(f"{shape}@0", checker.Stats())
                if s.reads < MIN_READS:
                    msg = f"matrix {name} k{n_keys} {shape}: {s.reads} reads, below {MIN_READS}"
                    raise VacuousError(msg)
                out.append(
                    Outcome(
                        f"CONC:matrix:{name}:k{n_keys}:{shape}:linearizable",
                        s.stale == 0 and s.fabricated == 0 and s.errors == 0,
                        {
                            "writes": writes,
                            "reads": s.reads,
                            "stale": s.stale,
                            "fabricated": s.fabricated,
                            "errors": s.errors,
                            "max_window_s": round(s.max_window_s, 4),
                            "examples": s.examples,
                            "history": path.name,
                        },
                    )
                )
    return out
