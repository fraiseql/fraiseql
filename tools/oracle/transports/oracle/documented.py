"""The documented transport differences: docs/graphql-conformance.md, section "Transports".

Each row of that table names one difference from POST. For each id, this module holds:

- a matcher: it recognises a corpus cell showing the difference, from the request's shape
  and the responses' codes, never from a message;
- a probe: it asks once on each side of the boundary;
- a judge: a pure function of the probe's two answers. It returns a finding when the
  answer inside the boundary is not served, or the one outside it is not refused as the
  row says.

The table decides what is documented. An id the table lists with no matcher or probe here
is a harness error, so a new row cannot pass unchecked. A matcher whose id the table no
longer lists matches nothing, so removing a row makes its cells undocumented again.
"""

from __future__ import annotations

import json
import os
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, Any

from . import generate as gen
from .clients import (
    Clients,
    Response,
    _get_unbounded,
    get_target,
    graphql_body,
    rest_target,
)

if TYPE_CHECKING:
    from .compare import Outcome

DOC = Path(
    os.environ.get("ORACLE_CONFORMANCE_DOC")
    or Path(__file__).resolve().parents[4] / "docs" / "graphql-conformance.md"
)
URL_CAP = 65_534
FILTER_CAP = 4096
TICKET_CAP = 256 * 1024
# Field scopes the fixture declares (fixture/fraiseql.toml): field -> scope, role -> scopes.
FIELD_SCOPE = {"secret": "read:secret", "note": "read:note", "email": "read:email"}
ROLE_SCOPES = {
    "auditor": {"read:secret", "read:note", "read:email"},
    "admin": {"read:secret", "read:note", "read:email"},
    "viewer": {"read:note"},
}


def read_table(doc: Path = DOC) -> list[str]:
    """The ids of the Transports table, in order."""
    text = doc.read_text()
    start = text.find("\n## Transports\n")
    if start < 0:
        msg = f'{doc}: no "## Transports" section'
        raise RuntimeError(msg)
    ids = []
    for line in text[start:].split("\n")[2:]:
        if line.startswith(("## ", "### ")):
            break
        cells = [c.strip() for c in line.split("|")]
        if len(cells) >= 5 and cells[1].startswith("`") and cells[1].endswith("`"):
            ids.append(cells[1][1:-1])
    if not ids:
        msg = f"{doc}: the Transports table has no rows"
        raise RuntimeError(msg)
    return ids


@dataclass
class CellView:
    """What a matcher sees of one compared cell."""

    req: gen.Request
    column: str
    ref: Outcome
    got: Outcome
    resp: Response
    anonymous: bool
    roots: dict[str, str]
    issues: list[str]


def _unreadable(who: str) -> set[str]:
    """The scoped fields `who` may not read."""
    p = gen.PRINCIPALS.get(who)
    roles = (p[2] or []) if p else []
    scopes = set().union(*(ROLE_SCOPES.get(r, set()) for r in roles)) if roles else set()
    return {f for f, s in FIELD_SCOPE.items() if s not in scopes}


def _omits_unreadable_only(c: CellView) -> bool:
    """MCP served rows with no field the caller may not read, and that is the only
    difference: POST refused the selection (`on_deny = "reject"`), or served the same
    rows with those fields null (`on_deny = "mask"`)."""
    hidden = _unreadable(c.req.who)
    if c.got.cls != "OK" or c.got.keys & hidden:
        return False
    if c.ref.cls == "DENIED":
        return True
    omitted = [i.removeprefix("omitted:").split("+") for i in c.issues if i.startswith("omitted:")]
    return c.ref.cls == "OK" and len(omitted) == len(c.issues) == 1 and set(omitted[0]) <= hidden


def _target_length(c: CellView) -> int:
    if c.column == "GET":
        doc, variables, _ = gen.graphql(c.req)
        return len(get_target(doc, variables))
    path, params = gen.rest(c.req, c.roots)
    return len(rest_target(path, params))


