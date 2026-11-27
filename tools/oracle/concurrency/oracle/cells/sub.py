"""Subscriptions: every committed change reaches each subscriber once, in commit order.

Three tenant-A subscribers and one tenant-B subscriber open `accountChanged` over
graphql-transport-ws, then 4 writers bump tenant-A accounts `WRITES` times in all. Per
account, versions are the commit order, since the row lock serialises the bumps.

- `tenant_a:exactly_once_in_commit_order`: each tenant-A subscriber receives every
  committed (account, version) once, none that was not committed, and per account in
  increasing version order.
- `tenant_b:receives_no_other_tenant`: the tenant-B subscriber receives none of them,
  and its subscription is neither refused nor ended.
"""

from __future__ import annotations

import asyncio
import collections
import json
import os
import random
import time
from typing import TYPE_CHECKING, Any

import websockets

from oracle.cells import Outcome, VacuousError
from oracle.client import AsyncClient, account_id
from oracle.issuer import TENANT_A, TENANT_B

if TYPE_CHECKING:
    from oracle.issuer import Issuer
    from oracle.run import Run

WRITES = int(os.environ.get("ORACLE_SUB_WRITES", "500"))
KEYS = 20
DRAIN_S = 15.0
SUBSCRIBE = "subscription { accountChanged { id version tenantId } }"
BUMP = "mutation($id: ID!) { bumpAccount(id: $id) { id version } }"


async def _subscriber(
    url: str,
    token: str,
    *,
    got: list[tuple[str, int]],
    other: list[str],
    ready: asyncio.Future[str],
    done: asyncio.Event,
) -> None:
    auth = {"authorization": "Bearer " + token}
    async with websockets.connect(
        url, subprotocols=[websockets.Subprotocol("graphql-transport-ws")], additional_headers=auth
    ) as ws:
        await ws.send(json.dumps({"type": "connection_init", "payload": auth}))
        ack = json.loads(await ws.recv())
        await ws.send(json.dumps({"id": "1", "type": "subscribe", "payload": {"query": SUBSCRIBE}}))
        ready.set_result(ack.get("type", ""))
        while True:
            try:
                m = json.loads(await asyncio.wait_for(ws.recv(), 0.5))
            except TimeoutError:
                if done.is_set():
                    return
                continue
            if m.get("type") == "next":
                d = (m["payload"].get("data") or {}).get("accountChanged")
                if d:
                    got.append((d["id"], d["version"]))
                else:
                    other.append(json.dumps(m)[:200])
            elif m.get("type") in ("error", "complete"):
                other.append(json.dumps(m)[:300])
                return


async def _writer(i: int, c: AsyncClient, left: list[int], log: list[tuple[str, int]]) -> None:
    rng = random.Random(200 + i)
    while left[0] > 0:
        left[0] -= 1
        _, body = await c.gql(BUMP, {"id": account_id(rng.randint(1, KEYS))})
        d = (body.get("data") or {}).get("bumpAccount")
        if d:
            log.append((d["id"], d["version"]))


async def _round(base: str, issuer: Issuer) -> dict[str, Any]:
    ws_url = base.replace("http", "ws", 1) + "/ws"
    names = [("a0", TENANT_A), ("a1", TENANT_A), ("a2", TENANT_A), ("b", TENANT_B)]
    got: dict[str, list[tuple[str, int]]] = {n: [] for n, _ in names}
    other: dict[str, list[str]] = {n: [] for n, _ in names}
    done = asyncio.Event()
    loop = asyncio.get_running_loop()
    readies: list[asyncio.Future[str]] = [loop.create_future() for _ in names]
    tasks = [
        asyncio.create_task(
            _subscriber(
                ws_url,
                issuer.token(n, t),
                got=got[n],
                other=other[n],
                ready=readies[i],
                done=done,
            )
        )
        for i, (n, t) in enumerate(names)
    ]
    acks = [await asyncio.wait_for(f, 10) for f in readies]
    await asyncio.sleep(1.0)
    committed: list[tuple[str, int]] = []
    writers = [AsyncClient(base, issuer) for _ in range(4)]
    left = [WRITES]
    await asyncio.gather(*[_writer(i, c, left, committed) for i, c in enumerate(writers)])
    for c in writers:
        await c.close()
    deadline = time.monotonic() + DRAIN_S
    while time.monotonic() < deadline and any(
        len(got[n]) < len(committed) for n in ("a0", "a1", "a2")
    ):
        await asyncio.sleep(0.1)
    await asyncio.sleep(0.5)
    done.set()
    await asyncio.gather(*tasks)
    return {"acks": acks, "committed": committed, "got": got, "other": other}


def _judge(events: list[tuple[str, int]], committed: set[tuple[str, int]]) -> dict[str, int]:
    count = collections.Counter(events)
    last: dict[str, int] = {}
    disorder = 0
    for i, v in events:
        if i in last and v < last[i]:
            disorder += 1
        last[i] = max(v, last.get(i, -1))
    return {
        "received": len(events),
        "missing": sum(1 for x in committed if x not in count),
        "duplicate": sum(1 for n in count.values() if n > 1),
        "phantom": sum(1 for x in count if x not in committed),
        "out_of_order": disorder,
    }


def run(r: Run) -> list[Outcome]:
    with r.serve("sub", cache=False, ttl=0, observers=True) as base:
        res = asyncio.run(_round(base, r.issuer))
    if any(a != "connection_ack" for a in res["acks"]):
        msg = f"sub: a subscriber was not acknowledged: {res['acks']}"
        raise VacuousError(msg)
    committed = set(res["committed"])
    if len(committed) != WRITES:
        msg = f"sub: {len(committed)} of {WRITES} writes committed"
        raise VacuousError(msg)
    a = {n: _judge(res["got"][n], committed) for n in ("a0", "a1", "a2")}
    b = _judge(res["got"]["b"], committed)
    return [
        Outcome(
            "CONC:sub:tenant_a:exactly_once_in_commit_order",
            all(
                j["missing"] == j["duplicate"] == j["phantom"] == j["out_of_order"] == 0
                and not res["other"][n]
                for n, j in a.items()
            ),
            {"writes": WRITES, "subscribers": a, "other": {n: res["other"][n][:2] for n in a}},
        ),
        Outcome(
            "CONC:sub:tenant_b:receives_no_other_tenant",
            b["received"] == 0 and not res["other"]["b"],
            {"received": b["received"], "other": res["other"]["b"][:2]},
        ),
    ]
