"""Cross-transport differential oracle.

Each seeded logical request (oracle/generate.py) is sent over every transport it has a
spelling on, and every cell is compared with POST /graphql, the reference column. POST is
itself graded against graphql-js by tools/oracle/graphql-js, so agreement with POST is
agreement with the spec, transitively.

Metrics (README.md):
  - agreement: cells agreeing with POST, over all compared cells;
  - refused on POST, served elsewhere: POST refuses, another transport returns data;
  - no row on POST, served elsewhere: POST returns no row, another transport returns one;
  - undocumented divergences: every other disagreeing cell;
  - error-format divergences: a code outside `extensions.code`, by column, code and place;
  - pending columns: a transport with no cells yet (gRPC until it is mounted).

The gate is the graphql-js oracle's: every disagreement, format line and column line is
compared with baseline.json in both directions, so a new one is a regression and a
recorded one that no longer occurs is progress the baseline must record (`--bless`).

Exit codes: 0 agreement with the baseline, 1 the gate failed, 2 the harness failed.
"""

from __future__ import annotations

import argparse
import dataclasses
import importlib.util
import json
import os
import sys
import threading
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import httpx

from . import documented, probes, ws
from . import generate as gen
from .clients import Clients, Endpoints, Response
from .compare import compare_cell, fingerprint, outcome
from .issuer import load_key, mint
from .protocol import early_refusal
from .proxy import Proxy

HERE = Path(__file__).resolve().parent.parent
SEEDS = [1, 2]
PER_SEED = 2000
WORKERS = 8
SHOWN = 25
GATED = ["disagreements", "formats", "columns", "protocol", "documented"]
# The transports whose refusals must carry their code in `extensions.code`. The others
# carry it as the Transports table's "Error carriers" says.
GRAPHQL_COLUMNS = {"POST", "GET", "SSE", "WS"}
# Metrics that may never rise above their baseline value; today each is ratcheted at 0.
# A baseline without the value counts as 0, so a missing value cannot loosen the ratchet.
RATCHETED = ["empty_on_post_served_elsewhere"]
GATE_KEYS = [*GATED, "ratchets"]


@dataclass
class Private:
    requests: list[gen.Request]
    principals: dict[str, Any]
    corpus: gen.Corpus
    label: str
    probe_fns: list[probes.Probe] = field(default_factory=list)


EXTENSIONS = ("ROOTS", "ORDER_FIELDS", "FILTERS", "ORDER_GATED", "HEADERS", "WS_SELECTIONS")


def load_private() -> Private:
    """ORACLE_PRIVATE_MODULE, a Python file kept outside the repository, may export
    REQUESTS (extra requests, as dicts of Request fields), PRINCIPALS and PROBES (see
    oracle/probes.py), and extend the generator's corpus with ROOTS, ORDER_FIELDS, FILTERS,
    ORDER_GATED, HEADERS and WS_SELECTIONS."""
    path = os.environ.get("ORACLE_PRIVATE_MODULE")
    if not path:
        return Private([], {}, gen.PUBLIC, label="public")
    spec = importlib.util.spec_from_file_location("oracle_private", path)
    if spec is None or spec.loader is None:
        msg = f"cannot load {path}"
        raise RuntimeError(msg)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    reqs = []
    for spec_ in getattr(mod, "REQUESTS", []):
        fields: dict[str, Any] = {"style": "inline", "hdr": ""}
        fields.update(spec_)
        reqs.append(gen.Request(**fields))
    extra = {k: getattr(mod, k) for k in EXTENSIONS if hasattr(mod, k)}
    return Private(
        reqs,
        dict(getattr(mod, "PRINCIPALS", {})),
        gen.PUBLIC.extended(**extra),
        label=f"private:{Path(path).name}",
        probe_fns=list(getattr(mod, "PROBES", [])),
    )


def columns_state(http: str) -> list[str]:
    """One line per column that is not compared: today, gRPC while it is not mounted."""
    caps = httpx.get(f"{http}/health", timeout=10).json().get("capabilities", {})
    grpc = caps.get("grpc", {})
    if grpc.get("started") or grpc.get("serving"):
        return ["column GRPC: mounted, and the oracle has no client for it"]
    return ["column GRPC: pending (not mounted)"]


