"""Sequential cells: a write made through one server, then a read through it.

The documented promise: on one server, a cached read is never staler than the last write
made through it, and a successful mutation evicts every cached entry of every view it
reaches.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from oracle import db
from oracle.cells import Outcome, VacuousError
from oracle.client import Client, account_id

if TYPE_CHECKING:
    from oracle.run import Run

# Each shape gets its own account (40 + its index), so no cell sees another's write.
SHAPES = ["direct", "list", "nested", "nestedcol", "fn", "notefn", "noteatomic", "summary"]
ERROR_OUTCOME_ACCOUNT = 48
BUMP_FAIL = "mutation($id: ID!) { bumpFail(id: $id) { version } }"


def _db_version(url: str, n: int) -> int:
    return int(db.scalar(url, "select version from tb_account where id = %s", account_id(n)))


def _direct_write(url: str, n: int) -> None:
    db.execute(url, "update tb_account set version = version + 1 where id = %s", account_id(n))


def run(r: Run) -> list[Outcome]:
    out: list[Outcome] = []
    with r.serve("probe", ttl=0) as base:
        alice = Client(base, r.issuer)
        for i, shape in enumerate(SHAPES):
            n = 40 + i
            alice.read(shape, n)
            before = alice.read(shape, n)
            # A write the server never sees must still be answered from the cache, or
            # this shape is not cached and its eviction cell would hold vacuously.
            _direct_write(r.db_url, n)
            if alice.read(shape, n) != before:
                msg = f"probe {shape}: a read after a write the server did not see was not cached"
                raise VacuousError(msg)
            wrote = alice.bump(n)
            served = alice.read(shape, n)
            out.append(
                Outcome(
                    f"CONC:probe:{shape}:evicted",
                    served == wrote,
                    {"account": n, "before": before, "wrote": wrote, "served": served},
                )
            )

        n = ERROR_OUTCOME_ACCOUNT
        alice.read("direct", n)
        before = alice.read("direct", n)
        response = alice.gql(BUMP_FAIL, {"id": account_id(n)})
        if not response.get("errors"):
            msg = f"probe error_outcome: bumpFail did not report an error: {str(response)[:300]}"
            raise VacuousError(msg)
        stored = _db_version(r.db_url, n)
        served = alice.read("direct", n)
        # Either the failed write is not committed, or what is served is what is stored.
        out.append(
            Outcome(
                "CONC:probe:error_outcome:consistent",
                served == stored,
                {"account": n, "before": before, "stored": stored, "served": served},
            )
        )
        alice.close()
    return out
