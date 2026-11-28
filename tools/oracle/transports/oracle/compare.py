"""Outcomes, and the comparison of one cell with the POST /graphql reference.

An outcome's class comes from its codes alone, through `CODE_CLASS`. A code missing from
the table keeps its own name (`ERR:<code>`), so a new code never merges into a known
class unnoticed.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from .clients import Response
    from .generate import Request

# code -> class. Each transport's own spelling of the same refusal maps to one class.
CODE_CLASS = {
    "FORBIDDEN": "DENIED",
    "flight:FlightUnauthorizedError": "DENIED",
    "UNAUTHENTICATED": "UNAUTH",
    "UNAUTHORIZED": "UNAUTH",
    "http-401": "UNAUTH",
    "flight:FlightUnauthenticatedError": "UNAUTH",
    "VALIDATION_ERROR": "INVALID",
    "GRAPHQL_VALIDATION_FAILED": "INVALID",
    "BAD_REQUEST": "INVALID",
    "flight:ArrowInvalid": "INVALID",
    "NOT_FOUND": "NOTFOUND",
    "http-404": "NOTFOUND",
}


@dataclass
class Outcome:
    cls: str
    rows: list[Any] | None
    keys: set[str]


def outcome(resp: Response) -> Outcome:
    """The class and rows of a response. A partial response keeps its rows, so data served
    beside an error still counts as served."""
    rows: list[Any] | None = None
    if resp.data is not None:
        rows = resp.data if isinstance(resp.data, list) else [resp.data]
    keys = {k for r in rows or [] if isinstance(r, dict) for k in r}
    if resp.codes:
        cls = "+".join(sorted({CODE_CLASS.get(c, f"ERR:{c}") for c in resp.codes}))
        if resp.has_data:
            return Outcome(f"PARTIAL({cls})", rows, keys)
        return Outcome(cls, None, set())
    return Outcome("OK", rows, keys)


def _norm(v: Any) -> Any:
    if isinstance(v, float) and v.is_integer() and abs(v) < 2**53:
        return int(v)
    return v


def project(rows: list[Any] | None, fields: list[str]) -> list[Any] | None:
    if rows is None:
        return None
    return [{f: _norm(r.get(f)) for f in fields} if isinstance(r, dict) else r for r in rows]


def compare_cell(req: Request, column: str, ref: Outcome, got: Outcome) -> list[str]:
    """Issues of one cell against POST. Empty when they agree.

    MCP takes no selection: it returns the fields the caller may read. Its rows are
    compared on the selected fields it returned; a selected field it left out is the
    issue `omitted:<fields>`, so the omission is counted, never skipped.
    """
    issues: list[str] = []
    fields = list(req.fields)
    if column == "MCP" and got.cls == "OK" and got.rows:
        omitted = sorted(f for f in fields if f not in got.keys)
        if omitted:
            issues.append(f"omitted:{'+'.join(omitted)}")
            fields = [f for f in fields if f in got.keys]
    if ref.cls != got.cls:
        issues.append(f"class:{ref.cls}->{got.cls}")
        return issues
    if ref.cls == "OK" and project(ref.rows, fields) != project(got.rows, fields):
        issues.append("data")
    return issues


def fingerprint(req: Request) -> str:
    canon = json.dumps(req.canon(), sort_keys=True, ensure_ascii=False)
    return hashlib.sha256(canon.encode()).hexdigest()[:16]
