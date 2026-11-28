"""The WS column: GraphQL subscriptions over graphql-transport-ws, graded against POST.

Every subscription is open before any row is written, so every subscriber is graded on
every row, whatever the timing:

1. one subscription per (principal, selection) opens, and is acknowledged or refused;
2. each writer creates one row through POST `createOrder`;
3. the subscriptions collect events until each has one per row, or the wait ends;
4. each (subscription, row) cell is compared with what POST `order(id:)` returns to that
   principal for that selection.

A principal POST serves the row to must receive one event with the same data. A principal
POST serves nothing (or refuses) must receive no event (or the same refusal). The rows
are deleted afterwards, so the rest of the corpus and later runs see the fixture
unchanged.
"""

from __future__ import annotations

import asyncio
import hashlib
import json
import time
from collections.abc import Callable
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any

import psycopg
import websockets
from websockets.exceptions import ConnectionClosed, InvalidStatus
from websockets.typing import Subprotocol

from . import generate as gen
from .clients import Response

if TYPE_CHECKING:
    from .clients import Clients

SELECTIONS = [
    ["id", "label", "tenantId"],
    ["id", "secret"],
    ["id", "note"],
    ["id", "amount", "big", "ratio", "createdAt", "status"],
]
# writer principal -> the customer its row references
WRITERS = {
    "A-viewer": "00000004-0000-4000-8000-000000000000",
    "B-viewer": "00000003-0000-4000-8000-000000000000",
}
LABEL = "ws-oracle"
ACK_WAIT = 5.0
EVENT_WAIT = 8.0


@dataclass
class Subscription:
    who: str
    fields: list[str]
    refusal: Response | None = None
    events: list[dict[str, Any]] = field(default_factory=list)


async def _open(
    url: str, token: str | None, sub: Subscription, ready: asyncio.Event, stop: asyncio.Event
) -> None:
    """Runs one subscription until `stop`. A refusal at any step is kept as codes."""
    headers = {"authorization": f"Bearer {token}"} if token else {}
    doc = f"subscription {{ orderCreated {{ {' '.join(sub.fields)} }} }}"
    try:
        async with websockets.connect(
            url, subprotocols=[Subprotocol("graphql-transport-ws")], additional_headers=headers
        ) as sock:
            await sock.send(json.dumps({"type": "connection_init", "payload": {}}))
            ack = json.loads(await asyncio.wait_for(sock.recv(), ACK_WAIT))
            if ack.get("type") != "connection_ack":
                sub.refusal = Response(status="ws", codes=[f"ws-{ack.get('type')}"], code_at="none")
                return
            await sock.send(json.dumps({"id": "1", "type": "subscribe", "payload": {"query": doc}}))
            ready.set()
            while not stop.is_set():
                try:
                    frame = json.loads(await asyncio.wait_for(sock.recv(), 0.2))
                except TimeoutError:
                    continue
                kind = frame.get("type")
                if kind == "ping":
                    await sock.send(json.dumps({"type": "pong"}))
                elif kind == "next":
                    sub.events.append(frame.get("payload") or {})
                elif kind in ("error", "complete"):
                    errors = frame.get("payload") if kind == "error" else []
                    codes = [
                        str((e.get("extensions") or {}).get("code") or "no-code")
                        for e in errors or []
                        if isinstance(e, dict)
                    ]
                    sub.refusal = Response(
                        status="ws",
                        codes=codes or [f"ws-{kind}"],
                        code_at="extensions" if codes else "none",
                    )
                    return
    except InvalidStatus as e:
        sub.refusal = Response(
            status=str(e.response.status_code),
            codes=[f"http-{e.response.status_code}"],
            code_at="none",
        )
    except ConnectionClosed as e:
        code = e.rcvd.code if e.rcvd else 1006
        sub.refusal = Response(status="ws", codes=[f"ws-close-{code}"], code_at="none")
    except (OSError, TimeoutError) as e:
        sub.refusal = Response(status="ws", codes=[f"ws:{type(e).__name__}"], code_at="none")
    finally:
        ready.set()


def _event_row(payload: dict[str, Any]) -> Any:
    data = payload.get("data")
    return data.get("orderCreated") if isinstance(data, dict) else None


# (subscriptions, writer -> written row id) -> None
Tamper = Callable[[list[Subscription], dict[str, str]], None]


