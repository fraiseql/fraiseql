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
