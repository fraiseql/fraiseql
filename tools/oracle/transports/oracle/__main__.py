"""Cross-transport differential oracle.

Each seeded logical request (oracle/generate.py) is sent over every transport it has a
spelling on, and every cell is compared with POST /graphql, the reference column. POST is
itself graded against graphql-js by tools/oracle/graphql-js, so agreement with POST is
agreement with the spec, transitively.

Metrics (README.md):
  - agreement: cells agreeing with POST, over all compared cells;
  - refused on POST, served elsewhere: POST refuses, another transport returns data;
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
import importlib.util
import json
import os
import sys
import threading
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import httpx

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
GATED = ["disagreements", "formats", "columns", "protocol"]


@dataclass
class Private:
    requests: list[gen.Request]
    principals: dict[str, Any]
    corpus: gen.Corpus
    label: str


EXTENSIONS = ("ROOTS", "ORDER_FIELDS", "FILTERS", "ORDER_GATED", "HEADERS")


def load_private() -> Private:
    """ORACLE_PRIVATE_MODULE, a Python file kept outside the repository, may export
    REQUESTS (extra requests, as dicts of Request fields) and PRINCIPALS, and extend the
    generator's corpus with ROOTS, ORDER_FIELDS, FILTERS, ORDER_GATED and HEADERS."""
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
    retries = sum(c.retries_seen for c in runner.all_clients)
    for c in runner.all_clients:
        c.close()

    per_col: dict[str, list[int]] = {c: [0, 0] for c in gen.COLUMNS}
    failing: dict[str, Any] = {}
    formats: set[str] = set()
    served_after_refusal = 0
    divergent = 0
    format_cells = 0
    for req, res in zip(reqs, answers, strict=True):
        fp = fingerprint(req)
        ref = outcome(res["POST"])
        for col, resp in res.items():
            places = set(resp.code_at.split(",")) - {"", "extensions"}
            if places:
                format_cells += 1
                for code in sorted(set(resp.codes) - {"-"}):
                    formats.add(f"{col} {code} at {resp.code_at}")
            if col == "POST":
                continue
            got = outcome(resp)
            issues = compare_cell(req, col, ref, got)
            per_col[col][1] += 1
            if not issues:
                per_col[col][0] += 1
                continue
            divergent += 1
            if ref.cls != "OK" and got.rows:
                served_after_refusal += 1
            failing[f"{fp} {col} {','.join(issues)}"] = {
                "request": req.canon() | {"name": req.name},
                "column": col,
                "POST": res["POST"].__dict__,
                col: resp.__dict__,
            }
    cells = sum(t for _, t in per_col.values())
    agree = sum(a for a, _ in per_col.values())
    columns = columns_state(args.http)
    metrics = {
        "agreement": agree,
        "cells": cells,
        "refused_on_post_served_elsewhere": served_after_refusal,
        "undocumented_divergences": divergent,
        "error_format_cells": format_cells,
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
        },
        "metrics": metrics,
        "disagreements": sorted(failing),
        "formats": sorted(formats),
        "columns": columns,
        "protocol": protocol,
    }
    return current, failing


LINES = [
    ("agreement with POST", "agreement", "cells", +1),
    ("refused on POST, served elsewhere", "refused_on_post_served_elsewhere", None, -1),
    ("undocumented divergences", "undocumented_divergences", None, -1),
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
            delta = f"   {d:+d} vs baseline {'better' if (d > 0) == (good > 0) else 'worse'}"
        print(f"  {name + ':':40}{v}{delta}")
    print("  per column: " + ", ".join(f"{c} {v}" for c, v in m["per_column"].items()))


def gate(current: dict[str, Any], base: dict[str, Any]) -> dict[str, Any]:
    if json.dumps(base.get("corpus"), sort_keys=True) != json.dumps(
        current["corpus"], sort_keys=True
    ):
        return {"ok": False, "corpus_changed": True}
    reg = {k: sorted(set(current[k]) - set(base.get(k, []))) for k in GATED}
    prog = {k: sorted(set(base.get(k, [])) - set(current[k])) for k in GATED}
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
    for k in ("formats", "columns", "protocol"):
        for line in reg[k]:
            print(f"REGRESSION: {k}: {line}")
    n = sum(len(v) for v in prog.values())
    if n:
        entries = "entry" if n == 1 else "entries"
        print(f"\nPROGRESS not yet recorded: {n} baseline {entries} no longer occur:")
        for k in GATED:
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
    if g["ok"] or g["corpus_changed"]:
        return False
    return all(
        sorted(g[s][k]) == sorted(expected) if (s, k) == (side, key) else not g[s][k]
        for s in ("regressions", "progress")
        for k in GATED
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
    _check(
        checks,
        "an MCP cell leaving a selected field out is counted",
        issues == ["omitted:secret"],
        ",".join(issues),
    )


def _selftest_sites(checks: Checks, current: dict[str, Any], base: dict[str, Any]) -> None:
    """Every gate site fails on a one-entry change to the baseline, in both directions.

    The sites are the baseline's own lists, not GATED: a list the gate stopped reading
    must fail here, not drop out of the self-test."""
    sites = [k for k, v in base.items() if isinstance(v, list)]
    _check(
        checks,
        "the baseline has the four gated lists",
        sorted(sites) == ["columns", "disagreements", "formats", "protocol"],
        str(sites),
    )
    for k in sites:
        if base.get(k):
            entry = base[k][0]
            g = gate(current, {**base, k: base[k][1:]})
            _check(
                checks,
                f"{k}: an entry missing from the baseline is a regression",
                _only(g, "regressions", k, [entry]),
            )
        else:
            _check(checks, f"{k}: the baseline has an entry to remove", False, "empty")
        fake = f"selftest-{k}-entry"
        g = gate(current, {**base, k: [*base.get(k, []), fake]})
        _check(
            checks,
            f"{k}: a baseline entry that no longer occurs is unrecorded progress",
            _only(g, "progress", k, [fake]),
        )
    other = {**base, "corpus": {**base["corpus"], "seeds": [*base["corpus"]["seeds"], 9]}}
    _check(checks, "a changed corpus fails", gate(current, other)["corpus_changed"])


def _proxied(
    args: argparse.Namespace, base: dict[str, Any], rewrite: Any, column: str
) -> dict[str, Any]:
    """The run through a proxy rewriting `column`'s responses: which cells changed, and how."""
    proxy = Proxy(args.http, rewrite)
    try:
        m, _ = measure(
            argparse.Namespace(**{**vars(args), "http": proxy.url, "protocol_http": args.http})
        )
    finally:
        proxy.close()
    g = gate(m, base)
    regs = [] if g["corpus_changed"] else g["regressions"]["disagreements"]
    moved = [] if g["corpus_changed"] else g["progress"]["disagreements"]
    reg_fps = {k.split(" ")[0] for k in regs}
    others = [k for k in GATED if k != "disagreements"]
    return {
        "rewrites": proxy.rewrites,
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
    }


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
    - Every gate site fails when the baseline is changed by one entry, in both directions,
      and names that entry; a changed corpus fails.
    - A proxy makes REST serve rows where it refused: only REST cells are reported, and
      "refused on POST, served elsewhere" rises.
    - A proxy alters one value on GET: only GET cells are reported, each as data.
    """
    checks: Checks = []
    _selftest_pure(checks)
    current, _ = measure(args)
    if not gate(current, base)["ok"]:
        msg = "the self-test needs a server that matches the baseline; run the oracle first"
        raise RuntimeError(msg)
    _check(checks, "unchanged server matches the baseline", True)
    _selftest_sites(checks, current, base)

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
