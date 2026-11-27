"""The seeded generator of logical requests, and their rendering per transport.

A logical request names a root, a selection, and optionally a filter, an order, a page, a
principal and a header. `render` spells it for each transport. A transport with no
spelling for it gets no cell: `applicable` decides that from the request's shape alone,
never from an outcome.
"""

from __future__ import annotations

import json
import random
from dataclasses import dataclass, field, replace
from typing import Any

# Principals, as a generic dimension: every one is rendered on every transport.
# name -> (sub, tenant claim, roles, seconds to expiry, audience) or None for no token.
PRINCIPALS: dict[str, tuple[str, str | None, list[str] | None, int, str | None] | None] = {
    "A-viewer": ("alice", "tA", ["viewer"], 600, None),
    "B-viewer": ("bob", "tB", ["viewer"], 600, None),
    "A-norole": ("carol", "tA", None, 600, None),
    "A-auditor": ("dan", "tA", ["auditor"], 600, None),
    "B-admin": ("erin", "tB", ["admin"], 600, None),
    "notenant": ("frank", None, ["viewer"], 600, None),
    "expired": ("gina", "tA", ["admin"], -300, None),
    "wrongaud": ("hal", "tA", ["admin"], 600, "another-audience"),
    "anon": None,
}

# root -> REST path ("{id}" for the single-row root)
ROOTS = {
    "orders": "/orders",
    "rlsOrders": "/rls-orders",
    "adminOrders": "/admin-orders",
    "customers": "/customers",
    "order": "/orders/{id}",
}
ORDER_FIELDS = [
    "id",
    "tenantId",
    "fkCustomer",
    "label",
    "amount",
    "big",
    "ratio",
    "createdAt",
    "status",
    "secret",
    "note",
    "seenTenant",
    "seenSub",
]
CUSTOMER_FIELDS = ["id", "tenantId", "name", "email"]
# Fields a scope gates. The generator selects them like any field, and filters and orders
# on the other fields; ORACLE_PRIVATE_MODULE may add filters and ordering of its own.
GATED_FIELDS = {"secret", "note", "email"}
FILTERS: list[tuple[str, str, list[Any]]] = [
    ("label", "eq", ["Order 1", 'Order 7 ünï "q" \\ ☃', "Order+1", "nope"]),
    ("label", "startswith", ["Order 1", "Order"]),
    ("label", "icontains", ["ORDER 2", "ünï"]),
    ("status", "eq", ["SHIPPED", "PENDING"]),
    ("amount", "gt", ["1", "10.5", "-2"]),
    ("big", "eq", ["9007199254740993", "1"]),
    ("big", "lt", ["0"]),
    ("ratio", "gte", [0.3, 1e300]),
    ("tenantId", "eq", ["tB", "tA"]),
    (
        "id",
        "in",
        [["00000001-1111-4000-8000-000000000000", "00000003-1111-4000-8000-000000000000"]],
    ),
    ("createdAt", "gt", ["2024-03-31T20:00:00+02:00", "2024-03-31T20:00:00Z"]),
    ("name", "eq", ["Cust 2"]),
]
IDS = [
    "00000001-1111-4000-8000-000000000000",
    "00000003-1111-4000-8000-000000000000",
    "00000005-1111-4000-8000-000000000000",
    "ffffffff-1111-4000-8000-000000000000",
]
# GraphQL-only shapes: no REST or MCP spelling.
GRAPHQL_ONLY = {
    "skip_gated",
    "include_false_gated",
    "skip_var",
    "typename",
    "field_alias",
    "nested",
    "unknown",
    "dup",
}
EXTRAS = [
    None,
    None,
    None,
    "skip_gated",
    "include_false_gated",
    "typename",
    "field_alias",
    "nested",
    "unknown",
    "dup",
    "huge_filter",
    "skip_var",
]
COLUMNS = ["POST", "GET", "SSE", "REST", "MCP", "FLIGHT"]


