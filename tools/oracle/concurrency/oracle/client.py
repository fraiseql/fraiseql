"""GraphQL clients for one principal, and the read shapes over an account's version."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

import httpx

from oracle.issuer import TENANT_A, Issuer

if TYPE_CHECKING:
    from collections.abc import Callable


def account_id(n: int) -> str:
    return f"{n:08x}-0000-4000-8000-000000000000"


def _first(root: str, field: str = "accountVersion") -> Callable[[dict[str, Any]], Any]:
    def read(data: dict[str, Any]) -> Any:
        rows = data[root]
        return rows[0][field] if rows else None

    return read


# name -> (document, reads the version out of `data`). Each document takes `$id`.
SHAPES: dict[str, tuple[str, Callable[[dict[str, Any]], Any]]] = {
    "direct": (
        "query($id: ID!) { account(id: $id) { version seenTenant } }",
        lambda d: d["account"] and d["account"]["version"],
    ),
    "list": (
        "query($id: ID!) { accounts(where: { id: { eq: $id } }) { version } }",
        _first("accounts", "version"),
    ),
    "nested": (
        "query($id: ID!) { items(where: { fkAccount: { eq: $id } }, limit: 1) "
        "{ account { version } } }",
        lambda d: d["items"][0]["account"]["version"] if d["items"] else None,
    ),
    "nestedcol": (
        "query($id: ID!) { items(where: { fkAccount: { eq: $id } }, limit: 1) { accountVersion } }",
        _first("items"),
    ),
    "fn": (
        "query($id: ID!) { itemsFn(where: { fkAccount: { eq: $id } }, limit: 1) "
        "{ accountVersion } }",
        _first("itemsFn"),
    ),
    "notefn": (
        "query($id: ID!) { notesFn(where: { fkAccount: { eq: $id } }, limit: 1) "
        "{ accountVersion } }",
        _first("notesFn"),
    ),
    "noteatomic": (
        "query($id: ID!) { notesAtomic(where: { fkAccount: { eq: $id } }, limit: 1) "
        "{ accountVersion } }",
        _first("notesAtomic"),
    ),
    "summary": (
        "query($id: ID!) { accountSummaries(where: { id: { eq: $id } }) { version } }",
        _first("accountSummaries", "version"),
    ),
}

BUMP = "mutation($id: ID!) { bumpAccount(id: $id) { version } }"


class ReadError(RuntimeError):
    """A read that returned no data: the oracle cannot judge it."""


def version_of(shape: str, body: dict[str, Any]) -> int:
    data = body.get("data")
    if not data:
        msg = f"{shape}: no data: {str(body.get('errors'))[:300]}"
        raise ReadError(msg)
    v = SHAPES[shape][1](data)
    if not isinstance(v, int):
        msg = f"{shape}: no version in {str(data)[:300]}"
        raise ReadError(msg)
    return v


class Client:
    """A synchronous client for one principal."""

    def __init__(
        self,
        base: str,
        issuer: Issuer,
        sub: str = "alice",
        tenant: str = TENANT_A,
        roles: tuple[str, ...] = ("viewer",),
    ) -> None:
        self.base = base
        self.headers = {"authorization": "Bearer " + issuer.token(sub, tenant, roles)}
        self.http = httpx.Client(base_url=base, headers=self.headers, timeout=30.0)

    def gql(
        self,
        query: str,
        variables: dict[str, Any] | None = None,
        headers: dict[str, str] | None = None,
        timeout: float | None = None,
    ) -> dict[str, Any]:
        r = self.http.post(
            "/graphql",
            json={"query": query, "variables": variables or {}},
            headers=headers or {},
            timeout=timeout if timeout is not None else 30.0,
        )
        try:
            return r.json()
        except ValueError:
            return {"errors": [{"message": f"non-JSON {r.status_code}: {r.text[:200]}"}]}

    def read(self, shape: str, n: int) -> int:
        return version_of(shape, self.gql(SHAPES[shape][0], {"id": account_id(n)}))

    def bump(self, n: int) -> int:
        body = self.gql(BUMP, {"id": account_id(n)})
        data = body.get("data") or {}
        if not data.get("bumpAccount"):
            msg = f"bumpAccount({n}) failed: {str(body)[:300]}"
            raise ReadError(msg)
        return data["bumpAccount"]["version"]

    def close(self) -> None:
        self.http.close()


class AsyncClient:
    """An asynchronous client for one principal, for the concurrent cells."""

    def __init__(
        self,
        base: str,
        issuer: Issuer,
        sub: str = "alice",
        tenant: str = TENANT_A,
        roles: tuple[str, ...] = ("viewer",),
    ) -> None:
        headers = {"authorization": "Bearer " + issuer.token(sub, tenant, roles)}
        self.http = httpx.AsyncClient(
            base_url=base,
            headers=headers,
            timeout=30.0,
            limits=httpx.Limits(max_connections=8, max_keepalive_connections=8),
        )

    async def gql(
        self,
        query: str,
        variables: dict[str, Any] | None = None,
        headers: dict[str, str] | None = None,
        timeout: float | None = None,
    ) -> tuple[int, dict[str, Any]]:
        r = await self.http.post(
            "/graphql",
            json={"query": query, "variables": variables or {}},
            headers=headers or {},
            timeout=timeout if timeout is not None else 30.0,
        )
        try:
            return r.status_code, r.json()
        except ValueError:
            return r.status_code, {"errors": [{"message": f"non-JSON {r.status_code}"}]}

    async def close(self) -> None:
        await self.http.aclose()
