"""High-performance JSON passthrough wrapper for GraphQL responses.

This module provides a lightweight wrapper around JSON data that acts like a Python object
for GraphQL field resolution, but without the overhead of actual object instantiation.
"""

from typing import Any, Dict, Optional, Type, Union, get_args, get_origin

from fraiseql.config.schema_config import SchemaConfig
from fraiseql.utils.naming import snake_to_camel


class JSONPassthrough:
    """High-performance wrapper for direct JSON passthrough from database to GraphQL.

    This class provides a thin wrapper around dictionary data that mimics object
    attribute access, allowing GraphQL to resolve fields without instantiating
    actual Python objects. It includes:

    - Lazy evaluation for nested objects
    - Caching of wrapped nested objects
    - Automatic snake_case to camelCase conversion
    - Type hints for better error messages
    - __typename injection for GraphQL compatibility
    """

    __slots__ = (
        "_config",
        "_data",
        "_injected_typename",
        "_type_hint",
        "_type_name",
        "_wrapped_cache",
    )

    def __init__(
        self, data: dict, type_name: Optional[str] = None, type_hint: Optional[Type] = None
    ):
        """Initialize the JSON passthrough wrapper.

        Args:
            data: The dictionary data from the database
            type_name: The GraphQL type name (optional, will be extracted from data)
            type_hint: The Python type hint for validation and nested type resolution
        """
        self._data = data
        # Extract type name from various sources
        # Priority: data.__typename > explicit type_name > type_hint.__name__
        if "__typename" in data:
            self._type_name = data["__typename"]
        elif type_name:
            self._type_name = type_name
        elif type_hint and hasattr(type_hint, "__name__"):
            self._type_name = type_hint.__name__
        else:
            self._type_name = "Unknown"
        self._type_hint = type_hint
        self._wrapped_cache: Dict[str, Any] = {}
        self._config = SchemaConfig.get_instance()
        self._injected_typename = False

        # Ensure __typename is present for GraphQL compatibility
        if "__typename" not in self._data and self._type_name and self._type_name != "Unknown":
            self._data["__typename"] = self._type_name
            self._injected_typename = True

    def __getattr__(self, name: str) -> Any:
        """Get attribute value with automatic snake_case/camelCase conversion.

        This method is called when accessing attributes on the wrapper. It:
        1. Checks the cache for previously wrapped values
        2. Tries both snake_case and camelCase field names
        3. Wraps nested objects in JSONPassthrough recursively
        4. Returns scalar values directly

        Args:
            name: The attribute name being accessed

        Returns:
            The value from the underlying data, wrapped if necessary

        Raises:
            AttributeError: If the field doesn't exist in the data
        """
        # Check cache first
        if name in self._wrapped_cache:
            return self._wrapped_cache[name]

        # Special handling for __typename (handle Python name mangling)
        if name == "__typename" or name.endswith("__typename"):
            return self._data.get("__typename", self._type_name)

        # Try both snake_case and camelCase
        keys_to_try = [name]
        if self._config.camel_case_fields:
            camel_name = snake_to_camel(name)
            if camel_name != name:
                keys_to_try.append(camel_name)

        # Also try the reverse - if name is camelCase, try snake_case
        if any(c.isupper() for c in name):
            from fraiseql.utils.casing import to_snake_case

            snake_name = to_snake_case(name)
            if snake_name != name and snake_name not in keys_to_try:
                keys_to_try.append(snake_name)

        for key in keys_to_try:
            if key in self._data:
                value = self._data[key]

                # Handle None values
                if value is None:
                    return None

                # Handle nested objects
                if isinstance(value, dict):
                    # Determine nested type if available
                    nested_type_hint = self._get_nested_type_hint(name)

                    # Check if this field is declared as a plain dict type
                    if self._is_plain_dict_type(nested_type_hint):
                        # Return the dict directly without wrapping
                        return value

                    nested_type_name = self._get_nested_type_name(nested_type_hint, value)

                    wrapped = JSONPassthrough(value, nested_type_name, nested_type_hint)
                    self._wrapped_cache[name] = wrapped
                    return wrapped

                # Handle lists
                if isinstance(value, list):
                    # Check if it's a list of objects
                    if value and isinstance(value[0], dict):
                        item_type_hint = self._get_list_item_type_hint(name)
                        wrapped_list = [
                            JSONPassthrough(
                                item,
                                item.get(
                                    "__typename", self._get_nested_type_name(item_type_hint, item)
                                ),
                                item_type_hint,
                            )
                            for item in value
                        ]
                        self._wrapped_cache[name] = wrapped_list
                        return wrapped_list
                    # Return lists of scalars directly
                    return value

                # Return scalar values directly
                return value

        # Field not found - provide helpful error message
        # Exclude injected __typename from available fields
        available = sorted(
            [k for k in self._data if k != "__typename" or not self._injected_typename]
        )
        raise AttributeError(
            f"'{self._type_name}' object has no attribute '{name}'. Available fields: {available}"
        )

    def _is_plain_dict_type(self, type_hint: Optional[Type]) -> bool:
        """Check if a type hint represents a plain dict type (e.g., dict[str, Any])."""
        if not type_hint:
            return False

        origin = get_origin(type_hint)
        if origin is dict:
            return True

        # Handle Dict (from typing module)
        if hasattr(type_hint, "__origin__") and str(type_hint).startswith("typing.Dict"):
            return True

        return False

    def _get_nested_type_hint(self, field_name: str) -> Optional[Type]:
        """Get the type hint for a nested field."""
        if not self._type_hint or not hasattr(self._type_hint, "__annotations__"):
            return None

        # Get annotations from the type
        annotations = getattr(self._type_hint, "__annotations__", {})

        # Check both snake_case and original field name
        type_hint = annotations.get(field_name)
        if not type_hint and any(c.isupper() for c in field_name):
            from fraiseql.utils.casing import to_snake_case

            snake_name = to_snake_case(field_name)
            type_hint = annotations.get(snake_name)

        # Unwrap Optional types
        if type_hint:
            origin = get_origin(type_hint)
            if origin is Union:
                args = get_args(type_hint)
                # Filter out None to get the actual type
                non_none_types = [t for t in args if t is not type(None)]
                if non_none_types:
                    return non_none_types[0]

        return type_hint

    def _get_list_item_type_hint(self, field_name: str) -> Optional[Type]:
        """Get the type hint for items in a list field."""
        field_type_hint = self._get_nested_type_hint(field_name)
        if not field_type_hint:
            return None

        # Extract list item type
        origin = get_origin(field_type_hint)
        if origin in (list, List):
            args = get_args(field_type_hint)
            return args[0] if args else None

        return None

    def _get_nested_type_name(self, type_hint: Optional[Type], data: dict) -> str:
        """Determine the type name for a nested object."""
        # First, check if __typename is in the data
        if "__typename" in data:
            return data["__typename"]

        # Then try to get from type hint
        if type_hint and hasattr(type_hint, "__name__"):
            return type_hint.__name__

        # Default to Unknown
        return "Unknown"

    def __repr__(self):
        """Provide a readable representation for debugging."""
        # Show only the original fields, not injected __typename
        fields = [k for k in self._data if k != "__typename" or not self._injected_typename]
        return f"JSONPassthrough({self._type_name}, fields={fields})"

    def __str__(self):
        """Provide a string representation."""
        return f"{self._type_name} (passthrough)"

    # Make the wrapper more dict-like for compatibility
    def get(self, key: str, default: Any = None) -> Any:
        """Get a value with a default, like dict.get()."""
        try:
            return getattr(self, key)
        except AttributeError:
            return default

    def __contains__(self, key: str) -> bool:
        """Check if a field exists in the data."""
        # Check both exact key and with case conversion
        if key in self._data:
            return True

        if self._config.camel_case_fields:
            camel_key = snake_to_camel(key)
            if camel_key in self._data:
                return True

        return False

    @property
    def __typename(self):
        """Return the GraphQL type name."""
        return self._data.get("__typename", self._type_name)

    @property
    def __dict__(self):
        """Return the underlying data for compatibility with some GraphQL libraries."""
        return self._data


