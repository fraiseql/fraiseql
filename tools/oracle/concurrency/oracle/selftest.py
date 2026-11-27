"""The oracle's proof that it can fail.

- Checker: hand-built histories, each with one known defect or none, must be reported as
  exactly that.
- Gate: a baseline differing from the outcomes by one entry, in each direction and at
  each site, must fail; the matching baseline must pass.

`python -m oracle --selftest` runs both and exits non-zero on the first miss.
"""

from __future__ import annotations

from typing import Any

from oracle import checker, gate
from oracle.cells import Outcome

META = {"op": "meta", "initial": {"1": 0}}


def _w(ti: float, tc: float, v: int | None, ok: bool | None = True) -> dict[str, Any]:
    return {"op": "w", "k": 1, "ti": ti, "tc": tc, "ok": ok, "v": v}


def _r(ti: float, tc: float, v: int | None, ok: bool = True) -> dict[str, Any]:
    return {"op": "r", "k": 1, "shape": "direct", "server": 0, "ti": ti, "tc": tc, "ok": ok, "v": v}


# name -> (operations, expected (stale, fabricated, errors))
CHECKER_CASES: dict[str, tuple[list[dict[str, Any]], tuple[int, int, int]]] = {
    "a read after the write sees it": ([_w(0, 1, 1), _r(2, 3, 1)], (0, 0, 0)),
    "a read after the write misses it": ([_w(0, 1, 1), _r(2, 3, 0)], (1, 0, 0)),
    "a read overlapping the write may miss it": ([_w(0, 1, 1), _r(0.5, 3, 0)], (0, 0, 0)),
    "a read invoked as the write completes is concurrent": ([_w(0, 1, 1), _r(1, 3, 0)], (0, 0, 0)),
    "a read sees a later write's version early": (
        [_w(0, 1, 1), _w(2, 5, 2), _r(3, 4, 2)],
        (0, 0, 0),
    ),
    "a version no write could have made yet": ([_w(2, 3, 1), _r(0, 1, 1)], (0, 1, 0)),
    "a version below the initial one": ([_r(0, 1, -1)], (0, 1, 0)),
    "a version beyond every invoked write": ([_w(0, 1, 1), _r(2, 3, 2)], (0, 1, 0)),
    "an unknown write may have happened": ([_w(0, 1, None, ok=None), _r(2, 3, 1)], (0, 0, 0)),
    "a read that returned nothing": ([_w(0, 1, 1), _r(2, 3, None, ok=False)], (0, 0, 1)),
    "an older version after a newer one is stale": (
        [_w(0, 1, 1), _w(2, 3, 2), _r(4, 5, 1)],
        (1, 0, 0),
    ),
}


def checker_misses() -> tuple[int, list[str]]:
    misses = []
    for name, (ops, expected) in CHECKER_CASES.items():
        stats = checker.check(META, ops)
        s = stats.get("direct@0", checker.Stats())
        got = (s.stale, s.fabricated, s.errors)
        if got != expected:
            misses.append(
                f"checker: {name}: (stale, fabricated, errors) {got}, expected {expected}"
            )
    return len(CHECKER_CASES), misses


def gate_misses() -> tuple[int, list[str]]:
    outcomes = [
        Outcome("CONC:t:red", holds=False),
        Outcome("CONC:t:green", holds=True),
    ]
    matching = {"CONC:t:red": {"issue": 1}}
    misses = []
    if gate.compare(outcomes, matching):
        misses.append(f"gate: the matching baseline fails: {gate.compare(outcomes, matching)}")
    variants: dict[str, tuple[list[Outcome], dict[str, dict[str, Any]], str]] = {
        "a failing cell missing from the baseline": (outcomes, {}, "NEW"),
        "a recorded cell that holds": (
            outcomes,
            {**matching, "CONC:t:green": {"issue": 2}},
            "GONE",
        ),
        "a recorded cell that did not run": (
            outcomes,
            {**matching, "CONC:t:absent": {"issue": 3}},
            "NOT RUN",
        ),
        "an entry with no issue": (outcomes, {"CONC:t:red": {"issue": None}}, "UNNAMED"),
        "a cell reported twice": (
            [*outcomes, Outcome("CONC:t:green", holds=True)],
            matching,
            "DUPLICATE",
        ),
    }
    for name, (os_, cells, word) in variants.items():
        problems = gate.compare(os_, cells)
        if not any(p.startswith(word) for p in problems):
            misses.append(f"gate: {name}: expected {word}, got {problems}")
    return 1 + len(variants), misses


def run() -> tuple[int, list[str]]:
    """The number of cases, and one line per case not reported as expected."""
    n_checker, m_checker = checker_misses()
    n_gate, m_gate = gate_misses()
    return n_checker + n_gate, m_checker + m_gate