class Runner:
    def __init__(
        self,
        ends: Endpoints,
        key_path: Path,
        issuer: str,
        principals: dict[str, Any],
        corpus: gen.Corpus,
    ) -> None:
        self.ends = ends
        self.corpus = corpus
        key = load_key(key_path)
        self.tokens: dict[str, str | None] = {}
        for name, p in principals.items():
            if p is None:
                self.tokens[name] = None
                continue
            sub, tenant, roles, exp_in, aud = p
            kw = {"audience": aud} if aud else {}
            self.tokens[name] = mint(
                key, issuer, sub=sub, tenant=tenant, roles=roles, exp_in=exp_in, **kw
            )
        self._local = threading.local()
        self.all_clients: list[Clients] = []
        self._lock = threading.Lock()

    def clients(self) -> Clients:
        c = getattr(self._local, "c", None)
        if c is None:
            c = Clients(self.ends)
            self._local.c = c
            with self._lock:
                self.all_clients.append(c)
        return c

    def run(self, req: gen.Request) -> dict[str, Response]:
        c = self.clients()
        tok = self.tokens[req.who]
        hdr = {"x-probe": req.hdr} if req.hdr else {}
        doc, variables, key = gen.graphql(req)
        path, params = gen.rest(req, self.corpus.roots)
        calls = {
            "POST": lambda: c.post(doc, variables, tok, hdr, key),
            "GET": lambda: c.get(doc, variables, tok, hdr, key),
            "SSE": lambda: c.sse(doc, variables, tok, hdr, key),
            "REST": lambda: c.rest(path, params, tok, hdr),
            "MCP": lambda: c.mcp(req.root, gen.mcp(req), tok, hdr, req.root),
            "FLIGHT": lambda: c.flight(doc, variables, tok, hdr, key),
        }
        return {col: calls[col]() for col in gen.applicable(req)}


def _ws_column(
    args: argparse.Namespace, http: str, tokens: dict[str, str | None], corpus: gen.Corpus
) -> list[ws.Cell]:
    """The WS column writes rows, so it runs after the corpus, which reads the fixture.
    It talks to the server itself: the self-test's proxy does not carry WebSockets."""
    direct = Clients(Endpoints(http, args.flight))
    try:
        return ws.run(
            direct,
            http,
            tokens,
            args.db,
            selections=ws.SELECTIONS + corpus.ws_selections,
            tamper=getattr(args, "ws_tamper", None),
        )
    finally:
        direct.close()


@dataclass
class Tally:
    """Every compared cell, counted."""

    table: list[str]
    per_col: dict[str, list[int]] = field(
        default_factory=lambda: {c: [0, 0] for c in [*gen.COLUMNS, "WS"]}
    )
    failing: dict[str, Any] = field(default_factory=dict)
    formats: set[str] = field(default_factory=set)
    served_after_refusal: int = 0
    served_after_empty: int = 0
    divergent: int = 0
    format_cells: int = 0
    by_row: dict[str, int] = field(default_factory=dict)


def _probe_table(
    args: argparse.Namespace, http: str, tokens: dict[str, str | None], table: list[str]
) -> list[str]:
    """The Transports table's boundary probes, on the server itself: the self-test's proxy
    cannot carry a 64 KiB request line."""
    direct = Clients(Endpoints(http, args.flight))
    try:
        return documented.probe(direct, http, tokens, table)
    finally:
        direct.close()


def _probe_cells(
    args: argparse.Namespace,
    http: str,
    tokens: dict[str, str | None],
    found: list[probes.Probe],
) -> list[probes.ProbeCell]:
    """The private module's probe cells. Server variants need the binaries run.sh passes."""
    if not found:
        return []
    bins = (
        probes.Binaries(args.cli, args.server, args.work)
        if args.cli and args.server and args.work
        else None
    )
    ctx = probes.ProbeContext(
        Endpoints(http, args.flight), args.issuer, args.key, args.db, tokens, bins
    )
    return probes.run(ctx, found)


def _tally(
    graded: list[tuple[str, gen.Request, str, Response, Response]],
    table: list[str],
    principals: dict[str, Any],
    roots: dict[str, str],
) -> Tally:
    t = Tally(table, by_row=dict.fromkeys(table, 0))
    for fp, req, col, post, resp in graded:
        places = set(resp.code_at.split(",")) - {"", "extensions"}
        if places and col in GRAPHQL_COLUMNS:
            t.format_cells += 1
            for code in sorted(set(resp.codes) - {"-"}):
                t.formats.add(f"{col} {code} at {resp.code_at}")
        if col == "POST":
            continue
        ref, got = outcome(post), outcome(resp)
        issues = compare_cell(req, col, ref, got)
        t.per_col[col][1] += 1
        if not issues:
            t.per_col[col][0] += 1
            continue
        row = documented.matching(
            documented.CellView(
                req,
                col,
                ref,
                got,
                resp,
                req.who in principals and principals[req.who] is None,
                roots,
                issues,
            ),
            table,
        )
        if row:
            t.by_row[row] += 1
        else:
            t.divergent += 1
        if ref.cls != "OK" and got.rows:
            t.served_after_refusal += 1
        elif ref.cls == "OK" and not ref.rows and got.rows:
            t.served_after_empty += 1
        t.failing[f"{fp} {col} {','.join(issues)}"] = {
            "request": req.canon() | {"name": req.name},
            "column": col,
            "documented": row,
            "POST": post.__dict__,
            col: resp.__dict__,
        }
    return t


