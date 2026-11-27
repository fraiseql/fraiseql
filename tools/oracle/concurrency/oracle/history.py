"""A register workload: writers bump account versions, readers read them back.

Every operation is one JSON line with its monotonic invoke (`ti`) and complete (`tc`)
times, in the format `checker.py` reads. Each worker draws its keys from its own
`random.Random`, seeded from the run's seed and the worker's index, so a run's choices
do not depend on scheduling.
"""

from __future__ import annotations

import asyncio
import json
import random
import time
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

import httpx

from oracle.client import BUMP, SHAPES, AsyncClient, account_id

if TYPE_CHECKING:
    from pathlib import Path

    from oracle.issuer import Issuer


@dataclass(frozen=True)
class Workload:
    keys: list[int]
    shapes: list[str]
    secs: float
    writers: int = 4
    readers: int = 12
    seed: int = 1


async def _writer(
    i: int, client: AsyncClient, w: Workload, stop: float, log: list[dict[str, Any]]
) -> None:
    rng = random.Random(w.seed * 1000 + i)
    while time.monotonic() < stop:
        k = rng.choice(w.keys)
        ti = time.monotonic()
        try:
            _, body = await client.gql(BUMP, {"id": account_id(k)})
        except httpx.HTTPError as e:
            log.append(
                {
                    "op": "w",
                    "p": f"w{i}",
                    "k": k,
                    "ti": ti,
                    "tc": time.monotonic(),
                    "ok": None,
                    "err": repr(e)[:200],
                }
            )
            continue
        tc = time.monotonic()
        d = (body.get("data") or {}).get("bumpAccount")
        log.append(
            {
                "op": "w",
                "p": f"w{i}",
                "k": k,
                "ti": ti,
                "tc": tc,
                "ok": d is not None,
                "v": d["version"] if d else None,
                "err": None if d else str(body.get("errors"))[:200],
            }
        )


async def _reader(
    i: int,
    client: AsyncClient,
    *,
    server: int,
    shape: str,
    w: Workload,
    stop: float,
    log: list[dict[str, Any]],
) -> None:
    rng = random.Random(w.seed * 1000 + 500 + i)
    query, extract = SHAPES[shape]
    while time.monotonic() < stop:
        k = rng.choice(w.keys)
        ti = time.monotonic()
        rec: dict[str, Any] = {
            "op": "r",
            "p": f"r{i}",
            "shape": shape,
            "server": server,
            "k": k,
            "ti": ti,
        }
        try:
            _, body = await client.gql(query, {"id": account_id(k)})
        except httpx.HTTPError as e:
            log.append(
                {**rec, "tc": time.monotonic(), "ok": False, "v": None, "err": repr(e)[:200]}
            )
            continue
        rec["tc"] = time.monotonic()
        data = body.get("data")
        rec["ok"] = data is not None
        rec["v"] = extract(data) if data else None
        if not data:
            rec["err"] = str(body.get("errors"))[:200]
        log.append(rec)


async def _run(
    write_url: str, read_urls: list[str], issuer: Issuer, w: Workload
) -> list[dict[str, Any]]:
    log: list[dict[str, Any]] = []
    stop = time.monotonic() + w.secs
    clients: list[AsyncClient] = []
    tasks = []
    for i in range(w.writers):
        c = AsyncClient(write_url, issuer, sub="alice")
        clients.append(c)
        tasks.append(_writer(i, c, w, stop, log))
    for i in range(w.readers):
        # Half the readers are the writer's principal, half are others of its tenant.
        sub = "alice" if i % 2 == 0 else f"bob{i}"
        server = i % len(read_urls)
        c = AsyncClient(read_urls[server], issuer, sub=sub)
        clients.append(c)
        tasks.append(
            _reader(i, c, server=server, shape=w.shapes[i % len(w.shapes)], w=w, stop=stop, log=log)
        )
    try:
        await asyncio.gather(*tasks)
    finally:
        for c in clients:
            await c.close()
    return log


def record(
    path: Path,
    *,
    write_url: str,
    read_urls: list[str],
    issuer: Issuer,
    w: Workload,
    initial: dict[int, int],
) -> None:
    """Runs the workload and writes its history to `path`."""
    ops = asyncio.run(_run(write_url, read_urls, issuer, w))
    meta = {"op": "meta", "initial": {str(k): v for k, v in initial.items()}, "workload": vars(w)}
    with path.open("w") as f:
        f.write(json.dumps(meta) + "\n")
        for o in ops:
            f.write(json.dumps(o) + "\n")