@dataclass(frozen=True)
class Corpus:
    """What the generator draws from. `PUBLIC` is the committed corpus;
    ORACLE_PRIVATE_MODULE may extend it (see `__main__.load_private`)."""

    roots: dict[str, str] = field(default_factory=lambda: dict(ROOTS))
    order_fields: list[str] = field(default_factory=lambda: list(ORDER_FIELDS))
    filters: list[tuple[str, str, list[Any]]] = field(default_factory=lambda: list(FILTERS))
    order_gated: bool = False
    # Values sent in the `x-probe` header; none means no header is sent.
    headers: list[str] = field(default_factory=list)
    # Extra selections for the WS column's subscriptions (oracle.ws.SELECTIONS always run).
    ws_selections: list[list[str]] = field(default_factory=list)

    def extended(self, **extra: Any) -> Corpus:
        return replace(
            self,
            roots={**self.roots, **extra.get("ROOTS", {})},
            order_fields=self.order_fields + list(extra.get("ORDER_FIELDS", [])),
            filters=self.filters + list(extra.get("FILTERS", [])),
            order_gated=bool(extra.get("ORDER_GATED", self.order_gated)),
            headers=self.headers + list(extra.get("HEADERS", [])),
            ws_selections=self.ws_selections + list(extra.get("WS_SELECTIONS", [])),
        )


PUBLIC = Corpus()


@dataclass
class Request:
    name: str
    root: str
    fields: list[str]
    who: str
    hdr: str  # the `x-probe` header's value; empty: no header
    style: str
    extra: str | None = None
    id: str | None = None
    limit: int | None = None
    offset: int | None = None
    where: dict[str, Any] | None = None
    order: tuple[str, str] | None = None

    def canon(self) -> dict[str, Any]:
        d = {k: v for k, v in self.__dict__.items() if k != "name"}
        d["order"] = list(self.order) if self.order else None
        return d


def generate(count: int, seed: int, corpus: Corpus = PUBLIC) -> list[Request]:
    """`count` requests from `seed`. A pure function of its arguments."""
    rng = random.Random(seed)
    out = []
    for i in range(count):
        root = rng.choice(list(corpus.roots))
        pool = CUSTOMER_FIELDS if root == "customers" else corpus.order_fields
        fields = rng.sample(pool, rng.randint(1, min(5, len(pool))))
        if "id" not in fields:
            fields.insert(0, "id")
        r = Request(
            name=f"fuzz:{seed}:{i}",
            root=root,
            fields=fields,
            who=rng.choice(list(PRINCIPALS)),
            hdr=rng.choice(corpus.headers) if corpus.headers else "",
            style="",
        )
        if root == "order":
            r.id = rng.choice(IDS)
        else:
            r.limit = rng.choice([None, 1, 3, 10, 25, 26, 1000])
            r.offset = rng.choice([None, None, 0, 2, 30])
            if rng.random() < 0.6:
                name, op, values = rng.choice([f for f in corpus.filters if f[0] in pool])
                r.where = {name: {op: rng.choice(values)}}
            if rng.random() < 0.4:
                orderable = (
                    pool if corpus.order_gated else [f for f in pool if f not in GATED_FIELDS]
                )
                r.order = (rng.choice(orderable), rng.choice(["ASC", "DESC"]))
        r.style = rng.choice(["inline", "vars", "fragment", "alias"])
        r.extra = rng.choice(EXTRAS)
        if r.extra == "huge_filter" and root != "order":
            r.where = {"label": {"eq": "x" * rng.choice([1000, 30000, 100000])}}
        out.append(r)
    return out


def applicable(r: Request) -> list[str]:
    """The columns this request has a spelling on."""
    cols = ["POST", "GET", "SSE", "FLIGHT"]
    if r.extra not in GRAPHQL_ONLY:
        cols[3:3] = ["REST", "MCP"]
    return cols


def _literal(v: Any) -> str:
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, (int, float)):
        return repr(v)
    if isinstance(v, list):
        return "[" + ", ".join(_literal(x) for x in v) + "]"
    return json.dumps(v)


EXTRA_SELECTION = {
    "skip_gated": " secret @skip(if: true) note @skip(if: true)",
    "include_false_gated": " secret @include(if: false)",
    "skip_var": " secret @skip(if: $s)",
    "field_alias": " zz: label yy: id",
    "unknown": " nonexistentField",
}


