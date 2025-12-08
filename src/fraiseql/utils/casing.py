"""String case conversion utilities."""

import re
from typing import Any


def to_camel_case(s: str) -> str:
    """Convert snake_case to camelCase."""
    parts = s.split("_")
    return parts[0] + "".join(word.capitalize() for word in parts[1:])


def to_snake_case(s: str) -> str:
    """Convert camelCase to snake_case."""
    # Handle consecutive capitals like APIKey -> api_key
    # First, insert underscores between lowercase and uppercase
    s1 = re.sub("(.)([A-Z][a-z]+)", r"\1_\2", s)
    # Then handle the sequence of capitals followed by a lowercase letter
    s2 = re.sub("([a-z0-9])([A-Z])", r"\1_\2", s1)
    return s2.lower()


def transform_keys_to_camel_case(data: Any) -> Any:
    """Recursively transform dictionary keys from snake_case to camelCase.

    Args:
        data: The data to transform (dict, list, or primitive value)

    Returns:
        The transformed data with camelCase keys
    """
    if isinstance(data, dict):
        return {to_camel_case(k): transform_keys_to_camel_case(v) for k, v in data.items()}
    if isinstance(data, list):
        return [transform_keys_to_camel_case(item) for item in data]
    return data


def dict_keys_to_snake_case(data: dict | list | Any) -> dict | list | Any:
    """Recursively convert dictionary keys from camelCase to snake_case.

    This function is used to convert GraphQL input (camelCase) to PostgreSQL-compatible
    format (snake_case) before serializing to JSONB.

    Args:
        data: Input data structure (dict, list, or primitive)

    Returns:
        Data structure with all dict keys converted to snake_case

    Examples:
        >>> dict_keys_to_snake_case({"firstName": "John", "lastName": "Doe"})
        {'first_name': 'John', 'last_name': 'Doe'}

        >>> dict_keys_to_snake_case({"user": {"emailAddress": "john@example.com"}})
        {'user': {'email_address': 'john@example.com'}}

        >>> dict_keys_to_snake_case({"items": [{"itemName": "A"}, {"itemName": "B"}]})
        {'items': [{'item_name': 'A'}, {'item_name': 'B'}]}
    """
    if isinstance(data, dict):
        # Recursively convert all keys in the dict
        return {to_snake_case(key): dict_keys_to_snake_case(value) for key, value in data.items()}
    if isinstance(data, list):
        # Recursively convert all items in the list
        return [dict_keys_to_snake_case(item) for item in data]
    # Primitive value - return as-is
    return data
