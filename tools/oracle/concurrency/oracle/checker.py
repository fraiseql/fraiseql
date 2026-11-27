"""Linearizability-lite over a per-account register.

A history is one JSON line of metadata (`{"op": "meta", "initial": {key: version}}`)
followed by one line per operation, each with monotonic invoke (`ti`) and complete (`tc`)
times. A write (`"op": "w"`) bumps one key and, when `ok`, returns the version it wrote; a
write whose outcome is unknown (`ok` null: the client gave up) may or may not have
happened. A read (`"op": "r"`) returns the version a read shape served.

- A read is STALE when it returns less than the largest version returned by a write on
  its key whose response completed strictly before the read was invoked. A write
  completing exactly when the read is invoked is concurrent with it.
- A read is FABRICATED when it returns a version no write could have produced by the
  time it completed: below the key's initial version, or above the initial version plus
  the writes on the key invoked before the read completed.
- A read that returned no version is an ERROR.
"""

from __future__ import annotations

import bisect
import collections
import json
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from pathlib import Path


@dataclass
class Stats:
    reads: int = 0
    stale: int = 0
    fabricated: int = 0
    errors: int = 0
    max_window_s: float = 0.0
    examples: list[dict[str, Any]] = field(default_factory=list)

    def example(self, kind: str, op: dict[str, Any], **extra: Any) -> None:
        if len(self.examples) < 3:
            self.examples.append({"kind": kind, "read": op, **extra})


def load(path: Path) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    lines = [json.loads(line) for line in path.read_text().splitlines() if line]
    return lines[0], lines[1:]


def check(meta: dict[str, Any], ops: list[dict[str, Any]]) -> dict[str, Stats]:
    """Per `shape@server` statistics of a history."""
    initial = {int(k): int(v) for k, v in meta["initial"].items()}
    done: dict[int, list[tuple[float, int]]] = collections.defaultdict(list)
    invoked: dict[int, list[float]] = collections.defaultdict(list)
    for o in ops:
        if o["op"] != "w":
            continue
        invoked[o["k"]].append(o["ti"])
        if o["ok"]:
            done[o["k"]].append((o["tc"], o["v"]))
    # Per key, by completion time: (tc, largest version returned so far, this version).
    prefix: dict[int, list[tuple[float, int, int]]] = {}
    for k, ws in done.items():
        ws.sort()
        best = -1
        prefix[k] = []
        for tc, v in ws:
            best = max(best, v)
            prefix[k].append((tc, best, v))
    for ts in invoked.values():
        ts.sort()

    stats: dict[str, Stats] = collections.defaultdict(Stats)
    for o in ops:
        if o["op"] != "r":
            continue
        s = stats[f"{o['shape']}@{o.get('server', 0)}"]
        s.reads += 1
        v = o.get("v")
        if not o["ok"] or not isinstance(v, int):
            s.errors += 1
            s.example("error", o)
            continue
        k = o["k"]
        ceiling = initial[k] + bisect.bisect_left(invoked.get(k, []), o["tc"])
        if v < initial[k] or v > ceiling:
            s.fabricated += 1
            s.example("fabricated", o, initial=initial[k], ceiling=ceiling)
            continue
        arr = prefix.get(k, [])
        i = bisect.bisect_left(arr, (o["ti"], -1, -1))
        if i and v < arr[i - 1][1]:
            s.stale += 1
            first = next(tc for tc, _, wv in arr[:i] if wv > v)
            window = o["ti"] - first
            s.max_window_s = max(s.max_window_s, window)
            s.example("stale", o, required=arr[i - 1][1], window_s=round(window, 4))
    return dict(stats)
