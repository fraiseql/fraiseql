"""The in-flight fence: a read that was running while a write evicted is not stored.

The `slow` shape's statement takes 300 ms and takes its snapshot when it starts. A write
that completes while it runs is not in its answer, which is correct for that read, but
must not be stored for the reads after it:

1. a slow read starts (a cache miss);
2. 100 ms later, a write to the same account completes;
3. the slow read returns the version before the write (the precondition: the two
   really overlapped);
4. the next read must return the written version (`inflight_read_not_stored`).
"""

from __future__ import annotations

import concurrent.futures
import time
from typing import TYPE_CHECKING

from oracle import db
from oracle.cells import Outcome, VacuousError
from oracle.client import Client, account_id

if TYPE_CHECKING:
    from oracle.run import Run

ACCOUNT = 39
OVERLAP_S = 0.1


def run(r: Run) -> list[Outcome]:
    n = ACCOUNT
    with r.serve("fence", ttl=0) as base:
        alice = Client(base, r.issuer)
        reader = Client(base, r.issuer)
        # The slow shape is cached at all: a write the server never sees is not read.
        cached = alice.read("slow", n)
        db.execute(
            r.db_url, "update tb_account set version = version + 1 where id = %s", account_id(n)
        )
        if alice.read("slow", n) != cached:
            msg = "fence: the slow shape is not cached"
            raise VacuousError(msg)
        alice.bump(n)

        with concurrent.futures.ThreadPoolExecutor(1) as pool:
            inflight = pool.submit(reader.read, "slow", n)
            time.sleep(OVERLAP_S)
            wrote = alice.bump(n)
            during = inflight.result()
        if during >= wrote:
            msg = f"fence: the slow read returned {during}, so it did not overlap write {wrote}"
            raise VacuousError(msg)
        served = alice.read("slow", n)
        alice.close()
        reader.close()
    return [
        Outcome(
            "CONC:fence:inflight_read_not_stored",
            served == wrote,
            {"account": n, "inflight_read": during, "wrote": wrote, "served": served},
        )
    ]
