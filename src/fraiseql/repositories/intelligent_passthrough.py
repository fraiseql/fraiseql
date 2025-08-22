"""Intelligent passthrough routing for optimal performance.

This module automatically selects the best execution path:
- Raw JSON passthrough when possible (maximum performance)
- Python object processing when needed (custom logic, field auth, etc.)

Users continue to use find() and find_one() - the optimization is transparent.
"""

import logging
from typing import Any

from fraiseql.core.raw_json_executor import RawJSONResult

logger = logging.getLogger(__name__)


class IntelligentPassthroughMixin:
    """Mixin that automatically selects optimal execution path for queries.

    Transparent to users - they continue using find() and find_one().
    Internally routes to either:
    - Raw JSON passthrough (PostgreSQL JSON → HTTP directly)
    - Python processing (when custom logic is needed)
    """

    async def find(self, view_name: str, **kwargs) -> Any:
        """Find records with automatic passthrough optimization."""
        # Check if we can use raw JSON passthrough
        if self._can_use_raw_passthrough(view_name, **kwargs):
            return await self._find_raw_passthrough(view_name, **kwargs)
        # Fall back to Python processing
        return await self._find_python_processing(view_name, **kwargs)

    async def find_one(self, view_name: str, **kwargs) -> Any:
        """Find single record with automatic passthrough optimization."""
        # Check if we can use raw JSON passthrough
        if self._can_use_raw_passthrough(view_name, **kwargs):
            return await self._find_one_raw_passthrough(view_name, **kwargs)
        # Fall back to Python processing
        return await self._find_one_python_processing(view_name, **kwargs)

    def _can_use_raw_passthrough(self, view_name: str, **kwargs) -> bool:
        """Determine if raw JSON passthrough is possible and beneficial.

        Raw passthrough is possible when:
        1. We're in production/passthrough mode
        2. No custom field resolvers are needed
        3. No field-level authorization
        4. No complex post-processing required
        5. GraphQL info is available for field mapping
        """
        # Must be in passthrough mode
        if not self._should_use_passthrough():
            logger.debug("Raw passthrough: disabled - not in passthrough mode")
            return False

        # Must have raw JSON capability
        if not hasattr(self, "find_raw_json") or not hasattr(self, "find_one_raw_json"):
            logger.debug("Raw passthrough: disabled - raw JSON methods not available")
            return False

        # Must have GraphQL field info for proper JSON generation
        context = getattr(self, "context", {})
        graphql_info = context.get("graphql_info")
        if not graphql_info:
            logger.debug("Raw passthrough: disabled - no GraphQL field info")
            return False

        # Check for field-level authorization
        if self._has_field_authorization(view_name):
            logger.debug("Raw passthrough: disabled - field authorization required")
            return False

        # Check for custom field resolvers
        if self._has_custom_resolvers(view_name):
            logger.debug("Raw passthrough: disabled - custom resolvers present")
            return False

        # Check for complex query features that need Python processing
        if self._needs_python_processing(**kwargs):
            logger.debug("Raw passthrough: disabled - complex query features")
            return False

        logger.debug("Raw passthrough: enabled - all conditions met")
        return True

    async def _find_raw_passthrough(self, view_name: str, **kwargs) -> RawJSONResult:
        """Execute find using raw JSON passthrough."""
        context = getattr(self, "context", {})
        field_name = context.get("graphql_field_name", "data")
        graphql_info = context.get("graphql_info")

        logger.debug(f"Executing raw passthrough find: {view_name}")
        return await self.find_raw_json(view_name, field_name, graphql_info, **kwargs)

    async def _find_one_raw_passthrough(self, view_name: str, **kwargs) -> RawJSONResult:
        """Execute find_one using raw JSON passthrough."""
        context = getattr(self, "context", {})
        field_name = context.get("graphql_field_name", "data")
        graphql_info = context.get("graphql_info")

        logger.debug(f"Executing raw passthrough find_one: {view_name}")
        return await self.find_one_raw_json(view_name, field_name, graphql_info, **kwargs)

    async def _find_python_processing(self, view_name: str, **kwargs) -> Any:
        """Execute find using Python object processing."""
        logger.debug(f"Executing Python processing find: {view_name}")

        # Call the original implementation (from parent class)
        result = await super().find(view_name, **kwargs)

        # Apply any passthrough wrapping if needed
        return self._wrap_as_raw_json_if_needed(result)

    async def _find_one_python_processing(self, view_name: str, **kwargs) -> Any:
        """Execute find_one using Python object processing."""
        logger.debug(f"Executing Python processing find_one: {view_name}")

        # Call the original implementation (from parent class)
        result = await super().find_one(view_name, **kwargs)

        # Apply any passthrough wrapping if needed
        return self._wrap_as_raw_json_if_needed(result)

    def _should_use_passthrough(self) -> bool:
        """Check if passthrough mode is enabled."""
        if not hasattr(self, "context"):
            return False

        context = self.context
        return (
            context.get("mode") in ("production", "staging")
            or context.get("json_passthrough", False)
            or context.get("execution_mode") == "passthrough"
            or context.get("_passthrough_enabled", False)
        )

    def _has_field_authorization(self, view_name: str) -> bool:
        """Check if the view/type has field-level authorization."""
        # This would check your authorization registry
        # Implementation depends on your auth system

        # Example implementation:
        # auth_registry = getattr(self, '_auth_registry', {})
        # return view_name in auth_registry

        return False  # Default: no field auth

    def _has_custom_resolvers(self, view_name: str) -> bool:
        """Check if the view/type has custom field resolvers."""
        # This would check your resolver registry
        # Implementation depends on your GraphQL schema setup

        # Example implementation:
        # resolver_registry = getattr(self, '_resolver_registry', {})
        # return view_name in resolver_registry

        return False  # Default: no custom resolvers

    def _needs_python_processing(self, **kwargs) -> bool:
        """Check if query parameters require Python processing."""
        # Complex where clauses that can't be handled by raw SQL
        where = kwargs.get("where")
        if where and hasattr(where, "_needs_python_processing"):
            return where._needs_python_processing()

        # Custom ordering that requires post-processing
        order_by = kwargs.get("order_by")
        if order_by and hasattr(order_by, "_needs_python_processing"):
            return order_by._needs_python_processing()

        # Pagination with complex cursors
        # Custom aggregations
        # etc.

        return False  # Default: no special processing needed

    def _wrap_as_raw_json_if_needed(self, result: Any) -> Any:
        """Wrap result for passthrough mode only if needed."""
        if not self._should_use_passthrough():
            return result

        # Already wrapped
        if isinstance(result, RawJSONResult):
            return result

        # Need to wrap for passthrough
        if result is None or isinstance(result, (list, dict)):
            context = getattr(self, "context", {})
            field_name = context.get("graphql_field_name", "data")

            # Use the enhanced JSON encoder
            import json

            from fraiseql.fastapi.json_encoder import FraiseQLJSONEncoder

            graphql_response = {"data": {field_name: result}}
            return RawJSONResult(json.dumps(graphql_response, cls=FraiseQLJSONEncoder))

        return result