def measure(args: argparse.Namespace) -> tuple[dict[str, Any], dict[str, Any]]:
    priv = load_private()
    principals = {**gen.PRINCIPALS, **priv.principals}
    reqs = [r for s in SEEDS for r in gen.generate(PER_SEED, s, priv.corpus)] + priv.requests
    runner = Runner(
        Endpoints(args.http, args.flight), args.key, args.issuer, principals, priv.corpus
    )
    with ThreadPoolExecutor(WORKERS) as pool:
        answers = list(pool.map(runner.run, reqs))
    expired = runner.tokens["expired"]
    if expired is None:
        msg = "the expired principal has no token"
        raise RuntimeError(msg)
    # The self-test measures through a proxy, which reads every body; the connection
    # probe must still reach the server itself.
    protocol_http = getattr(args, "protocol_http", None) or args.http
    protocol = [line for line in [early_refusal(protocol_http, expired)] if line]
    ws_cells = _ws_column(args, protocol_http, runner.tokens, priv.corpus)
    table = documented.read_table()
    documented.check(table)
    held = _probe_table(args, protocol_http, runner.tokens, table)
    probe_cells = _probe_cells(args, protocol_http, runner.tokens, priv.probe_fns)
    retries = sum(c.retries_seen for c in runner.all_clients)
    for c in runner.all_clients:
        c.close()

    graded: list[tuple[str, gen.Request, str, Response, Response]] = []
    for req, res in zip(reqs, answers, strict=True):
        fp = fingerprint(req)
        graded += [(fp, req, col, res["POST"], resp) for col, resp in res.items()]
    graded += [(ws.fingerprint(c), c.request, "WS", c.ref, c.got) for c in ws_cells]
    graded += [(fingerprint(c.request), c.request, c.column, c.ref, c.got) for c in probe_cells]

    t = _tally(graded, table, principals, priv.corpus.roots)
    per_col, failing, by_row = t.per_col, t.failing, t.by_row
    cells = sum(t for _, t in per_col.values())
    agree = sum(a for a, _ in per_col.values())
    columns = columns_state(args.http)
    metrics = {
        "agreement": agree,
        "cells": cells,
        "refused_on_post_served_elsewhere": t.served_after_refusal,
        "empty_on_post_served_elsewhere": t.served_after_empty,
        "undocumented_divergences": t.divergent,
        "documented_differences": sum(by_row.values()),
        "documented_by_row": by_row,
        "error_format_cells": t.format_cells,
        "pending_columns": sum("pending" in line for line in columns),
        "per_column": {c: f"{a}/{t}" for c, (a, t) in per_col.items() if c != "POST"},
    }
    # Load-dependent, so reported and never gated: the protocol line gates the cause.
    print(f"connection retries after an unannounced close: {retries}")
    current = {
        "corpus": {
            "label": priv.label,
            "seeds": SEEDS,
            "per_seed": PER_SEED,
            "private_requests": len(priv.requests),
            "probe_cells": len(probe_cells),
        },
        "metrics": metrics,
        "disagreements": sorted(failing),
        "formats": sorted(t.formats),
        "columns": columns,
        "protocol": protocol,
        "documented": held,
    }
    return current, failing


LINES = [
    ("agreement with POST", "agreement", "cells", +1),
    ("refused on POST, served elsewhere", "refused_on_post_served_elsewhere", None, -1),
    ("no row on POST, served elsewhere", "empty_on_post_served_elsewhere", None, -1),
    ("undocumented divergences", "undocumented_divergences", None, -1),
    ("documented differences (Transports table)", "documented_differences", None, 0),
    ("cells with a code outside extensions", "error_format_cells", None, -1),
    ("pending columns", "pending_columns", None, -1),
]


def print_metrics(label: str, m: dict[str, Any], base: dict[str, Any] | None) -> None:
    print(f"cross-transport oracle ({label})")
    bm = (base or {}).get("metrics", {})
    for name, k, total, good in LINES:
        v = f"{m[k]}/{m[total]} ({100 * m[k] / m[total]:.1f}%)" if total and m[total] else f"{m[k]}"
        delta = ""
        if k in bm and bm[k] != m[k]:
            d = m[k] - bm[k]
            verdict = "changed" if good == 0 else "better" if (d > 0) == (good > 0) else "worse"
            delta = f"   {d:+d} vs baseline {verdict}"
        print(f"  {name + ':':40}{v}{delta}")
    print("  per column: " + ", ".join(f"{c} {v}" for c, v in m["per_column"].items()))
    print(
        "  documented, by row: " + ", ".join(f"{r} {n}" for r, n in m["documented_by_row"].items())
    )


