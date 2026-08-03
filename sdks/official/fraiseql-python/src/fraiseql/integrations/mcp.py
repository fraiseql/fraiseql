"""Raw MCP (Model Context Protocol) integration for FraiseQL.

Exposes FraiseQL operations as MCP tool descriptors and dispatches ``call_tool``
requests through the typed SDK client, for embedding in any Python MCP server
implementation — no MCP SDK dependency, the descriptors and results are plain
dicts in the MCP wire shape.

This is the in-process complement to the server's own MCP transport: use it
when the agent loop and FraiseQL client already live in the same Python
process and an extra MCP server hop would be overhead.

Example:
    ```python
    from fraiseql.client import FraiseQLClient
    from fraiseql.integrations.mcp import FraiseQLMcpTools

    client = FraiseQLClient("http://localhost:8080/graphql", auth_token=token)
    tools = await FraiseQLMcpTools.from_client(client)

    tools.list_tools()                       # → MCP tools/list result entries
    await tools.call_tool("users", {"limit": 5})   # → MCP tools/call result
    ```
"""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, Any

from fraiseql.integrations._spec import OperationSpec, operation_specs

if TYPE_CHECKING:
    from collections.abc import Mapping

    from fraiseql.client import FraiseQLClient


def as_tools(
    schema_data: dict[str, Any],
    *,
    include: list[str] | None = None,
    exclude: list[str] | None = None,
) -> list[dict[str, Any]]:
    """MCP tool descriptors for every exposed FraiseQL operation.

    Args:
        schema_data: The introspection payload (``client.introspect()`` result).
        include: Whitelist of operation names (None = all).
        exclude: Blacklist of operation names (None = none).
    """
    return [
        _descriptor(spec) for spec in operation_specs(schema_data, include=include, exclude=exclude)
    ]


def _descriptor(spec: OperationSpec) -> dict[str, Any]:
    return {
        "name": spec.name,
        "description": spec.display_description,
        "inputSchema": spec.parameters_schema,
    }


class FraiseQLMcpTools:
    """FraiseQL operations as MCP tools, with a ``tools/call`` dispatcher."""

    def __init__(
        self,
        client: FraiseQLClient,
        schema_data: dict[str, Any],
        *,
        include: list[str] | None = None,
        exclude: list[str] | None = None,
    ) -> None:
        self._client = client
        self._specs: dict[str, OperationSpec] = {
            spec.name: spec
            for spec in operation_specs(schema_data, include=include, exclude=exclude)
        }

    @classmethod
    async def from_client(
        cls,
        client: FraiseQLClient,
        *,
        include: list[str] | None = None,
        exclude: list[str] | None = None,
    ) -> FraiseQLMcpTools:
        """Introspect the connected server and build the tool set."""
        schema_data = await client.introspect()
        return cls(client, schema_data, include=include, exclude=exclude)

    def list_tools(self) -> list[dict[str, Any]]:
        """The MCP tool descriptors (``tools/list`` result entries)."""
        return [_descriptor(spec) for spec in self._specs.values()]

    async def call_tool(
        self,
        name: str,
        arguments: Mapping[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Execute an MCP ``tools/call`` through the typed client.

        Only operations in this tool set are callable — an unknown name is an
        error result, never a server round-trip, so an include/exclude
        allowlist holds at the dispatch boundary too.

        Returns:
            An MCP tool result: ``{"content": [{"type": "text", "text": ...}],
            "isError": bool}``.
        """
        spec = self._specs.get(name)
        if spec is None:
            return _error_result(f"Unknown tool: {name}")

        try:
            result = await self._client.execute(
                spec.document,
                variables=dict(arguments) if arguments else None,
            )
        except Exception as exc:  # MCP results carry errors in-band
            return _error_result(str(exc))

        return {
            "content": [{"type": "text", "text": json.dumps(result.get("data", {}))}],
            "isError": False,
        }


def _error_result(message: str) -> dict[str, Any]:
    return {"content": [{"type": "text", "text": message}], "isError": True}