class AutoOptimizedRepository(IntelligentPassthroughMixin):
    """Repository that automatically optimizes between raw JSON and Python processing.

    Usage:
    ------
    # Users write the same code as before
    users = await repo.find("v_user", where={"active": True})
    user = await repo.find_one("v_user", id=user_id)

    # But internally:
    # - Simple queries → Raw JSON passthrough (10-100x faster)
    # - Complex queries → Python processing (when needed)
    """

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)

        # Performance metrics
        self._raw_passthrough_count = 0
        self._python_processing_count = 0

    async def find(self, *args, **kwargs):
        """Optimized find with automatic path selection."""
        if hasattr(self, "_can_use_raw_passthrough") and self._can_use_raw_passthrough(
            args[0] if args else "", **kwargs
        ):
            self._raw_passthrough_count += 1
        else:
            self._python_processing_count += 1

        return await super().find(*args, **kwargs)

    async def find_one(self, *args, **kwargs):
        """Optimized find_one with automatic path selection."""
        if hasattr(self, "_can_use_raw_passthrough") and self._can_use_raw_passthrough(
            args[0] if args else "", **kwargs
        ):
            self._raw_passthrough_count += 1
        else:
            self._python_processing_count += 1

        return await super().find_one(*args, **kwargs)

    def get_performance_stats(self) -> dict:
        """Get performance optimization statistics."""
        total = self._raw_passthrough_count + self._python_processing_count
        if total == 0:
            return {"raw_passthrough_rate": 0, "total_queries": 0}

        return {
            "total_queries": total,
            "raw_passthrough_count": self._raw_passthrough_count,
            "python_processing_count": self._python_processing_count,
            "raw_passthrough_rate": self._raw_passthrough_count / total,
            "performance_boost": f"{self._raw_passthrough_count}x queries optimized",
        }


# Example usage patterns that work automatically:
#
# async def example_usage(repo: AutoOptimizedRepository):
#     """Examples showing automatic optimization."""
#     # Simple query → Raw JSON passthrough (fast)
#     await repo.find("v_user", where={"active": True})
#
#     # Single lookup → Raw JSON passthrough (fast)
#     await repo.find_one("v_user", id="user-123")
#
#     # Get optimization stats
#     stats = repo.get_performance_stats()
#     logger.info(f"Optimized {stats['raw_passthrough_count']} of {stats['total_queries']} queries")
