"""Lazy property descriptors for auto-generating WhereInput and OrderBy types."""

from typing import Any, TypeVar

T = TypeVar("T")

# Global cache for auto-generated types
_auto_generated_cache: dict[str, type] = {}


class LazyWhereInputProperty:
    """Descriptor for lazy WhereInput type generation."""

    def __get__(self, obj: Any, objtype: type[T]) -> type:
        """Generate and cache WhereInput type on first access."""
        if obj is not None:
            # Called on instance, not class - return bound descriptor
            return self

        # Called on class - generate WhereInput
        cache_key = f"{objtype.__module__}.{objtype.__name__}_WhereInput"

        if cache_key not in _auto_generated_cache:
            from fraiseql.sql.graphql_where_generator import create_graphql_where_input

            try:
                _auto_generated_cache[cache_key] = create_graphql_where_input(objtype)
            except Exception as e:
                msg = f"Failed to auto-generate WhereInput for {objtype.__name__}: {e}"
                raise RuntimeError(msg) from e

        return _auto_generated_cache[cache_key]


class LazyOrderByProperty:
    """Descriptor for lazy OrderBy type generation."""

    def __get__(self, obj: Any, objtype: type[T]) -> type:
        """Generate and cache OrderBy type on first access."""
        if obj is not None:
            return self

        cache_key = f"{objtype.__module__}.{objtype.__name__}_OrderBy"

        if cache_key not in _auto_generated_cache:
            from fraiseql.sql.graphql_order_by_generator import create_graphql_order_by_input

            try:
                _auto_generated_cache[cache_key] = create_graphql_order_by_input(objtype)
            except Exception as e:
                msg = f"Failed to auto-generate OrderBy for {objtype.__name__}: {e}"
                raise RuntimeError(msg) from e

        return _auto_generated_cache[cache_key]


def clear_auto_generated_cache() -> None:
    """Clear the auto-generated type cache (useful for testing)."""
    _auto_generated_cache.clear()
