"""`Idempotency-Key`: a mutation carrying one executes at most once per key."""

from __future__ import annotations

import asyncio
import uuid
from typing import TYPE_CHECKING

from oracle import db
from oracle.cells import Outcome, VacuousError
from oracle.client import AsyncClient, Client

if TYPE_CHECKING:
    from oracle.run import Run

RECORD = "mutation($t: String!, $ms: Int!) { recordEffect(tag: $t, ms: $ms) { version } }"
CONCURRENT = 5
CONCURRENT_SLEEP_MS = 400


def effects(url: str, tag: str) -> int:
    return int(db.scalar(url, "select count(*) from tb_effect where tag = %s", tag))


def _tag(prefix: str) -> str:
    return f"{prefix}-{uuid.uuid4().hex[:12]}"


async def _concurrent(base: str, r: Run, tag: str) -> list[int]:
    client = AsyncClient(base, r.issuer)
    try:
        sends = [
            client.gql(
                RECORD,
                {"t": tag, "ms": CONCURRENT_SLEEP_MS},
                headers={"idempotency-key": "k-" + tag},
            )
            for _ in range(CONCURRENT)
        ]
        return [status for status, _ in await asyncio.gather(*sends)]
    finally:
        await client.close()


def run(r: Run) -> list[Outcome]:
    out: list[Outcome] = []
    with r.serve("idem", ttl=0) as base:
        alice = Client(base, r.issuer)

        # Without a key every send executes: proves the effect count sees each one.
        tag = _tag("nokey")
        for _ in range(2):
            alice.gql(RECORD, {"t": tag, "ms": 0})
        if effects(r.db_url, tag) != 2:
            msg = f"idem: two sends without a key recorded {effects(r.db_url, tag)} effects"
            raise VacuousError(msg)

        tag = _tag("seq")
        for _ in range(3):
            alice.gql(RECORD, {"t": tag, "ms": 0}, headers={"idempotency-key": "k-" + tag})
        n = effects(r.db_url, tag)
        out.append(
            Outcome("CONC:idem:sequential:executes_once", n == 1, {"sends": 3, "effects": n})
        )

        tag = _tag("conc")
        statuses = asyncio.run(_concurrent(base, r, tag))
        n = effects(r.db_url, tag)
        out.append(
            Outcome(
                "CONC:idem:concurrent:executes_once",
                n == 1,
                {"sends": CONCURRENT, "effects": n, "statuses": statuses},
            )
        )
        alice.close()
    return out
