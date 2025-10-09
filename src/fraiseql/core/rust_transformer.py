"""FraiseQL-RS integration for ultra-fast JSON transformation.

This module provides integration between FraiseQL's GraphQL types and the
fraiseql-rs Rust extension for high-performance JSON transformation.
"""

import logging
from typing import Any, Dict, Optional, Type, get_args, get_origin

try:
    import fraiseql_rs

    FRAISEQL_RS_AVAILABLE = True
except ImportError:
    FRAISEQL_RS_AVAILABLE = False
    fraiseql_rs = None

logger = logging.getLogger(__name__)


class RustTransformer:
    """Manages fraiseql-rs schema registry and JSON transformations.

    This class builds a fraiseql-rs schema from FraiseQL GraphQL types
    and provides methods to transform JSON payloads from snake_case to
    camelCase with __typename injection.
    """

    def __init__(self):
        """Initialize the Rust transformer."""
        self._registry: Optional[Any] = None
        self._schema: Dict[str, Dict] = {}
        self._enabled = FRAISEQL_RS_AVAILABLE

        if self._enabled:
            self._registry = fraiseql_rs.SchemaRegistry()
            logger.info("fraiseql-rs transformer initialized")
        else:
            logger.warning("fraiseql-rs not available - falling back to Python transformations")

    @property
    def enabled(self) -> bool:
        """Check if Rust transformer is available and enabled."""
        return self._enabled and self._registry is not None

    def register_type(self, type_class: Type, type_name: Optional[str] = None) -> None:
        """Register a GraphQL type with the Rust transformer.

        Args:
            type_class: The FraiseQL/Strawberry GraphQL type class
            type_name: Optional type name (defaults to class name)
        """
        if not self.enabled:
            return

        type_name = type_name or type_class.__name__

        # Build field schema from type annotations
        fields = {}

        # Get annotations from the type
        annotations = getattr(type_class, "__annotations__", {})

        for field_name, field_type in annotations.items():
            # Skip private fields
            if field_name.startswith("_"):
                continue

            # Map Python type to fraiseql-rs schema type
            schema_type = self._map_python_type_to_schema(field_type)
            if schema_type:
                fields[field_name] = schema_type

        # Register with fraiseql-rs
        type_def = {"fields": fields}
        self._schema[type_name] = type_def
        self._registry.register_type(type_name, type_def)

        logger.debug(f"Registered type '{type_name}' with {len(fields)} fields")

    def _map_python_type_to_schema(self, python_type: Type) -> Optional[str]:
        """Map Python type annotation to fraiseql-rs schema type string.

        Args:
            python_type: Python type annotation

        Returns:
            Schema type string (e.g., "Int", "String", "[Post]")
        """
        # Handle Optional types
        origin = get_origin(python_type)
        if origin is type(None):
            return None

        # Unwrap Optional[T] -> T
        from typing import Union

        if origin is Union:
            args = get_args(python_type)
            non_none_types = [t for t in args if t is not type(None)]
            if non_none_types:
                python_type = non_none_types[0]
                origin = get_origin(python_type)

        # Handle list types
        if origin is list:
            args = get_args(python_type)
            if args:
                inner_type = self._map_python_type_to_schema(args[0])
                if inner_type:
                    return f"[{inner_type}]"
            return None

        # Handle basic types
        if python_type is int:
            return "Int"
        if python_type is str:
            return "String"
        if python_type is bool:
            return "Boolean"
        if python_type is float:
            return "Float"

        # Handle dict (should not add __typename)
        if origin is dict:
            return None  # Skip dict fields

        # Handle custom types (objects)
        if hasattr(python_type, "__name__"):
            return python_type.__name__

        return None

    def transform(self, json_str: str, root_type: str) -> str:
        """Transform JSON string using Rust transformer.

        Args:
            json_str: JSON string with snake_case keys
            root_type: Root GraphQL type name

        Returns:
            Transformed JSON string with camelCase keys and __typename
        """
        if not self.enabled:
            # Fallback to Python transformation
            import json

            from fraiseql.utils.casing import transform_keys_to_camel_case

            data = json.loads(json_str)
            transformed = transform_keys_to_camel_case(data)
            # Add __typename
            if isinstance(transformed, dict):
                transformed["__typename"] = root_type
            return json.dumps(transformed)

        # Use Rust transformer
        try:
            return self._registry.transform(json_str, root_type)
        except Exception as e:
            logger.error(f"Rust transformation failed: {e}, falling back to Python")
            # Fallback to Python
            import json

            from fraiseql.utils.casing import transform_keys_to_camel_case

            data = json.loads(json_str)
            transformed = transform_keys_to_camel_case(data)
            if isinstance(transformed, dict):
                transformed["__typename"] = root_type
            return json.dumps(transformed)

    def transform_json_passthrough(self, json_str: str, root_type: Optional[str] = None) -> str:
        """Transform JSON without typename if not needed.

        Args:
            json_str: JSON string with snake_case keys
            root_type: Optional root type for __typename injection

        Returns:
            Transformed JSON string with camelCase keys
        """
        if not self.enabled:
            import json

            from fraiseql.utils.casing import transform_keys_to_camel_case

            data = json.loads(json_str)
            transformed = transform_keys_to_camel_case(data)
            return json.dumps(transformed)

        # Use Rust transformer
        try:
            if root_type and root_type in self._schema:
                return self._registry.transform(json_str, root_type)
            # Use plain transform_json for camelCase only
            return fraiseql_rs.transform_json(json_str)
        except Exception as e:
            logger.error(f"Rust transformation failed: {e}, falling back to Python")
            import json

            from fraiseql.utils.casing import transform_keys_to_camel_case

            data = json.loads(json_str)
            transformed = transform_keys_to_camel_case(data)
            return json.dumps(transformed)


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
