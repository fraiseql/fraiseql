"""Type registry and metadata management for FraiseQL database operations.

This module manages:
- Type registry: Maps view names to Python type classes
- Table metadata: Stores column information and JSONB configuration
- Type caching: Caches type lookups for performance
"""

import logging
from typing import Any

logger = logging.getLogger(__name__)

# Type registry for development mode
_type_registry: dict[str, type] = {}

# Table metadata registry - stores column information at registration time
# This avoids expensive runtime introspection
_table_metadata: dict[str, dict[str, Any]] = {}


def register_type_for_view(
    view_name: str,
    type_class: type,
    table_columns: set[str] | None = None,
    has_jsonb_data: bool | None = None,
    jsonb_column: str | None = None,
    fk_relationships: dict[str, str] | None = None,
    validate_fk_strict: bool = True,
) -> None:
    """Register a type class for a specific view name with optional metadata.

    This is used in development mode to instantiate proper types from view data.
    Storing metadata at registration time avoids expensive runtime introspection.

    Args:
        view_name: The database view name
        type_class: The Python type class decorated with @fraise_type
        table_columns: Optional set of actual database columns (for hybrid tables)
        has_jsonb_data: Optional flag indicating if table has a JSONB 'data' column
        jsonb_column: Optional name of the JSONB column (defaults to "data")
        fk_relationships: Map GraphQL field name → FK column name.
            Example: {"machine": "machine_id", "printer": "printer_id"}
            If not specified, uses convention: field + "_id"
        validate_fk_strict: If True, raise error on FK validation failures.
            If False, only warn (useful for legacy code migration).
    """
    _type_registry[view_name] = type_class
    logger.debug(f"Registered type {type_class.__name__} for view {view_name}")

    # Initialize FK relationships
    fk_relationships = fk_relationships or {}

    # Validate FK relationships if strict mode
    if validate_fk_strict and fk_relationships and table_columns:
        for field_name, fk_column in fk_relationships.items():
            if fk_column not in table_columns:
                raise ValueError(
                    f"Invalid FK relationship for {view_name}: "
                    f"Field '{field_name}' mapped to FK column '{fk_column}', "
                    f"but '{fk_column}' not in table_columns: {table_columns}. "
                    f"Either add '{fk_column}' to table_columns or fix fk_relationships. "
                    f"To allow this (not recommended), set validate_fk_strict=False.",
                )

    # Store metadata if provided
    if (
        table_columns is not None
        or has_jsonb_data is not None
        or jsonb_column is not None
        or fk_relationships
    ):
        metadata = {
            "columns": table_columns or set(),
            "has_jsonb_data": has_jsonb_data or False,
            "jsonb_column": jsonb_column,  # Always store the jsonb_column value
            "fk_relationships": fk_relationships,
            "validate_fk_strict": validate_fk_strict,
        }
        _table_metadata[view_name] = metadata
        logger.debug(
            f"Registered metadata for {view_name}: {len(table_columns or set())} columns, "
            f"jsonb={has_jsonb_data}, jsonb_column={jsonb_column}",
        )


def _get_type_for_view(view_name: str) -> type | None:
    """Get the registered type class for a view name.

    Args:
        view_name: The view name to look up

    Returns:
        The registered type class, or None if not found
    """
    return _type_registry.get(view_name)


def _ensure_table_columns_cached(view_name: str) -> set[str]:
    """Get table columns for a view, checking metadata cache first.

    Args:
        view_name: The view name

    Returns:
        Set of column names, or empty set if metadata not available
    """
    if view_name in _table_metadata:
        return _table_metadata[view_name].get("columns", set())
    return set()


def clear_type_registry() -> None:
    """Clear all registered types (useful for testing)."""
    _type_registry.clear()
    _table_metadata.clear()