def gate(current: dict[str, Any], base: dict[str, Any]) -> dict[str, Any]:
    if json.dumps(base.get("corpus"), sort_keys=True) != json.dumps(
        current["corpus"], sort_keys=True
    ):
        return {"ok": False, "corpus_changed": True}
    reg = {k: sorted(set(current[k]) - set(base.get(k, []))) for k in GATED}
    prog = {k: sorted(set(base.get(k, [])) - set(current[k])) for k in GATED}
    bm = base.get("metrics", {})
    reg["ratchets"] = [
        f"{m}: {current['metrics'][m]} > {bm.get(m, 0)}"
        for m in RATCHETED
        if current["metrics"][m] > bm.get(m, 0)
    ]
    prog["ratchets"] = [
        f"{m}: {current['metrics'][m]} < {bm.get(m, 0)}"
        for m in RATCHETED
        if current["metrics"][m] < bm.get(m, 0)
    ]
    ok = not any(reg.values()) and not any(prog.values())
    return {"ok": ok, "corpus_changed": False, "regressions": reg, "progress": prog}


def print_gate(g: dict[str, Any], failing: dict[str, Any]) -> None:
    if g["corpus_changed"]:
        print("FAIL: the corpus changed; re-bless the baseline")
        return
    reg, prog = g["regressions"], g["progress"]
    if reg["disagreements"]:
        print(f"\nREGRESSION: {len(reg['disagreements'])} cell(s) not in the baseline:")
        for k in reg["disagreements"][:SHOWN]:
            f = failing[k]
            col = f["column"]
            print(f"  {k}\n    request: {json.dumps(f['request'], ensure_ascii=False)[:400]}")
            for side in ("POST", col):
                r = f[side]
                data = json.dumps(r["data"])[:200]
                print(f"    {side:6} {r['status']} codes={r['codes']} data={data}")
        if len(reg["disagreements"]) > SHOWN:
            print(f"  ... and {len(reg['disagreements']) - SHOWN} more; --report writes every one")
    for k in ("formats", "columns", "protocol", "documented", "ratchets"):
        for line in reg[k]:
            print(f"REGRESSION: {k}: {line}")
    n = sum(len(v) for v in prog.values())
    if n:
        entries = "entry" if n == 1 else "entries"
        print(f"\nPROGRESS not yet recorded: {n} baseline {entries} no longer occur:")
        for k in GATE_KEYS:
            for line in prog[k][:SHOWN]:
                print(f"  {k}: {line}")
        print("Record it: rerun with --bless and commit baseline.json with the change.")
    nr = sum(len(v) for v in reg.values())
    print(
        "\nOK: every cell matches the baseline exactly"
        if g["ok"]
        else f"\nFAIL: {nr} regression(s), {n} unrecorded improvement(s)"
    )


def _only(g: dict[str, Any], side: str, key: str, expected: list[str]) -> bool:
    """The gate failed with exactly `expected` under `side`/`key`, and nothing else."""
    if g["ok"] or g["corpus_changed"] or key not in g[side]:
        return False
    return all(
        sorted(g[s].get(k, [])) == sorted(expected) if (s, k) == (side, key) else not g[s].get(k)
        for s in ("regressions", "progress")
        for k in GATE_KEYS
    )


Checks = list[bool]


def _check(checks: Checks, name: str, ok: bool, detail: str = "") -> None:
    checks.append(ok)
    print(f"{'PASS' if ok else 'FAIL'} {name}{': ' + detail if detail else ''}")


def _selftest_pure(checks: Checks) -> None:
    """Checks that hold whatever the server answers."""
    from .clients import Response, graphql_body  # noqa: PLC0415 - self-test only

    unknown = outcome(Response(status="200", codes=["SOMETHING_NEW"]))
    _check(
        checks,
        "an unknown code keeps its own class",
        unknown.cls == "ERR:SOMETHING_NEW",
        unknown.cls,
    )
    bodyless = outcome(graphql_body(401, "Token has expired", "orders"))
    _check(checks, "a bodyless 401 is UNAUTH by its status", bodyless.cls == "UNAUTH", bodyless.cls)
    req = gen.Request(
        name="t", root="orders", fields=["id", "secret"], who="A-viewer", hdr="", style="inline"
    )
    ref = outcome(Response(status="200", data=[{"id": "1", "secret": "s"}], has_data=True))
    mcp = outcome(Response(status="200", data=[{"id": "1"}], has_data=True))
    refused = outcome(Response(status="200", codes=["VALIDATION_ERROR"]))
    served = compare_cell(req, "REST", refused, ref)
    _check(
        checks,
        "a cell served where POST refused is a class divergence",
        served == ["class:INVALID->OK"],
        ",".join(served),
    )
    issues = compare_cell(req, "MCP", ref, mcp)
    sub = ws.Subscription("A-viewer", ["id"], events=[{"data": {"orderCreated": {"id": "r2"}}}])
    _check(
        checks,
        "a WS event for another row is not this row's event",
        not ws.answer(sub, "r1").has_data,
    )
    closed = ws.Subscription(
        "expired", ["id"], refusal=Response(status="ws", codes=["ws-close-4401"])
    )
    closed.events.append(sub.events[0])
    _check(
        checks,
        "a refused WS subscription answers its refusal, whatever it received",
        outcome(ws.answer(closed, "r2")).cls == "UNAUTH",
    )
    sub.events.append(sub.events[0])
    twice = outcome(ws.answer(sub, "r2"))
    _check(
        checks,
        "a WS row delivered twice differs from POST's one row",
        compare_cell(
            req, "WS", outcome(Response(status="200", data={"id": "r2"}, has_data=True)), twice
        )
        == ["data"],
    )
    _check(
        checks,
        "an MCP cell leaving a selected field out is counted",
        issues == ["omitted:secret"],
        ",".join(issues),
    )


