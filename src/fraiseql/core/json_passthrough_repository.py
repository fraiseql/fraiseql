"""Repository wrapper that enables automatic JSON passthrough in production mode."""

import json
from typing import Any, Dict, Optional

from fraiseql.core.raw_json_executor import RawJSONResult
from fraiseql.fastapi.json_encoder import FraiseQLJSONEncoder


class JsonPassthroughRepositoryMixin:
    """Mixin that adds JSON passthrough capabilities to repositories.

    When the repository detects it's in production/turbo mode via the context,
    it automatically returns RawJSONResult instead of Python objects.
    """

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self._passthrough_enabled = False
        self._current_field_name = None

    def _should_use_passthrough(self) -> bool:
        """Check if passthrough mode should be used."""
        if not hasattr(self, "context"):
            return False

        context = getattr(self, "context", {})

        # Check various indicators
        return (
            context.get("mode") in ("production", "staging")
            or context.get("json_passthrough", False)
            or context.get("execution_mode") == "passthrough"
            or self._passthrough_enabled
        )

    def _wrap_result_as_json(self, result: Any, field_name: str) -> Any:
        """Wrap result as RawJSONResult if in passthrough mode."""
        if not self._should_use_passthrough():
            return result

        # Don't wrap if already RawJSONResult
        if isinstance(result, RawJSONResult):
            return result

        # Get field name from context if available
        if not field_name and hasattr(self, "context"):
            field_name = self.context.get("graphql_field_name", "data")

        # For lists and dicts, wrap as raw JSON using FraiseQLJSONEncoder
        # This ensures PostgreSQL types are properly converted while preserving JSON types
        if isinstance(result, (list, dict)):
            # Create GraphQL response structure
            graphql_response = {"data": {field_name: result}}
            return RawJSONResult(json.dumps(graphql_response, cls=FraiseQLJSONEncoder))

        # For None, also wrap
        if result is None:
            graphql_response = {"data": {field_name: None}}
            return RawJSONResult(json.dumps(graphql_response, cls=FraiseQLJSONEncoder))

        # For other types, return as-is
        return result

    async def find(self, *args, **kwargs) -> Any:
        """Find with automatic JSON passthrough."""
        result = await super().find(*args, **kwargs)

        # Get field name from context
        field_name = None
        if hasattr(self, "context"):
            field_name = self.context.get("graphql_field_name")

        return self._wrap_result_as_json(result, field_name)

    async def find_one(self, *args, **kwargs) -> Any:
        """Find one with automatic JSON passthrough."""
        result = await super().find_one(*args, **kwargs)

        # Get field name from context
        field_name = None
        if hasattr(self, "context"):
            field_name = self.context.get("graphql_field_name")

        return self._wrap_result_as_json(result, field_name)

    async def query(self, *args, **kwargs) -> Any:
        """Query with automatic JSON passthrough."""
        result = await super().query(*args, **kwargs)

        # Get field name from context
        field_name = None
        if hasattr(self, "context"):
            field_name = self.context.get("graphql_field_name")

        return self._wrap_result_as_json(result, field_name)

    async def execute_turbo_query(
        self, sql: str, params: Optional[Dict[str, Any]] = None
    ) -> RawJSONResult:
        """Execute a TurboRouter SQL query and return raw JSON.

        This method is specifically for TurboRouter queries and always
        returns RawJSONResult to bypass GraphQL typing.
        """
        # Execute the SQL and get raw JSON from PostgreSQL
        # This assumes the SQL uses json_agg or similar to return JSON
        conn = getattr(self, "_conn", None) or getattr(self, "conn", None)
        if not conn:
            raise RuntimeError("No database connection available")

        async with conn.cursor() as cur:
            await cur.execute(sql, params or {})
            result = await cur.fetchone()

            # Assume the query returns a single JSON column
            if result and result[0]:
                # If it's already a string (from json_agg), use it directly
                if isinstance(result[0], str):
                    return RawJSONResult(result[0])
                # Otherwise convert to JSON
                return RawJSONResult(json.dumps(result[0]))

            # Return empty result
            return RawJSONResult('{"data": {}}')
