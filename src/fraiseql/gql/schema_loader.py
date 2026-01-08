"""Python schema loader for Rust-exported GraphQL schemas.

This module loads and caches the complete GraphQL schema (filter types,
order by configurations) that Rust exports via fraiseql_rs.

Phase A.2: Schema Loading and Caching
Instead of generating schemas at runtime, Python loads pre-built schemas from Rust
and caches them in memory for high performance.

The schema provides:
- All filter types (String, Int, Float, etc.) with their operators
- OrderBy configuration with directions (ASC, DESC)
- Version information for compatibility checking
"""

import json
import logging
from typing import Any

logger = logging.getLogger(__name__)

# Cache for loaded schema
_cached_schema: dict[str, Any] | None = None


def load_schema() -> dict[str, Any]:
    """Load and cache the complete GraphQL schema from Rust.

    Returns the schema dictionary containing:
    - filter_schemas: All filter types with operators
    - order_by_schemas: OrderBy configuration
    - version: Schema version for compatibility

    Returns:
        Complete schema dictionary exported from Rust.

    Raises:
        ImportError: If fraiseql_rs is not available.
        json.JSONDecodeError: If schema JSON is invalid.
    """
    global _cached_schema

    # Return cached schema if already loaded
    if _cached_schema is not None:
        return _cached_schema

    # Import Rust module
    try:
        from fraiseql import fraiseql_rs
    except ImportError as e:
        logger.error("Failed to import fraiseql_rs: %s", e)
        raise

    # Call Rust FFI to export schema
    schema_json = fraiseql_rs.export_schema_generators()

    # Parse JSON string to dict
    try:
        schema = json.loads(schema_json)
    except json.JSONDecodeError as e:
        logger.error("Failed to parse schema JSON: %s", e)
        raise

    # Cache schema in memory
    _cached_schema = schema

    logger.debug("Loaded schema version %s", schema.get("version"))
    return schema


def _get_cached_schema() -> dict[str, Any] | None:
    """Get cached schema without reloading.

    Returns:
        Cached schema if loaded, None otherwise.
    """
    return _cached_schema


def get_filter_schema(type_name: str) -> dict[str, Any]:
    """Get filter schema for a specific type.

    Args:
        type_name: Name of the type (e.g., "String", "Int", "Array")

    Returns:
        Filter schema for the type containing fields dict.

    Raises:
        KeyError: If type not found in schema.
    """
    schema = load_schema()
    return schema["filter_schemas"][type_name]


def get_filter_operators(type_name: str) -> dict[str, dict[str, Any]]:
    """Get all operators for a filter type.

    Args:
        type_name: Name of the type (e.g., "String", "Int")

    Returns:
        Dictionary mapping operator names to their definitions.
        Each operator has "type" and "nullable" keys.

    Raises:
        KeyError: If type not found in schema.
    """
    filter_schema = get_filter_schema(type_name)
    return filter_schema["fields"]


def get_order_by_schema() -> dict[str, Any]:
    """Get OrderBy schema configuration.

    Returns:
        OrderBy schema containing directions and configuration.
    """
    schema = load_schema()
    return schema["order_by_schemas"]


def get_schema_version() -> str:
    """Get schema version for compatibility checking.

    Returns:
        Schema version string (e.g., "1.0").
    """
    schema = load_schema()
    return schema.get("version", "unknown")