# Type alias for clarity
JSONData = Union[dict, JSONPassthrough]


def is_json_passthrough(obj: Any) -> bool:
    """Check if an object is a JSONPassthrough instance."""
    return isinstance(obj, JSONPassthrough)


def wrap_in_passthrough(
    data: Union[dict, list, Any], type_hint: Optional[Type] = None
) -> Union[JSONPassthrough, list[JSONPassthrough], Any]:
    """Wrap data in JSONPassthrough if it's a dict or list of dicts.

    Args:
        data: The data to potentially wrap
        type_hint: Optional type hint for the data

    Returns:
        Wrapped data if applicable, otherwise the original data
    """
    if isinstance(data, dict):
        type_name = data.get("__typename", "Unknown")
        if type_hint and hasattr(type_hint, "__name__"):
            type_name = type_hint.__name__
        return JSONPassthrough(data, type_name, type_hint)

    if isinstance(data, list) and data and isinstance(data[0], dict):
        # Get item type from type hint if available
        item_type = None
        if type_hint:
            origin = get_origin(type_hint)
            if origin in (list, List):
                args = get_args(type_hint)
                item_type = args[0] if args else None

        return [
            JSONPassthrough(
                item,
                item.get("__typename", item_type.__name__ if item_type else "Unknown"),
                item_type,
            )
            for item in data
        ]

    return data


# Import List for type hints
from typing import List