MATCHERS: dict[str, Callable[[CellView], bool]] = {
    "flight-requires-auth": lambda c: (
        c.column == "FLIGHT"
        and c.anonymous
        and c.resp.codes == ["flight:FlightUnauthenticatedError"]
    ),
    "mcp-requires-auth": lambda c: (
        c.column == "MCP" and c.anonymous and c.resp.codes == ["http-401"]
    ),
    "url-length-cap": lambda c: (
        c.column in ("GET", "REST") and c.resp.codes == ["http-414"] and _target_length(c) > URL_CAP
    ),
    "rest-filter-cap": lambda c: (
        c.column == "REST"
        and c.resp.codes == ["BAD_REQUEST"]
        and c.req.where is not None
        and len(json.dumps(c.req.where)) > FILTER_CAP
    ),
    "flight-ticket-cap": lambda c: (
        c.column == "FLIGHT"
        and c.resp.codes == ["flight:ArrowInvalid"]
        and len(json.dumps({"type": "GraphQLQuery", "query": gen.graphql(c.req)[0]})) > TICKET_CAP
    ),
    "mcp-field-omission": lambda c: c.column == "MCP" and _omits_unreadable_only(c),
}


def matching(view: CellView, ids: list[str]) -> str | None:
    """The documented row a cell shows, or None."""
    return next((i for i in ids if MATCHERS[i](view)), None)


# -- probes --------------------------------------------------------------------------

Judge = Callable[[Response, Response], "str | None"]


def _served(r: Response) -> bool:
    return not r.codes and r.has_data


def _orders_doc(pad: int) -> str:
    return '{ orders(limit: 1, where: {label: {eq: "%s"}}) { id } }' % ("x" * pad)


def _sized_get(http: str, token: str, size: int) -> Response:
    """GET /graphql with a request target of exactly `size` bytes."""
    base = len(get_target(_orders_doc(0), None))
    target = get_target(_orders_doc(size - base), None)
    if len(target) != size:
        msg = f"GET probe target is {len(target)} bytes, not {size}"
        raise RuntimeError(msg)
    return graphql_body(
        *_get_unbounded(http + target, {"authorization": f"Bearer {token}"}), "orders"
    )


def _sized_rest(http: str, token: str, size: int) -> Response:
    """A REST read with a request target of exactly `size` bytes."""

    def params(pad: int) -> dict[str, Any]:
        return {"select": "id", "limit": 1, "filter": json.dumps({"label": {"eq": "x" * pad}})}

    base = len(rest_target("/orders", params(0)))
    target = rest_target("/orders", params(size - base))
    if len(target) != size:
        msg = f"REST probe target is {len(target)} bytes, not {size}"
        raise RuntimeError(msg)
    status, _ = _get_unbounded(http + target, {"authorization": f"Bearer {token}"})
    return Response(status=str(status), codes=[f"http-{status}"] if status == 414 else [])


def _sized_ticket(c: Clients, token: str, size: int) -> Response:
    doc = "{ orders(limit: 1) { id } }"
    base = len(json.dumps({"type": "GraphQLQuery", "query": doc}))
    return c.flight(doc + " " * (size - base), None, token, {}, "orders")


def _filter(c: Clients, token: str, size: int) -> Response:
    base = len(json.dumps({"label": {"eq": ""}}))
    where = json.dumps({"label": {"eq": "x" * (size - base)}})
    return c.rest("/orders", {"select": "id", "limit": 1, "filter": where}, token, {})


@dataclass
class Probe:
    """Ask once inside the boundary and once outside it."""

    ask: Callable[[Clients, str, dict[str, str | None]], tuple[Response, Response]]
    judge: Judge


def _expect(inside_ok: Callable[[Response], bool], outside_codes: list[str]) -> Judge:
    def judge(inside: Response, outside: Response) -> str | None:
        problems = []
        if not inside_ok(inside):
            problems.append(f"inside the boundary: {inside.status} {inside.codes}")
        if outside.codes != outside_codes:
            problems.append(f"outside it: {outside.status} {outside.codes}, not {outside_codes}")
        return "; ".join(problems) or None

    return judge


