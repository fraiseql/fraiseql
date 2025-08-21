"""Repository mixin for automatic JSON passthrough support."""

import json
import logging
from typing import Any

from fraiseql.core.raw_json_executor import RawJSONResult
from fraiseql.fastapi.json_encoder import FraiseQLJSONEncoder

logger = logging.getLogger(__name__)


class PassthroughMixin:
    """Mixin that adds automatic JSON passthrough to repository methods.

    When the repository detects passthrough mode from the context,
    it returns RawJSONResult instead of Python objects, bypassing
    GraphQL type validation entirely.
    """

    def _should_use_passthrough(self) -> bool:
        """Check if passthrough mode is enabled."""
        if not hasattr(self, "context"):
            logger.debug("PassthroughMixin: No context attribute")
            return False

        context = self.context
        # Only enable passthrough if explicitly set in context
        # Do NOT enable just because mode is production/staging
        result = (
            context.get("json_passthrough", False)
            or context.get("execution_mode") == "passthrough"
            or context.get("_passthrough_enabled", False)
        )
        logger.info(
            f"PassthroughMixin: _should_use_passthrough = {result}, "
            f"mode={context.get('mode')}, json_passthrough={context.get('json_passthrough')}"
        )
        return result

    def _get_field_name(self) -> str:
        """Get the current GraphQL field name from context."""
        if hasattr(self, "context"):
            return (
                self.context.get("_passthrough_field")
                or self.context.get("graphql_field_name")
                or "data"
            )
        return "data"

    def _wrap_as_raw_json(self, result: Any) -> Any:
        """Wrap result as RawJSONResult if in passthrough mode.

        WARNING: This method should generally NOT be used in production.
        True passthrough should use find_raw_json() / find_one_raw_json()
        which return raw JSON strings from PostgreSQL without any Python processing.

        This method exists only as a fallback for cases where raw JSON methods
        aren't available.
        """
        if not self._should_use_passthrough():
            return result

        # Don't double-wrap
        if isinstance(result, RawJSONResult):
            return result

        # CRITICAL: Log when this fallback path is used
        import logging

        logger = logging.getLogger(__name__)
        logger.warning(
            "PassthroughMixin fallback used - this should be avoided in production. "
            "Use find_raw_json() / find_one_raw_json() for true passthrough."
        )

        # Get field name
        field_name = self._get_field_name()

        # For None, lists, and dicts, wrap as raw JSON using FraiseQLJSONEncoder
        # This ensures PostgreSQL types are converted while preserving JSON-native types
        if result is None or isinstance(result, (list, dict)):
            # Create the GraphQL response structure
            graphql_response = {"data": {field_name: result}}
            return RawJSONResult(json.dumps(graphql_response, cls=FraiseQLJSONEncoder))

        # For other types, return as-is
        return result

    # Override common repository methods to add passthrough support

    async def find(self, *args, **kwargs):
        """Find with automatic passthrough support."""
        result = await super().find(*args, **kwargs)
        return self._wrap_as_raw_json(result)

    async def find_one(self, *args, **kwargs):
        """Find one with automatic passthrough support."""
        logger.info("PassthroughMixin.find_one called, passthrough check...")
        result = await super().find_one(*args, **kwargs)
        wrapped = self._wrap_as_raw_json(result)
        logger.info(
            f"PassthroughMixin.find_one: result type={type(result).__name__}, "
            f"wrapped type={type(wrapped).__name__}"
        )
        return wrapped

    async def query(self, *args, **kwargs):
        """Query with automatic passthrough support."""
        result = await super().query(*args, **kwargs)
        return self._wrap_as_raw_json(result)

    async def get(self, *args, **kwargs):
        """Get with automatic passthrough support."""
        result = await super().get(*args, **kwargs)
        return self._wrap_as_raw_json(result)

    # For repositories that have these common methods
    async def query_with_tenant(self, *args, **kwargs):
        """Query with tenant and automatic passthrough support."""
        result = await super().query_with_tenant(*args, **kwargs)
        return self._wrap_as_raw_json(result)

    async def get_by_id_with_tenant(self, *args, **kwargs):
        """Get by ID with tenant and automatic passthrough support."""
        result = await super().get_by_id_with_tenant(*args, **kwargs)
        return self._wrap_as_raw_json(result)