def _selftest_documented(checks: Checks) -> None:
    """The Transports table: what the oracle reads from it, and what its judges accept."""
    from .clients import Response  # noqa: PLC0415 - self-test only

    table = documented.read_table()
    _check(checks, "the Transports table has rows the oracle can check", bool(table), str(table))
    try:
        documented.check([*table, "selftest-unknown-row"])
        unknown_fails = False
    except RuntimeError:
        unknown_fails = True
    _check(checks, "a table row the oracle cannot check stops the run", unknown_fails)

    rows = [{"id": "1"}]
    served = Response(status="OK", data=rows, has_data=True)
    honoured: dict[str, list[tuple[Response, Response]]] = {
        "flight-requires-auth": [
            (served, Response(status="x", codes=["flight:FlightUnauthenticatedError"]))
        ],
        "mcp-requires-auth": [(served, Response(status="401", codes=["http-401"]))],
        "url-length-cap": [(served, Response(status="414", codes=["http-414"]))] * 2,
        "rest-filter-cap": [(served, Response(status="400", codes=["BAD_REQUEST"]))],
        "flight-ticket-cap": [(served, Response(status="x", codes=["flight:ArrowInvalid"]))],
        "mcp-field-omission": [
            (served, Response(status="OK", data=[{"id": "1", "secret": "s"}], has_data=True))
        ],
    }
    for row in table:
        for n, (probe, (inside, outside)) in enumerate(
            zip(documented.PROBES[row], honoured[row], strict=True)
        ):
            ok = probe.judge(inside, outside) is None
            # One side at a time: a judge that checks only one side fails here.
            inside_moved = probe.judge(outside, outside)
            outside_moved = probe.judge(inside, inside)
            _check(
                checks,
                f"{row}#{n}: the row honoured passes; either side moved fails",
                ok and inside_moved is not None and outside_moved is not None,
                f"honoured: {probe.judge(inside, outside)}; inside moved: {inside_moved}; "
                f"outside moved: {outside_moved}",
            )

    def view(
        column: str, who: str, ref: Response, got: Response, ids: list[str] | None = None
    ) -> bool:
        req = gen.Request(
            name="t",
            root="orders",
            fields=["id", "secret", "note"],
            who=who,
            hdr="",
            style="inline",
        )
        a, b = outcome(ref), outcome(got)
        cell = documented.CellView(
            req, column, a, b, got, who == "anon", dict(gen.ROOTS), compare_cell(req, column, a, b)
        )
        return documented.matching(cell, table if ids is None else ids) is not None

    post = Response(status="OK", data=[{"id": "1", "secret": None, "note": "n"}], has_data=True)
    _check(
        checks,
        "MCP leaving out a field POST refuses is documented",
        view(
            "MCP",
            "A-viewer",
            Response(status="x", codes=["FORBIDDEN"]),
            Response(status="OK", data=[{"id": "1", "note": "n"}], has_data=True),
        ),
    )
    _check(
        checks,
        "MCP leaving out a field POST serves null is not",
        not view(
            "MCP",
            "A-viewer",
            post,
            Response(status="OK", data=[{"id": "1", "note": "n"}], has_data=True),
        ),
    )
    _check(
        checks,
        "MCP leaving out a field the caller may read is not",
        not view("MCP", "A-viewer", post, Response(status="OK", data=[{"id": "1"}], has_data=True)),
    )
    _check(
        checks,
        "MCP serving a field the caller may not read is not",
        not view(
            "MCP",
            "A-viewer",
            Response(status="x", codes=["FORBIDDEN"]),
            Response(status="OK", data=[{"id": "1", "secret": "s"}], has_data=True),
        ),
    )
    _check(
        checks,
        "a 414 on a short request target is not documented",
        not view("GET", "A-viewer", post, Response(status="414", codes=["http-414"])),
    )
    _check(
        checks,
        "Flight refusing an authenticated caller as unauthenticated is not documented",
        not view(
            "FLIGHT",
            "A-viewer",
            post,
            Response(status="x", codes=["flight:FlightUnauthenticatedError"]),
        ),
    )
    unauth = Response(status="x", codes=["flight:FlightUnauthenticatedError"])
    fewer = [r for r in table if r != "flight-requires-auth"]
    _check(
        checks,
        "a row removed from the table documents nothing",
        view("FLIGHT", "anon", post, unauth) and not view("FLIGHT", "anon", post, unauth, fewer),
    )