def _selection(r: Request, vdefs: list[str], variables: dict[str, Any]) -> str:
    sel = " ".join(r.fields)
    if r.extra in EXTRA_SELECTION:
        sel += EXTRA_SELECTION[r.extra]
    if r.extra == "skip_var":
        vdefs.append("$s: Boolean!")
        variables["s"] = True
    elif r.extra == "typename":
        sel = "__typename " + sel
    elif r.extra == "nested" and r.root != "customers":
        sel += " customer { id name }"
    elif r.extra == "dup":
        sel += " " + sel
    return sel


def _bind(
    r: Request,
    *,
    name: str,
    gql_type: str,
    value: Any,
    literal: str,
    vdefs: list[str],
    variables: dict[str, Any],
) -> str:
    """An argument value: a variable in the `vars` style, the literal otherwise."""
    if r.style != "vars":
        return literal
    vdefs.append(f"${name}: {gql_type}")
    variables[name] = value
    return f"${name}"


def _arguments(r: Request, vdefs: list[str], variables: dict[str, Any]) -> list[str]:
    def bind(name: str, gql_type: str, value: Any, literal: str) -> str:
        return _bind(
            r,
            name=name,
            gql_type=gql_type,
            value=value,
            literal=literal,
            vdefs=vdefs,
            variables=variables,
        )

    if r.root == "order":
        return [f"id: {bind('id', 'ID!', r.id, json.dumps(r.id))}"]
    args: list[str] = []
    if r.limit is not None:
        args.append(f"limit: {bind('l', 'Int', r.limit, str(r.limit))}")
    if r.offset is not None:
        args.append(f"offset: {r.offset}")
    if r.where:
        ((field, cond),) = r.where.items()
        ((op, v),) = cond.items()
        tname = "Customer" if r.root == "customers" else "Order"
        lit = v if field == "status" and op == "eq" else _literal(v)
        literal = f"{{{field}: {{{op}: {lit}}}}}"
        args.append(f"where: {bind('w', f'{tname}WhereInput', r.where, literal)}")
    if r.order:
        args.append(f'orderBy: [{{field: "{r.order[0]}", direction: {r.order[1]}}}]')
    return args


def graphql(r: Request) -> tuple[str, dict[str, Any] | None, str]:
    """(document, variables, response key)."""
    vdefs: list[str] = []
    variables: dict[str, Any] = {}
    sel = _selection(r, vdefs, variables)
    args = _arguments(r, vdefs, variables)
    a = f"({', '.join(args)})" if args else ""
    defs = f"({', '.join(vdefs)})" if vdefs else ""
    key = r.root
    if r.style == "fragment":
        tname = "Customer" if r.root == "customers" else "Order"
        doc = f"query{defs} {{ {r.root}{a} {{ ...F }} }} fragment F on {tname} {{ {sel} }}"
    elif r.style == "alias":
        key = f"x_{r.root}"
        doc = f"query{defs} {{ {key}: {r.root}{a} {{ {sel} }} }}"
    else:
        doc = f"query{defs} {{ {r.root}{a} {{ {sel} }} }}"
    return doc, variables or None, key


def rest(r: Request, roots: dict[str, str]) -> tuple[str, dict[str, Any]]:
    path = roots[r.root].replace("{id}", r.id or "")
    params: dict[str, Any] = {"select": ",".join(r.fields)}
    if r.limit is not None:
        params["limit"] = r.limit
    if r.offset is not None:
        params["offset"] = r.offset
    if r.where:
        params["filter"] = json.dumps(r.where)
    if r.order:
        params["sort"] = ("-" if r.order[1] == "DESC" else "") + r.order[0]
    return path, params


def mcp(r: Request) -> dict[str, Any]:
    if r.root == "order":
        return {"id": r.id}
    args: dict[str, Any] = {}
    if r.limit is not None:
        args["limit"] = r.limit
    if r.offset is not None:
        args["offset"] = r.offset
    if r.where:
        args["where"] = r.where
    if r.order:
        args["orderBy"] = [{"field": r.order[0], "direction": r.order[1]}]
    return args
