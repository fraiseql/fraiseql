//! The `client.py` runtime template.
//!
//! Identical for every generated client and dependency-free: the transport is
//! `urllib.request` from the standard library. The generator prepends the
//! schema-hash header; this constant is the body.

/// Contents of the generated `client.py` (without the auto-generated header).
pub(super) const CLIENT_PY: &str = r#""""Minimal GraphQL client over urllib.

The only runtime dependency of the generated client is the Python standard
library. Pass an ``httpx``/``requests``-based transport of your own by
subclassing :class:`FraiseqlClient` and overriding :meth:`request` if you need
connection pooling or async execution.
"""

from __future__ import annotations

import json
import urllib.error
import urllib.request
from collections.abc import Callable, Mapping
from typing import Any, cast

type HeaderSource = Mapping[str, str] | Callable[[], Mapping[str, str]]


def _resolve_headers(source: HeaderSource | None) -> dict[str, str]:
    """Normalise a header source to a plain dict.

    The ``cast`` calls carry the ``isinstance`` narrowing across the
    ``Mapping | Callable`` union for type checkers.
    """
    if source is None:
        return {}
    if isinstance(source, Mapping):
        return dict(cast("Mapping[str, str]", source))
    return dict(cast("Callable[[], Mapping[str, str]]", source)())


class FraiseqlError(Exception):
    """Raised when a GraphQL request fails at the HTTP or GraphQL-errors layer."""

    def __init__(self, message: str, errors: list[dict[str, Any]] | None = None) -> None:
        super().__init__(message)
        self.errors: list[dict[str, Any]] = errors or []


def omit_none(variables: Mapping[str, Any]) -> dict[str, Any]:
    """Drop ``None``-valued entries from a variables mapping.

    Optional operation arguments default to ``None`` and are omitted from the
    request, so the server applies its own defaults. Sending an explicit JSON
    ``null`` for an optional argument is not supported by the generated
    wrappers — call :meth:`FraiseqlClient.request` directly for that.
    """
    return {key: value for key, value in variables.items() if value is not None}


class FraiseqlClient:
    """Executes GraphQL documents against a FraiseQL endpoint.

    The generated operation functions wrap :meth:`request` and unwrap their
    single root field.
    """

    def __init__(
        self,
        endpoint: str,
        *,
        headers: HeaderSource | None = None,
        timeout: float = 30.0,
    ) -> None:
        self.endpoint = endpoint
        self.headers = headers
        self.timeout = timeout

    def request(
        self,
        document: str,
        variables: Mapping[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Execute a GraphQL document and return its ``data`` payload.

        Raises :class:`FraiseqlError` when the response carries GraphQL errors,
        a non-2xx HTTP status, or no ``data``.
        """
        merged_headers: dict[str, str] = {
            "content-type": "application/json",
            "accept": "application/json",
        }
        merged_headers.update(_resolve_headers(self.headers))
        body = json.dumps({"query": document, "variables": dict(variables or {})})
        request = urllib.request.Request(
            self.endpoint,
            data=body.encode("utf-8"),
            headers=merged_headers,
            method="POST",
        )

        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                payload = json.loads(response.read().decode("utf-8"))
        except urllib.error.HTTPError as exc:
            raise FraiseqlError(
                f"GraphQL request failed with HTTP {exc.code} {exc.reason}"
            ) from exc

        errors = payload.get("errors")
        if errors:
            raise FraiseqlError(errors[0].get("message", "GraphQL error"), errors)
        data = payload.get("data")
        if data is None:
            raise FraiseqlError("GraphQL response contained no data")
        return data
"#;
