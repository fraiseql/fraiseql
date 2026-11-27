"""The result cache keeps principals apart: what one principal filled, another may not read.

Each cell fills the cache as one principal and then reads the same document as another:

- `scope:viewer_refused_secret`: an admin reads a field that requires `read:secret`, then
  a viewer of the same tenant reads the same document and is refused;
- `tenant:other_tenant_reads_null`: tenant A reads its account, then tenant B reads the
  same document and gets `null` (the tenant is injected into the query);
- `session:rls_per_tenant`: tenant A and then tenant B list the session-filtered view,
  and each sees only its own tenant's rows.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from oracle.cells import Outcome, VacuousError
from oracle.client import Client, account_id
from oracle.issuer import TENANT_A, TENANT_B

if TYPE_CHECKING:
    from oracle.run import Run

ACCOUNT = 44
SECRET = "query($id: ID!) { account(id: $id) { id version secret seenTenant } }"
PLAIN = "query($id: ID!) { account(id: $id) { id version seenTenant } }"
RLS = "{ rlsAccounts(limit: 5) { id tenantId seenTenant } }"


def _codes(body: dict[str, Any]) -> list[str]:
    return [
        str((e.get("extensions") or {}).get("code") or e.get("code"))
        for e in body.get("errors") or []
    ]


def run(r: Run) -> list[Outcome]:
    out: list[Outcome] = []
    k = {"id": account_id(ACCOUNT)}
    with r.serve("sec", ttl=0) as base:
        admin = Client(base, r.issuer, sub="adm", roles=("admin",))
        viewer = Client(base, r.issuer, sub="vw", roles=("viewer",))
        filled = admin.gql(SECRET, k)
        if not (filled.get("data") or {}).get("account"):
            msg = f"sec: the admin could not read the secret field: {str(filled)[:300]}"
            raise VacuousError(msg)
        refused = viewer.gql(SECRET, k)
        account = (refused.get("data") or {}).get("account")
        out.append(
            Outcome(
                "CONC:sec:scope:viewer_refused_secret",
                bool(refused.get("errors")) and not (account and account.get("secret")),
                {"codes": _codes(refused), "data": refused.get("data")},
            )
        )

        a = Client(base, r.issuer, sub="alice", tenant=TENANT_A)
        b = Client(base, r.issuer, sub="bob", tenant=TENANT_B)
        own = a.gql(PLAIN, k)
        if not (own.get("data") or {}).get("account"):
            msg = f"sec: tenant A could not read its own account: {str(own)[:300]}"
            raise VacuousError(msg)
        other = b.gql(PLAIN, k)
        out.append(
            Outcome(
                "CONC:sec:tenant:other_tenant_reads_null",
                (other.get("data") or {}).get("account") is None,
                {"tenant_b": other.get("data")},
            )
        )

        rows_a = (a.gql(RLS).get("data") or {}).get("rlsAccounts") or []
        rows_b = (b.gql(RLS).get("data") or {}).get("rlsAccounts") or []
        if not rows_a or not rows_b:
            msg = f"sec: the session-filtered view returned no rows ({len(rows_a)}, {len(rows_b)})"
            raise VacuousError(msg)
        out.append(
            Outcome(
                "CONC:sec:session:rls_per_tenant",
                all(x["tenantId"] == TENANT_A for x in rows_a)
                and all(x["tenantId"] == TENANT_B for x in rows_b),
                {
                    "tenant_a": [x["tenantId"] for x in rows_a],
                    "tenant_b": [x["tenantId"] for x in rows_b],
                },
            )
        )
        for c in (admin, viewer, a, b):
            c.close()
    return out
