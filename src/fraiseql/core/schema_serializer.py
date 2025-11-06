"""GraphQL Schema Serializer for Rust Schema Registry.

This module serializes GraphQL schemas to a JSON intermediate representation (IR)
that can be passed to Rust for type resolution and transformation.

The serialized schema includes:
- Version information for forward compatibility
- Feature flags to document capabilities
- Type metadata for all GraphQL object types
- Field information including type names and nesting indicators

Design Notes:
- Uses versioned IR format to support future enhancements without breaking changes
- Includes feature flags to document capabilities
- Focuses on object types and their fields (scalars, interfaces, unions not needed
  for type resolution)
- Tracks nested object relationships for recursive transformation in Rust
"""

import logging
from typing import Any

from graphql import (
    GraphQLList,
    GraphQLNamedType,
    GraphQLNonNull,
    GraphQLObjectType,
    GraphQLSchema,
)

logger = logging.getLogger(__name__)


class SchemaSerializer:
    """Serializes GraphQL schemas to JSON IR format for Rust schema registry.

    The serialized format includes:
    - version: Schema IR version (for forward compatibility)
    - features: List of capabilities supported by this serialization
    - types: Dictionary mapping type names to type metadata

    Example output:
    {
        "version": "1.0",
        "features": ["type_resolution", "aliases"],
        "types": {
            "User": {
                "fields": {
                    "id": {
                        "type_name": "UUID",
                        "is_nested_object": false,
                        "is_list": false
                    },
                    "name": {
                        "type_name": "String",
                        "is_nested_object": false,
                        "is_list": false
                    }
                }
            }
        }
    }
    """

    VERSION = "1.0"
    FEATURES = ["type_resolution", "aliases"]

    def serialize_schema(self, schema: GraphQLSchema) -> dict[str, Any]:
        """Serialize a GraphQL schema to JSON IR format.

        Args:
            schema: The GraphQL schema to serialize

        Returns:
            Dictionary containing the serialized schema in IR format

        Raises:
            ValueError: If schema is None or invalid
        """
        if schema is None:
            raise ValueError("Schema cannot be None")

        logger.debug("Starting schema serialization")

        result = {
            "version": self.VERSION,
            "features": self.FEATURES.copy(),
            "types": {},
        }

        # Iterate through all types in the schema
        type_count = 0
        for type_name, type_def in schema.type_map.items():
            # Skip internal GraphQL types (starting with __)
            if type_name.startswith("__"):
                continue

            # Only serialize object types (not scalars, interfaces, unions, enums)
            if isinstance(type_def, GraphQLObjectType):
                try:
                    result["types"][type_name] = self._serialize_object_type(type_def)
                    type_count += 1
                    logger.debug(
                        f"Serialized type '{type_name}' with {len(type_def.fields)} fields"
                    )
                except Exception as e:
                    logger.warning(f"Failed to serialize type '{type_name}': {e}")
                    # Continue with other types rather than failing completely

        logger.info(f"Schema serialization complete: {type_count} types serialized")
        return result

    def _serialize_object_type(self, type_def: GraphQLObjectType) -> dict[str, Any]:
        """Serialize a GraphQL object type.

        Args:
            type_def: The GraphQL object type to serialize

        Returns:
            Dictionary containing field metadata
        """
        fields = {}

        for field_name, field_def in type_def.fields.items():
            fields[field_name] = self._serialize_field(field_def.type)

        return {"fields": fields}

    def _serialize_field(self, field_type: Any) -> dict[str, Any]:
        """Serialize a GraphQL field type.

        Args:
            field_type: The GraphQL field type (may be wrapped in NonNull/List)

        Returns:
            Dictionary with type_name, is_nested_object, and is_list flags
        """
        # Unwrap NonNull wrapper
        if isinstance(field_type, GraphQLNonNull):
            field_type = field_type.of_type

        # Check for List wrapper
        is_list = False
        if isinstance(field_type, GraphQLList):
            is_list = True
            field_type = field_type.of_type

            # Unwrap inner NonNull if present (e.g., [User!])
            if isinstance(field_type, GraphQLNonNull):
                field_type = field_type.of_type

        # Get the base type name
        type_name = field_type.name if isinstance(field_type, GraphQLNamedType) else str(field_type)

        # Determine if this is a nested object type
        is_nested_object = isinstance(field_type, GraphQLObjectType)

        return {
            "type_name": type_name,
            "is_nested_object": is_nested_object,
            "is_list": is_list,
        }
