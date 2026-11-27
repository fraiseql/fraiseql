"""A client that gives up while its mutation commits.

The mutations here insert into `tb_commit_delay`, whose deferred trigger makes COMMIT
take `COMMIT_MS`. The client gives up after `GIVE_UP_S`, well inside that COMMIT, so
every round cuts the request at the same point: the database commits the write after the
client has gone. What must follow a commit has to happen anyway:

- `committed_write_is_evicted`: the cached read of the account serves the committed
  version;
- `retry_executes_once`: the retry the client is told to make, under the same
  `Idempotency-Key`, does not execute the mutation a second time.

Each cell holds only when it holds in every one of its `ROUNDS`.
"""

from __future__ import annotations

import os
import time
import uuid
from typing import TYPE_CHECKING

import httpx

from oracle import db
from oracle.cells import Outcome, VacuousError
from oracle.cells.idem import effects
from oracle.client import Client, account_id

if TYPE_CHECKING:
    from oracle.run import Run

COMMIT_MS = 600
GIVE_UP_S = 0.2
SETTLE_S = COMMIT_MS / 1000 + 0.4
ROUNDS = int(os.environ.get("ORACLE_CUTOFF_ROUNDS", "3"))
FIRST_ACCOUNT = 30
BUMP = "mutation($id: ID!, $ms: Int!) { commitSlowBump(id: $id, ms: $ms) { version } }"
EFFECT = "mutation($t: String!, $ms: Int!) { commitSlowEffect(tag: $t, ms: $ms) { version } }"


def _gives_up(client: Client, query: str, variables: dict[str, object], **headers: str) -> None:
    try:
        client.gql(query, variables, headers=headers, timeout=GIVE_UP_S)
    except httpx.TimeoutException:
        return
    msg = f"cutoff: the request completed within {GIVE_UP_S}s, before its COMMIT finished"
    raise VacuousError(msg)


def _version(url: str, n: int) -> int:
    return int(db.scalar(url, "select version from tb_account where id = %s", account_id(n)))


def run(r: Run) -> list[Outcome]:
    evicted: list[dict[str, int]] = []
    once: list[dict[str, object]] = []
    with r.serve("cutoff", ttl=0) as base:
        alice = Client(base, r.issuer)
        for i in range(ROUNDS):
            n = FIRST_ACCOUNT + i
            alice.read("direct", n)
            before = alice.read("direct", n)
            _gives_up(alice, BUMP, {"id": account_id(n), "ms": COMMIT_MS})
            time.sleep(SETTLE_S)
            stored = _version(r.db_url, n)
            if stored != before + 1:
                msg = f"cutoff: account {n} stored {stored} after giving up, not {before + 1}"
                raise VacuousError(msg)
            evicted.append({"account": n, "stored": stored, "served": alice.read("direct", n)})

            tag = f"cut-{uuid.uuid4().hex[:12]}"
            key = {"idempotency-key": "k-" + tag}
            _gives_up(alice, EFFECT, {"t": tag, "ms": COMMIT_MS}, **key)
            time.sleep(SETTLE_S)
            if effects(r.db_url, tag) != 1:
                msg = f"cutoff: the given-up effect {tag} was not committed once"
                raise VacuousError(msg)
            alice.gql(EFFECT, {"t": tag, "ms": 0}, headers=key)
            once.append({"tag": tag, "effects": effects(r.db_url, tag)})
        alice.close()
    return [
        Outcome(
            "CONC:cutoff:committed_write_is_evicted",
            all(e["served"] == e["stored"] for e in evicted),
            {"rounds": evicted},
        ),
        Outcome(
            "CONC:cutoff:retry_executes_once",
            all(o["effects"] == 1 for o in once),
            {"rounds": once},
        ),
    ]
