"""Naming convention utilities for FraiseQL."""

from __future__ import annotations

import re


def snake_to_camel(name: str) -> str:
    """Convert snake_case to camelCase.

    Examples:
        snake_to_camel("user_name") -> "userName"
        snake_to_camel("get_user_by_id") -> "getUserById"
        snake_to_camel("is_active") -> "isActive"
        snake_to_camel("created_at_timestamp") -> "createdAtTimestamp"

    Special cases:
        - Already camelCase/PascalCase strings are preserved
        - All uppercase (e.g., "URL", "API") are preserved
        - Mixed case with no underscores is preserved
    """
    # If there are no underscores and the name has mixed case, preserve it
    if "_" not in name and not name.islower() and not name.isupper():
        return name

    # If it's all uppercase and has no underscores, preserve it
    if name.isupper() and "_" not in name:
        return name

    # Split by underscores
    parts = name.split("_")
    if not parts:
        return name

    # First part stays lowercase, rest are capitalized
    result = parts[0].lower()
    for part in parts[1:]:
        if part:  # Skip empty parts from double underscores
            # Preserve all-caps parts like "URL" or "API"
            if part.isupper() and len(part) > 1:
                result += part
            else:
                result += part.capitalize()

    return result


def camel_to_snake(name: str) -> str:
    """Convert camelCase or PascalCase to snake_case.

    Examples:
        camel_to_snake("userName") -> "user_name"
        camel_to_snake("getUserById") -> "get_user_by_id"
        camel_to_snake("isActive") -> "is_active"
        camel_to_snake("HTTPTimeout") -> "http_timeout"
        camel_to_snake("dns1Id") -> "dns_1_id"
    """
    # Handle letter followed by number (dns1Id -> dns_1Id)
    s0 = re.sub("([a-z])([0-9])", r"\1_\2", name)
    # Handle number followed directly by capital letter (dns_1Id -> dns_1_Id)
    s1 = re.sub("([0-9])([A-Z][a-z]*)", r"\1_\2", s0)
    # Handle sequences of capitals followed by a lowercase letter (but not after underscores)
    s2 = re.sub("([^_])([A-Z][a-z]+)", r"\1_\2", s1)
    # Handle lowercase followed by capital
    s3 = re.sub("([a-z])([A-Z])", r"\1_\2", s2)
    return s3.lower()


def is_snake_case(name: str) -> bool:
    """Check if a name is in snake_case format."""
    return bool(re.match(r"^[a-z][a-z0-9_]*$", name))


def is_camel_case(name: str) -> bool:
    """Check if a name is in camelCase format."""
    return bool(re.match(r"^[a-z][a-zA-Z0-9]*$", name)) and not name.islower()


def to_snake_case(name: str) -> str:
    """Convert any case to snake_case.

    This is an alias for camel_to_snake that handles any input.
    """
    return camel_to_snake(name)