@dataclass
class Cell:
    request: gen.Request
    writer: str
    ref: Response
    got: Response


def run(
    clients: Clients,
    ws_http: str,
    tokens: dict[str, str | None],
    db_url: str,
    *,
    selections: list[list[str]] = SELECTIONS,
    tamper: Tamper | None = None,
) -> list[Cell]:
    """Every (subscription, written row) cell, with POST's answer as its reference.

    `tamper`, for the self-test only, may change what the subscriptions received before
    they are graded: it stands in for a server that delivers differently."""
    subs = [Subscription(who, sel) for who in gen.PRINCIPALS for sel in selections]
    url = ws_http.replace("http", "ws", 1) + "/ws"
    rows: dict[str, str] = {}

    async def scenario() -> None:
        stop = asyncio.Event()
        readies = [asyncio.Event() for _ in subs]
        tasks = [
            asyncio.create_task(_open(url, tokens[s.who], s, r, stop))
            for s, r in zip(subs, readies, strict=True)
        ]
        await asyncio.wait_for(asyncio.gather(*(r.wait() for r in readies)), ACK_WAIT * 2)
        await asyncio.sleep(0.5)  # every subscribe frame handled before the first write
        for writer, customer in WRITERS.items():
            doc = (
                f'mutation {{ createOrder(label: "{LABEL} {writer}", amount: "1.5", '
                f'fkCustomer: "{customer}") {{ id }} }}'
            )
            r = await asyncio.to_thread(clients.post, doc, None, tokens[writer], {}, "createOrder")
            if not isinstance(r.data, dict) or "id" not in r.data:
                msg = f"the WS column's write as {writer} failed: {r.codes} {r.raw[:200]}"
                raise RuntimeError(msg)
            rows[writer] = r.data["id"]
        deadline = time.monotonic() + EVENT_WAIT
        live = [s for s in subs if s.refusal is None]
        while time.monotonic() < deadline and any(len(s.events) < len(rows) for s in live):
            await asyncio.sleep(0.1)
        await asyncio.sleep(0.5)  # an event beyond the expected count is still collected
        stop.set()
        await asyncio.gather(*tasks)

    try:
        asyncio.run(scenario())
        if tamper is not None:
            tamper(subs, rows)
        return _grade(clients, tokens, subs, rows)
    finally:
        with psycopg.connect(db_url, autocommit=True) as conn:
            conn.execute("DELETE FROM tb_order WHERE label LIKE %s", (f"{LABEL} %",))


def _grade(
    clients: Clients,
    tokens: dict[str, str | None],
    subs: list[Subscription],
    rows: dict[str, str],
) -> list[Cell]:
    cells = []
    for s in subs:
        for writer, row_id in rows.items():
            req = gen.Request(
                name=f"ws:{s.who}:{'+'.join(s.fields)}:{writer}",
                root="order",
                fields=s.fields,
                who=s.who,
                hdr="",
                style="inline",
                id=row_id,
            )
            doc, variables, key = gen.graphql(req)
            ref = clients.post(doc, variables, tokens[s.who], {}, key)
            cells.append(Cell(req, writer, ref, answer(s, row_id)))
    return cells


def answer(sub: Subscription, row_id: str) -> Response:
    """What the subscription answered for one row: its refusal, or the events carrying
    that row. One event is the row; none is no row; more than one is kept as a list, so
    a repeated delivery differs from POST's single row."""
    if sub.refusal is not None:
        return sub.refusal
    mine = [
        e
        for e in (_event_row(p) for p in sub.events)
        if isinstance(e, dict) and e.get("id") == row_id
    ]
    errors = [
        str((e.get("extensions") or {}).get("code") or "no-code")
        for p in sub.events
        for e in p.get("errors") or []
        if isinstance(e, dict)
    ]
    return Response(
        status="ws",
        data=(mine[0] if len(mine) == 1 else mine) if mine else None,
        has_data=bool(mine),
        codes=sorted(set(errors)),
        code_at="extensions" if errors else "",
    )


def fingerprint(cell: Cell) -> str:
    """The cell's identity across runs. The written row's id is new on every run, so it
    is left out."""
    who = {"ws": cell.request.who, "fields": cell.request.fields, "writer": cell.writer}
    return hashlib.sha256(json.dumps(who, sort_keys=True).encode()).hexdigest()[:16]
