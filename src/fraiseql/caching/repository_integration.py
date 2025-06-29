"""Integration of caching with FraiseQLRepository.

This module provides a cached wrapper for FraiseQLRepository that
automatically caches query results and invalidates cache on mutations.
"""

import logging
from typing import Any, Optional, Union

from fraiseql.db import FraiseQLRepository

from .cache_key import CacheKeyBuilder
from .result_cache import ResultCache

logger = logging.getLogger(__name__)


class CachedRepository(FraiseQLRepository):
    """Repository wrapper that adds caching functionality."""

    def __init__(
        self,
        base_repository: FraiseQLRepository,
        cache: ResultCache,
    ) -> None:
        """Initialize cached repository.

        Args:
            base_repository: The underlying FraiseQLRepository
            cache: ResultCache instance to use
        """
        # Don't call super().__init__ as we're wrapping, not extending
        self._base = base_repository
        self._cache = cache
        self._key_builder = CacheKeyBuilder()

        # Copy attributes from base repository
        self._pool = base_repository._pool
        self.context = base_repository.context
        self.mode = base_repository.mode

    async def find(
        self,
        view_name: str,
        skip_cache: bool = False,
        cache_ttl: Optional[int] = None,
        **kwargs,
    ) -> list[Union[dict[str, Any], Any]]:
        """Find records with caching support.

        Args:
            view_name: Name of the view to query
            skip_cache: If True, bypass cache
            cache_ttl: Custom TTL for this query
            **kwargs: Query filters and options

        Returns:
            List of records
        """
        if skip_cache:
            return await self._base.find(view_name, **kwargs)

        # Build cache key
        cache_key = self._key_builder.build_key(
            query_name=view_name,
            filters=kwargs,
        )

        # Use cache
        return await self._cache.get_or_set(
            key=cache_key,
            func=lambda: self._base.find(view_name, **kwargs),
            ttl=cache_ttl,
        )

    async def find_one(
        self,
        view_name: str,
        skip_cache: bool = False,
        cache_ttl: Optional[int] = None,
        **kwargs,
    ) -> Optional[Union[dict[str, Any], Any]]:
        """Find single record with caching support.

        Args:
            view_name: Name of the view to query
            skip_cache: If True, bypass cache
            cache_ttl: Custom TTL for this query
            **kwargs: Query filters

        Returns:
            Single record or None
        """
        if skip_cache:
            return await self._base.find_one(view_name, **kwargs)

        # Build cache key
        cache_key = self._key_builder.build_key(
            query_name=f"{view_name}:one",
            filters=kwargs,
        )

        # Use cache
        return await self._cache.get_or_set(
            key=cache_key,
            func=lambda: self._base.find_one(view_name, **kwargs),
            ttl=cache_ttl,
        )

    async def execute_function(
        self,
        function_name: str,
        input_data: dict[str, Any],
    ) -> dict[str, Any]:
        """Execute SQL function and invalidate related cache.

        Args:
            function_name: Name of the function to execute
            input_data: Input data for the function

        Returns:
            Function result
        """
        # Execute the function
        result = await self._base.execute_function(function_name, input_data)

        # Invalidate cache based on function name
        # Extract table name from function name (e.g., "create_user" -> "user")
        parts = function_name.split("_")
        if len(parts) >= 2:
            # Common patterns: create_user, update_user, delete_user
            table_name = parts[-1]
            pattern = self._key_builder.build_mutation_pattern(table_name)
            await self._cache.invalidate_pattern(pattern)

            # Also invalidate plural form (e.g., user -> users)
            plural_pattern = self._key_builder.build_mutation_pattern(f"{table_name}s")
            await self._cache.invalidate_pattern(plural_pattern)

        return result

    async def execute_function_with_context(
        self,
        function_name: str,
        context_args: tuple[Any, ...],
        input_data: dict[str, Any],
    ) -> dict[str, Any]:
        """Execute SQL function with context and invalidate cache.

        Args:
            function_name: Name of the function to execute
            context_args: Context arguments (e.g., tenant_id)
            input_data: Input data for the function

        Returns:
            Function result
        """
        # Execute the function
        result = await self._base.execute_function_with_context(
            function_name,
            context_args,
            input_data,
        )

        # Invalidate cache (same logic as execute_function)
        parts = function_name.split("_")
        if len(parts) >= 2:
            table_name = parts[-1]
            pattern = self._key_builder.build_mutation_pattern(table_name)
            await self._cache.invalidate_pattern(pattern)

            plural_pattern = self._key_builder.build_mutation_pattern(f"{table_name}s")
            await self._cache.invalidate_pattern(plural_pattern)

        return result

    # Delegate all other methods to base repository
    def __getattr__(self, name: str) -> Any:
        """Delegate unknown attributes to base repository."""
        return getattr(self._base, name)
