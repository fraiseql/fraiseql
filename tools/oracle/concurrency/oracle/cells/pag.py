"""Relay keyset pagination while rows are inserted and deleted.

3 writers insert and delete account 50's items while one reader per order walks every
page with `first`/`after`. A row that existed for the whole walk (its insert had
completed before the walk began, and its delete was not sent before the walk ended) must
appear exactly once in the walk, and no row may appear twice.

Offset pagination is not judged: each page is its own statement at READ COMMITTED, so
offset walks skip and repeat rows under writes by design.
"""

from __future__ import annotations

import asyncio
import collections
import os
import random
import time
from typing import TYPE_CHECKING, Any

from oracle import db
from oracle.cells import Outcome, VacuousError
from oracle.client import AsyncClient, account_id

if TYPE_CHECKING:
    from oracle.run import Run

ACCOUNT = account_id(50)
PAGE = 17
SECS = float(os.environ.get("ORACLE_PAG_SECS", "10"))
ORDERS = {
    "pk": "",
    "seq_desc": ', orderBy: [{ field: "seq", direction: DESC }]',
    "seq_asc": ', orderBy: [{ field: "seq", direction: ASC }]',
}
CREATE = "mutation($a: ID!, $s: Int!) { createItem(fkAccount: $a, seq: $s) { id } }"
DELETE = "mutation($i: ID!) { deleteItem(id: $i) { id } }"


class _Rows:
    """When each row's insert completed and when its delete was sent."""

    def __init__(self, initial: list[str]) -> None:
        self.born: dict[str, float] = dict.fromkeys(initial, 0.0)
        self.died: dict[str, float] = {}
        self.live = list(initial)

    def stable(self, start: float, end: float) -> list[str]:
        return [
            i
            for i, b in self.born.items()
            if b < start and not (i in self.died and self.died[i] < end)
        ]


async def _writer(i: int, c: AsyncClient, rows: _Rows, stop: float) -> int:
    rng = random.Random(100 + i)
    writes = 0
    while time.monotonic() < stop:
        if rng.random() < 0.5 or len(rows.live) < 50:
            _, body = await c.gql(CREATE, {"a": ACCOUNT, "s": rng.randrange(50)})
            d = (body.get("data") or {}).get("createItem")
            if d:
                rows.born[d["id"]] = time.monotonic()
                rows.live.append(d["id"])
                writes += 1
        else:
            victim = rows.live.pop(rng.randrange(len(rows.live)))
            rows.died[victim] = time.monotonic()
            await c.gql(DELETE, {"i": victim})
            writes += 1
    return writes


async def _walk(c: AsyncClient, order: str) -> tuple[list[str], str | None]:
    seen: list[str] = []
    after = ""
    while True:
        cursor = f', after: "{after}"' if after else ""
        _, body = await c.gql(
            f"{{ itemsConn(first: {PAGE}{cursor}{ORDERS[order]}, "
            f'where: {{ fkAccount: {{ eq: "{ACCOUNT}" }} }}) '
            "{ edges { node { id } } pageInfo { hasNextPage endCursor } } }"
        )
        if body.get("errors") or not body.get("data"):
            return seen, str(body.get("errors"))[:200]
        conn = body["data"]["itemsConn"]
        seen += [e["node"]["id"] for e in conn["edges"]]
        if not conn["pageInfo"]["hasNextPage"]:
            return seen, None
        after = conn["pageInfo"]["endCursor"]


async def _reader(c: AsyncClient, order: str, rows: _Rows, stop: float) -> dict[str, Any]:
    t: collections.Counter[str] = collections.Counter()
    examples: list[str] = []
    while time.monotonic() < stop:
        start = time.monotonic()
        seen, err = await _walk(c, order)
        end = time.monotonic()
        t["walks"] += 1
        if err:
            t["errors"] += 1
            if len(examples) < 3:
                examples.append(err)
            continue
        count = collections.Counter(seen)
        t["dup_rows"] += sum(1 for n in count.values() if n > 1)
        missing = [i for i in rows.stable(start, end) if i not in count]
        t["missing_rows"] += len(missing)
        if missing and len(examples) < 3:
            examples.append(f"missing {missing[:3]}")
    return {**t, "examples": examples}


async def _round(base: str, r: Run, rows: _Rows) -> tuple[int, dict[str, dict[str, Any]]]:
    stop = time.monotonic() + SECS
    writers = [AsyncClient(base, r.issuer) for _ in range(3)]
    readers = [AsyncClient(base, r.issuer) for _ in ORDERS]
    try:
        writes, walks = await asyncio.gather(
            asyncio.gather(*[_writer(i, c, rows, stop) for i, c in enumerate(writers)]),
            asyncio.gather(
                *[_reader(c, o, rows, stop) for c, o in zip(readers, ORDERS, strict=True)]
            ),
        )
    finally:
        for c in writers + readers:
            await c.close()
    return sum(writes), dict(zip(ORDERS, walks, strict=True))


def run(r: Run) -> list[Outcome]:
    out: list[Outcome] = []
    for name, cache in [("cache_off", False), ("cache_ttl0", True)]:
        initial = db.column(r.db_url, "select id::text from tb_item where fk_account = %s", ACCOUNT)
        with r.serve(f"pag-{name}", cache=cache, ttl=0) as base:
            writes, walks = asyncio.run(_round(base, r, _Rows(initial)))
        if writes == 0:
            msg = f"pag {name}: no write succeeded"
            raise VacuousError(msg)
        for order, t in walks.items():
            if t.get("walks", 0) == 0:
                msg = f"pag {name} {order}: no walk completed"
                raise VacuousError(msg)
            out.append(
                Outcome(
                    f"CONC:pag:{name}:relay_{order}:each_stable_row_once",
                    t.get("dup_rows", 0) == 0
                    and t.get("missing_rows", 0) == 0
                    and t.get("errors", 0) == 0,
                    {"writes": writes, **t},
                )
            )
    return out