def _selftest_probes(checks: Checks, args: argparse.Namespace) -> None:
    """Probe cells are graded like corpus cells, and a server variant runs with its edits.

    The same introspection document is sent to the main server and to two variants: one
    unchanged, which must agree with main, and one with introspection disabled, which
    must not. A variant whose edit did not apply would agree, and fail here."""
    doc = "{ __schema { queryType { name } } }"
    req = gen.Request(
        name="selftest:introspection",
        root="__schema",
        fields=["queryType"],
        who="A-viewer",
        hdr="",
        style="inline",
    )

    def probe(ctx: probes.ProbeContext) -> list[probes.ProbeCell]:
        token = ctx.sign(ctx.claims(sub="alice", tenant_id="tA", roles=["viewer"]))
        main = ctx.clients().post(doc, None, token, {}, "__schema")
        cells = []
        for name, server in (("same", {}), ("closed", {"introspection_enabled": False})):
            with ctx.variant(f"selftest-{name}", server=server) as ends:
                got = ctx.clients(ends).post(doc, None, token, {}, "__schema")
            cells.append(probes.ProbeCell(dataclasses.replace(req, name=name), "POST", main, got))
        return cells

    same, closed = _probe_cells(args, args.http, {}, [probe])
    same_issues = compare_cell(same.request, "GET", outcome(same.ref), outcome(same.got))
    closed_issues = compare_cell(closed.request, "GET", outcome(closed.ref), outcome(closed.got))
    _check(
        checks,
        "an unchanged server variant agrees with the main server",
        bool(outcome(same.ref).rows) and not same_issues,
        ",".join(same_issues),
    )
    _check(
        checks,
        "a variant's edit applies: introspection disabled there differs from main",
        bool(closed_issues),
        f"{closed.got.status} {closed.got.codes}",
    )
    try:
        probes._set_keys("a = 1\n[s]\nb = 2\n", {"b": 3})
        unknown = False
    except RuntimeError:
        unknown = True
    _check(
        checks, "a server setting the template does not have at top level stops a variant", unknown
    )


def _selftest_sites(checks: Checks, current: dict[str, Any], base: dict[str, Any]) -> None:
    """Every gate site fails on a one-entry change to the baseline, in both directions.

    The sites are the baseline's own lists, not GATED: a list the gate stopped reading
    must fail here, not drop out of the self-test."""
    sites = [k for k, v in base.items() if isinstance(v, list)]
    _check(
        checks,
        "the baseline has the five gated lists",
        sorted(sites) == ["columns", "disagreements", "documented", "formats", "protocol"],
        str(sites),
    )
    for k in sites:
        # A new entry in the run that the baseline does not hold. Added to the run, not
        # removed from the baseline, so a list that is empty when all is well is checked too.
        fake = f"selftest-{k}-entry"
        g = gate({**current, k: [*current[k], fake]}, base)
        _check(
            checks,
            f"{k}: an entry the baseline does not hold is a regression",
            _only(g, "regressions", k, [fake]),
        )
        g = gate(current, {**base, k: [*base.get(k, []), fake]})
        _check(
            checks,
            f"{k}: a baseline entry that no longer occurs is unrecorded progress",
            _only(g, "progress", k, [fake]),
        )
    for m in RATCHETED:
        value = current["metrics"][m]
        g = gate(current, {**base, "metrics": {**base["metrics"], m: value - 1}})
        _check(
            checks,
            f"{m}: rising above the baseline is a regression",
            _only(g, "regressions", "ratchets", [f"{m}: {value} > {value - 1}"]),
        )
        g = gate(current, {**base, "metrics": {**base["metrics"], m: value + 1}})
        _check(
            checks,
            f"{m}: falling below the baseline is unrecorded progress",
            _only(g, "progress", "ratchets", [f"{m}: {value} < {value + 1}"]),
        )
        missing = {**base, "metrics": {k: v for k, v in base["metrics"].items() if k != m}}
        g = gate({**current, "metrics": {**current["metrics"], m: value + 1}}, missing)
        _check(
            checks,
            f"{m}: a baseline without the value ratchets at 0",
            value + 1 > 0
            and "ratchets" in g.get("regressions", {})
            and g["regressions"]["ratchets"] == [f"{m}: {value + 1} > 0"],
        )
    other = {**base, "corpus": {**base["corpus"], "seeds": [*base["corpus"]["seeds"], 9]}}
    _check(checks, "a changed corpus fails", gate(current, other)["corpus_changed"])


