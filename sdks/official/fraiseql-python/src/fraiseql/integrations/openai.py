"""OpenAI function-calling integration for FraiseQL.

Exposes FraiseQL operations as OpenAI tool/function definitions and routes the
model's tool calls back through the typed SDK client — so authentication,
per-tenant context, and audit logging all reuse the client's existing path.

No extra dependency is required: the definitions are plain dicts in the shape
the OpenAI Chat Completions / Responses APIs accept.

Example:
    ```python
    from fraiseql.client import FraiseQLClient
    from fraiseql.integrations.openai import FraiseQLOpenAIFunctions

    client = FraiseQLClient("http://localhost:8080/graphql", auth_token=token)
    functions = await FraiseQLOpenAIFunctions.from_client(client)

    # 1. Hand the definitions to the model.
    tools = functions.definitions()

    # 2. When the model returns a tool call, execute it.
    result = await functions.call(tool_call.function.name, tool_call.function.arguments)
    ```
"""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, Any

from fraiseql.integrations._spec import OperationSpec, operation_specs

if TYPE_CHECKING:
    from collections.abc import Mapping

    from fraiseql.client import FraiseQLClient


def as_function_definitions(
    schema_data: dict[str, Any],
    *,
    include: list[str] | None = None,
    exclude: list[str] | None = None,
) -> list[dict[str, Any]]:
    """OpenAI tool definitions for every exposed FraiseQL operation.

    Args:
        schema_data: The introspection payload (``client.introspect()`` result).
        include: Whitelist of operation names (None = all).
        exclude: Blacklist of operation names (None = none).
    """
    return [
        _definition(spec) for spec in operation_specs(schema_data, include=include, exclude=exclude)
    ]


def _definition(spec: OperationSpec) -> dict[str, Any]:
    return {
        "type": "function",
        "function": {
            "name": spec.name,
            "description": spec.display_description,
            "parameters": spec.parameters_schema,
        },
    }


class FraiseQLOpenAIFunctions:
    """FraiseQL operations as OpenAI functions, with a call dispatcher."""

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
    ) -> FraiseQLOpenAIFunctions:
        """Introspect the connected server and build the function set."""
        schema_data = await client.introspect()
        return cls(client, schema_data, include=include, exclude=exclude)

    def definitions(self) -> list[dict[str, Any]]:
        """The OpenAI tool definitions, one per exposed operation."""
        return [_definition(spec) for spec in self._specs.values()]

    async def call(self, name: str, arguments: str | Mapping[str, Any] | None) -> dict[str, Any]:
        """Execute a model tool call through the typed client.

        Args:
            name: The function name from the tool call.
            arguments: The tool call's arguments — a JSON string (as the OpenAI
                API delivers it) or an already-parsed mapping.

        Returns:
            The operation's ``data`` payload.

        Raises:
            KeyError: If ``name`` is not an exposed operation — a model
                hallucinating a tool name must not become a server round-trip.
            ValueError: If ``arguments`` is a string but not valid JSON.
        """
        spec = self._specs[name]
        if isinstance(arguments, str):
            try:
                variables: dict[str, Any] = json.loads(arguments) if arguments else {}
            except json.JSONDecodeError as exc:
                msg = f"Tool call arguments for {name!r} are not valid JSON: {exc}"
                raise ValueError(msg) from exc
        else:
            variables = dict(arguments or {})

        result = await self._client.execute(spec.document, variables=variables or None)
        data: dict[str, Any] = result.get("data", {})
        return data
