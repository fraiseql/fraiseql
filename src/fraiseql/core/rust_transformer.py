"""FraiseQL-RS integration for ultra-fast JSON transformation.

This module provides integration between FraiseQL's GraphQL types and the
fraiseql-rs Rust extension for high-performance JSON transformation.
"""

import logging
from typing import Any, Optional, Type


# Lazy-load the Rust extension to avoid circular import issues
# The fraiseql package re-exports _fraiseql_rs in its __init__.py
def _get_fraiseql_rs():
    """Lazy-load the Rust extension module."""
    try:
        from fraiseql import _fraiseql_rs

        return _fraiseql_rs
    except ImportError as e:
        raise ImportError(
            "fraiseql Rust extension is not available. "
            "Please reinstall fraiseql: pip install --force-reinstall fraiseql"
        ) from e


# Create a namespace object that lazy-loads functions
class _FraiseQLRs:
    _module = None

    @staticmethod
    def to_camel_case(*args: Any, **kwargs: Any) -> Any:
        if _FraiseQLRs._module is None:
            _FraiseQLRs._module = _get_fraiseql_rs()
        return _FraiseQLRs._module.to_camel_case(*args, **kwargs)

    @staticmethod
    def transform_json(*args: Any, **kwargs: Any) -> Any:
        if _FraiseQLRs._module is None:
            _FraiseQLRs._module = _get_fraiseql_rs()
        return _FraiseQLRs._module.transform_json(*args, **kwargs)

    @staticmethod
    def build_graphql_response(*args: Any, **kwargs: Any) -> Any:
        if _FraiseQLRs._module is None:
            _FraiseQLRs._module = _get_fraiseql_rs()
        return _FraiseQLRs._module.build_graphql_response(*args, **kwargs)


fraiseql_rs = _FraiseQLRs()

logger = logging.getLogger(__name__)


class RustTransformer:
    """Manages fraiseql-rs JSON transformations (v0.2.0+).

    This class provides integration with fraiseql-rs for high-performance
    JSON transformation from snake_case to camelCase with __typename injection.

    Note: SchemaRegistry removed in v0.2.0 - transformation is now automatic.
    """

    def __init__(self) -> None:
        """Initialize the Rust transformer."""
        # SchemaRegistry removed in v0.2.0 - transformation now automatic!
        self._type_names: set[str] = set()  # Track registered types for validation
        logger.info("fraiseql-rs v0.2.0 transformer initialized")

    def register_type(self, type_class: Type, type_name: Optional[str] = None) -> None:
        """Register a GraphQL type name (schema analysis removed in v0.2.0).

        Note: fraiseql_rs v0.2.0 no longer requires schema registration.
        This method now just tracks type names for validation purposes.
        """
        type_name = type_name or type_class.__name__
        self._type_names.add(type_name)
        logger.debug(f"Registered type '{type_name}' (v0.2.0 - no schema needed)")

    def transform(self, json_str: str, root_type: str) -> str:
        """Transform JSON string to GraphQL response format.

        Args:
            json_str: JSON string with snake_case keys
            root_type: GraphQL type name for __typename injection

        Returns:
            GraphQL response JSON string with camelCase + __typename
        """
        # v0.2.0: Use build_graphql_response for single object
        response_bytes = fraiseql_rs.build_graphql_response(
            json_strings=[json_str],
            field_name="data",  # Generic wrapper field
            type_name=root_type,
            field_paths=None,
        )
        return response_bytes.decode("utf-8")

    def transform_json_passthrough(self, json_str: str, root_type: Optional[str] = None) -> str:
        """Transform JSON to camelCase (optionally with __typename).

        Args:
            json_str: JSON string with snake_case keys
            root_type: Optional type name for __typename injection

        Returns:
            Transformed JSON string with camelCase keys
        """
        if root_type:
            # With typename injection
            response_bytes = fraiseql_rs.build_graphql_response(
                json_strings=[json_str],
                field_name="data",
                type_name=root_type,
                field_paths=None,
            )
            return response_bytes.decode("utf-8")
        # CamelCase only (no typename)
        return fraiseql_rs.transform_json(json_str)


# Global singleton instance
_transformer: Optional[RustTransformer] = None


def get_transformer() -> RustTransformer:
    """Get the global RustTransformer instance.

    Returns:
        The singleton RustTransformer instance
    """
    global _transformer
    if _transformer is None:
        _transformer = RustTransformer()
    return _transformer


def register_graphql_types(*types: Type) -> None:
    """Register multiple GraphQL types with the Rust transformer.

    Args:
        *types: GraphQL type classes to register
    """
    transformer = get_transformer()
    for type_class in types:
        transformer.register_type(type_class)


def transform_db_json(json_str: str, root_type: str) -> str:
    """Transform database JSON to GraphQL response format.

    This is the main integration point for transforming PostgreSQL JSON
    results to GraphQL-compatible camelCase with __typename.

    Args:
        json_str: JSON string from database (snake_case)
        root_type: GraphQL type name

    Returns:
        Transformed JSON string (camelCase with __typename)
    """
    transformer = get_transformer()
    return transformer.transform(json_str, root_type)
