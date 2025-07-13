"""String case conversion utilities."""

import re


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