def _proxied(
    args: argparse.Namespace, base: dict[str, Any], rewrite: Any, column: str
) -> dict[str, Any]:
    """The run through a proxy rewriting `column`'s responses: which cells changed, and how."""
    proxy = Proxy(args.http, rewrite)
    try:
        changed = _changed(args, base, column, http=proxy.url, protocol_http=args.http)
    finally:
        proxy.close()
    return {**changed, "rewrites": proxy.rewrites}


def _changed(
    args: argparse.Namespace, base: dict[str, Any], column: str, **overrides: Any
) -> dict[str, Any]:
    """A run with `overrides` on the arguments: which cells changed, and how."""
    m, _ = measure(argparse.Namespace(**{**vars(args), **overrides}))
    g = gate(m, base)
    regs = [] if g["corpus_changed"] else g["regressions"]["disagreements"]
    moved = [] if g["corpus_changed"] else g["progress"]["disagreements"]
    reg_fps = {k.split(" ")[0] for k in regs}
    others = [k for k in GATED if k != "disagreements"]
    ratchets = (
        {} if g["corpus_changed"] else {s: g[s]["ratchets"] for s in ("regressions", "progress")}
    )
    return {
        "regs": regs,
        "columns": {k.split(" ")[1] for k in regs},
        "issues": {k.split(" ", 2)[2] for k in regs},
        # Nothing regresses outside the disagreements; anything else that changed is
        # progress on the rewritten column (a rewritten refusal loses its error format);
        # and a baseline cell's key moves only on its own request.
        "quiet": not g["corpus_changed"]
        and all(not g["regressions"][k] for k in others)
        and all(e.startswith(f"{column} ") for k in others for e in g["progress"][k])
        and all(k.split(" ")[0] in reg_fps for k in moved),
        "noise": {s: {k: g[s][k] for k in others if g[s][k]} for s in ("regressions", "progress")}
        if not g["corpus_changed"]
        else "corpus changed",
        "metrics": m["metrics"],
        "ratchets": ratchets,
    }


class _Fabricate:
    """Every live B-viewer subscription receives tenant A's row, which POST does not serve
    it. `injected` counts them: a subscription refused as POST refuses gets nothing."""

    def __init__(self) -> None:
        self.injected = 0

    def __call__(self, subs: list[ws.Subscription], rows: dict[str, str]) -> None:
        for s in subs:
            if s.who == "B-viewer" and s.refusal is None:
                row = dict.fromkeys(s.fields, "fabricated") | {"id": rows["A-viewer"]}
                s.events.append({"data": {"orderCreated": row}})
                self.injected += 1


def _ws_drop(subs: list[ws.Subscription], rows: dict[str, str]) -> None:
    """No subscription receives anything."""
    del rows
    for s in subs:
        s.events.clear()


def _selftest_ws(checks: Checks, args: argparse.Namespace, base: dict[str, Any]) -> None:
    key = "empty_on_post_served_elsewhere"
    fabricate = _Fabricate()
    r = _changed(args, base, "WS", ws_tamper=fabricate)
    rise = r["metrics"][key] - base["metrics"][key]
    cells = fabricate.injected
    _check(
        checks,
        "a WS event for a row POST does not serve is reported on WS only",
        cells > 0
        and r["columns"] == {"WS"}
        and len(r["regs"]) == cells
        and rise == cells
        and r["ratchets"]["regressions"] == [f"{key}: {cells} > 0"]
        and r["quiet"],
        f"{cells} injected, {len(r['regs'])} regression(s) on {sorted(r['columns'])}, "
        f"no row on POST +{rise}",
    )
    r = _changed(args, base, "WS", ws_tamper=_ws_drop)
    _check(
        checks,
        "WS events that stop arriving are reported on WS only, as data",
        len(r["regs"]) > 0 and r["columns"] == {"WS"} and r["issues"] == {"data"} and r["quiet"],
        f"{len(r['regs'])} regression(s) on {sorted(r['columns'])}, issues {sorted(r['issues'])}",
    )


def _serve_rest_refusals(method: str, path: str, status: int, body: bytes) -> Any:
    del method, body
    if path.startswith("/rest/v1/") and status in {400, 401, 403}:
        return 200, json.dumps({"data": [{"id": "served-by-the-proxy"}]}).encode()
    return None


def _alter_get(method: str, path: str, status: int, body: bytes) -> Any:
    if method == "GET" and path.startswith("/graphql?") and b'"Order 1"' in body:
        return status, body.replace(b'"Order 1"', b'"Order 1 (altered)"')
    return None