def _rows_without(field: str) -> Callable[[Response], bool]:
    return lambda r: _served(r) and bool(r.data) and all(field not in x for x in r.data)


def _omission_judge(viewer: Response, auditor: Response) -> str | None:
    problems = []
    if not _rows_without("secret")(viewer):
        problems.append(f"a caller without read:secret: {viewer.status} {viewer.codes}")
    if not (_served(auditor) and auditor.data and all("secret" in x for x in auditor.data)):
        problems.append(f"a caller with read:secret: {auditor.status} {auditor.codes}")
    return "; ".join(problems) or None


def _url_judge(inside: Response, outside: Response) -> str | None:
    """Inside the cap, the URL is not what refuses the request; outside it, 414."""
    return _expect(lambda r: "http-414" not in r.codes, ["http-414"])(inside, outside)


PROBES: dict[str, list[Probe]] = {
    "flight-requires-auth": [
        Probe(
            lambda c, _h, t: (
                c.flight("{ orders(limit: 1) { id } }", None, t["A-viewer"], {}, "orders"),
                c.flight("{ orders(limit: 1) { id } }", None, None, {}, "orders"),
            ),
            _expect(_served, ["flight:FlightUnauthenticatedError"]),
        )
    ],
    "mcp-requires-auth": [
        Probe(
            lambda c, _h, t: (
                c.mcp("orders", {"limit": 1}, t["A-viewer"], {}, "orders"),
                c.mcp("orders", {"limit": 1}, None, {}, "orders"),
            ),
            _expect(_served, ["http-401"]),
        )
    ],
    "url-length-cap": [
        Probe(
            lambda _c, h, t: (
                _sized_get(h, t["A-viewer"] or "", URL_CAP),
                _sized_get(h, t["A-viewer"] or "", URL_CAP + 1),
            ),
            _expect(lambda r: not r.codes, ["http-414"]),
        ),
        Probe(
            lambda _c, h, t: (
                _sized_rest(h, t["A-viewer"] or "", URL_CAP),
                _sized_rest(h, t["A-viewer"] or "", URL_CAP + 1),
            ),
            _url_judge,
        ),
    ],
    "rest-filter-cap": [
        Probe(
            lambda c, _h, t: (
                _filter(c, t["A-viewer"] or "", FILTER_CAP),
                _filter(c, t["A-viewer"] or "", FILTER_CAP + 1),
            ),
            _expect(lambda r: not r.codes, ["BAD_REQUEST"]),
        )
    ],
    "flight-ticket-cap": [
        Probe(
            lambda c, _h, t: (
                _sized_ticket(c, t["A-viewer"] or "", TICKET_CAP),
                _sized_ticket(c, t["A-viewer"] or "", TICKET_CAP + 1),
            ),
            _expect(_served, ["flight:ArrowInvalid"]),
        )
    ],
    "mcp-field-omission": [
        Probe(
            lambda c, _h, t: (
                c.mcp("orders", {"limit": 3}, t["A-viewer"], {}, "orders"),
                c.mcp("orders", {"limit": 3}, t["A-auditor"], {}, "orders"),
            ),
            _omission_judge,
        )
    ],
}


def check(ids: list[str]) -> None:
    """Every listed id has a matcher and a probe."""
    missing = [i for i in ids if i not in MATCHERS or i not in PROBES]
    if missing:
        msg = f"the Transports table lists {missing}, which the oracle does not know how to check"
        raise RuntimeError(msg)


def probe(c: Clients, http: str, tokens: dict[str, str | None], ids: list[str]) -> list[str]:
    """One line per documented row that does not hold, on either side of its boundary."""
    findings = []
    for i in ids:
        for n, p in enumerate(PROBES[i]):
            inside, outside = p.ask(c, http, tokens)
            finding = p.judge(inside, outside)
            if finding:
                findings.append(f"{i}#{n}: {finding}")
    return findings
