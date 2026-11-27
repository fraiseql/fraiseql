"""Read replicas: within the pin window, a client reads its own writes.

A write pins its principal to the primary for a while; another principal's reads may
still go to a replica that has not replayed the write. These cells hold the replica's
replay with `pg_wal_replay_pause()`, so its lag is a state, not a race:

1. replay paused; alice (the writer) bumps her account;
2. bob, same tenant, reads it: from the paused replica he gets the old version. That is
   allowed, and it is the precondition: it proves reads do reach the lagging replica;
3. alice reads it: she must get her write (`writer_reads_own_write`);
4. replay resumes and the replica catches up with the write;
5. alice and bob read it: both must get the write (`reads_after_catch_up`).

Each is run with the result cache off, where it must hold, and on at TTL 0.
"""

from __future__ import annotations

import time
from typing import TYPE_CHECKING, Any

from oracle import db
from oracle.cells import Outcome, VacuousError
from oracle.client import Client

if TYPE_CHECKING:
    from typing_extensions import LiteralString

    from oracle.run import Run

DATABASE = "oracle_concurrency_replica"
CONFIGS = [("cache_off", False, 10), ("cache_ttl0", True, 20)]


def _wait(what: str, url: str, query: LiteralString, *params: Any, deadline_s: float = 30) -> None:
    deadline = time.monotonic() + deadline_s
    while time.monotonic() < deadline:
        if db.scalar(url, query, *params):
            return
        time.sleep(0.05)
    msg = f"replica: {what} within {deadline_s}s"
    raise VacuousError(msg)


def _caught_up(primary: str, replica: str) -> None:
    lsn = db.scalar(primary, "select pg_current_wal_lsn()::text")
    _wait(
        "the replica did not replay the primary's WAL",
        replica,
        "select pg_last_wal_replay_lsn() >= %s::pg_lsn",
        lsn,
    )


def run(r: Run) -> list[Outcome]:
    if not (r.primary_url and r.replica_url):
        msg = "replica: no primary/replica pair (ORACLE_PRIMARY_URL, ORACLE_REPLICA_URL)"
        raise VacuousError(msg)
    primary = r.setup_database(r.primary_url, DATABASE)
    replica = db.with_database(r.replica_url, DATABASE)
    _caught_up(r.primary_url, r.replica_url)

    out: list[Outcome] = []
    top = f'read_replica_urls = ["{replica}"]'
    for name, cache, n in CONFIGS:
        with r.serve(f"replica-{name}", cache=cache, ttl=0, top=top, database_url=primary) as base:
            alice = Client(base, r.issuer, sub="alice")
            bob = Client(base, r.issuer, sub="bob")
            alice.read("direct", n)
            db.execute(r.replica_url, "select pg_wal_replay_pause()")
            _wait(
                "replay did not pause",
                r.replica_url,
                "select pg_get_wal_replay_pause_state() = 'paused'",
            )
            try:
                wrote = alice.bump(n)
                bob_paused = bob.read("direct", n)
                alice_paused = alice.read("direct", n)
            finally:
                db.execute(r.replica_url, "select pg_wal_replay_resume()")
            if bob_paused == wrote:
                msg = f"replica {name}: bob read the write while replay was paused"
                raise VacuousError(msg)
            _caught_up(r.primary_url, r.replica_url)
            alice_after = alice.read("direct", n)
            bob_after = bob.read("direct", n)
            detail = {
                "account": n,
                "wrote": wrote,
                "bob_paused": bob_paused,
                "alice_paused": alice_paused,
                "alice_after": alice_after,
                "bob_after": bob_after,
            }
            out.append(
                Outcome(
                    f"CONC:replica:{name}:writer_reads_own_write", alice_paused == wrote, detail
                )
            )
            out.append(
                Outcome(
                    f"CONC:replica:{name}:reads_after_catch_up",
                    alice_after == wrote and bob_after == wrote,
                    detail,
                )
            )
            alice.close()
            bob.close()
    return out