def selftest(args: argparse.Namespace, base: dict[str, Any]) -> int:
    """Proves the gate goes RED, against a server that matches the baseline.

    - Pure: the classifier keeps an unknown code's name, takes a bodyless response's
      status as its code, and the MCP rule counts an omitted field.
    - The Transports table: a row the oracle cannot check stops the run; each row's judge
      passes the row honoured and fails its boundary moved; a matcher documents only its
      own difference, and nothing once its row is removed.
    - Probe cells: an unchanged server variant agrees with main, and one with
      introspection disabled does not, so a variant's edits apply.
    - Every gate site fails when the baseline is changed by one entry, in both directions,
      and names that entry; a changed corpus fails.
    - A proxy makes REST serve rows where it refused: only REST cells are reported, and
      "refused on POST, served elsewhere" rises.
    - A proxy alters one value on GET: only GET cells are reported, each as data.
    - The WS column's events are tampered with (the proxy carries no WebSockets): a row
      POST does not serve, delivered, and events that stop arriving, are each reported
      on WS only.
    """
    checks: Checks = []
    _selftest_pure(checks)
    _selftest_documented(checks)
    current, _ = measure(args)
    if not gate(current, base)["ok"]:
        msg = "the self-test needs a server that matches the baseline; run the oracle first"
        raise RuntimeError(msg)
    _check(checks, "unchanged server matches the baseline", True)
    _selftest_sites(checks, current, base)
    _selftest_probes(checks, args)

    key = "refused_on_post_served_elsewhere"
    r = _proxied(args, base, _serve_rest_refusals, "REST")
    rise = r["metrics"][key] - base["metrics"][key]
    _check(
        checks,
        "REST serving rows it refused is reported on REST only, as served elsewhere",
        r["rewrites"] > 0 and r["columns"] == {"REST"} and rise > 0 and r["quiet"],
        f"{r['rewrites']} rewrites, {len(r['regs'])} regression(s) on {sorted(r['columns'])}, "
        f"served elsewhere +{rise}" + ("" if r["quiet"] else f"; other changes {r['noise']}"),
    )
    _selftest_ws(checks, args, base)
    r = _proxied(args, base, _alter_get, "GET")
    _check(
        checks,
        "a wrong value on GET is reported on GET only, as data",
        r["rewrites"] > 0
        and r["columns"] == {"GET"}
        and all("data" in i.split(",") for i in r["issues"])
        and r["quiet"],
        f"{r['rewrites']} rewrites, {len(r['regs'])} regression(s) on {sorted(r['columns'])}, "
        f"issues {sorted(r['issues'])}",
    )
    failed = checks.count(False)
    print(
        f"\nSELFTEST FAIL: {failed} of {len(checks)} checks"
        if failed
        else f"\nSELFTEST OK: {len(checks)} checks"
    )
    return 1 if failed else 0


def main() -> int:
    p = argparse.ArgumentParser(prog="oracle")
    p.add_argument("--http", required=True)
    p.add_argument("--flight", required=True)
    p.add_argument("--issuer", required=True)
    p.add_argument("--key", type=Path, required=True)
    p.add_argument("--db", required=True, help="the fixture database; the WS column writes rows")
    p.add_argument("--cli", type=Path, help="fraiseql-cli, for server variants")
    p.add_argument("--server", type=Path, help="fraiseql-server, for server variants")
    p.add_argument("--work", type=Path, help="a scratch directory for server variants")
    p.add_argument("--baseline", type=Path, default=HERE / "baseline.json")
    p.add_argument("--report", type=Path)
    p.add_argument("--bless", action="store_true")
    p.add_argument("--selftest", action="store_true")
    args = p.parse_args()

    if args.selftest:
        return selftest(args, json.loads(args.baseline.read_text()))
    current, failing = measure(args)
    base = json.loads(args.baseline.read_text()) if args.baseline.exists() else None
    print_metrics(current["corpus"]["label"], current["metrics"], base)
    if args.report:
        args.report.write_text(
            json.dumps({**current, "cells": failing}, indent=1, ensure_ascii=False, default=str)
            + "\n"
        )
    if args.bless:
        args.baseline.write_text(json.dumps(current, indent=1, ensure_ascii=False) + "\n")
        print(f"blessed {args.baseline}")
        return 0
    if base is None:
        print(f"FAIL: no baseline at {args.baseline}; run with --bless to record one")
        return 1
    g = gate(current, base)
    print_gate(g, failing)
    return 0 if g["ok"] else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as e:  # any harness failure is exit 2, never a pass
        import traceback

        traceback.print_exception(e)
        print(f"oracle harness error: {e}", file=sys.stderr)
        sys.exit(2)
