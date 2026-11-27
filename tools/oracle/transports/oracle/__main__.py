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
    protocol = [line for line in [early_refusal(args.http, expired)] if line]
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


def main() -> int:
    p = argparse.ArgumentParser(prog="oracle")
    p.add_argument("--http", required=True)
    p.add_argument("--flight", required=True)
    p.add_argument("--issuer", required=True)
    p.add_argument("--key", type=Path, required=True)
    p.add_argument("--baseline", type=Path, default=HERE / "baseline.json")
    p.add_argument("--report", type=Path)
    p.add_argument("--bless", action="store_true")
    args = p.parse_args()

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
