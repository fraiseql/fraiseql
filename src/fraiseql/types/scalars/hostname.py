"""Hostname scalar type for DNS hostname validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# RFC 1123 compliant hostname regex
# - Each label must be 1-63 characters
# - Labels can contain a-z, A-Z, 0-9, and hyphens
# - Labels cannot start or end with hyphens
# - Total hostname length must not exceed 253 characters
_HOSTNAME_REGEX = re.compile(
    r"^(?=.{1,253}$)"  # Total length check
    r"(?!-)"  # Cannot start with hyphen
    r"(?!.*--)"  # Cannot contain consecutive hyphens
    r"((?!-)[a-zA-Z0-9-]{1,63}(?<!-)\.)*"  # Labels with dots
    r"(?!-)[a-zA-Z0-9-]{1,63}(?<!-)$"  # Final label without dot
)


def serialize_hostname(value: Any) -> str | None:
    """Serialize hostname to string."""
    if value is None:
        return None

    value_str = str(value).lower()  # Hostnames are case-insensitive

    if not _HOSTNAME_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid hostname: {value}. Must be a valid DNS hostname (RFC 1123 compliant)"
        )

    return value_str


def parse_hostname_value(value: Any) -> str:
    """Parse hostname from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Hostname must be a string, got {type(value).__name__}")

    value_lower = value.lower()  # Normalize to lowercase

    if not _HOSTNAME_REGEX.match(value_lower):
        raise GraphQLError(
            f"Invalid hostname: {value}. Must be a valid DNS hostname (RFC 1123 compliant)"
        )

    return value_lower


def parse_hostname_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse hostname from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Hostname must be a string")

    return parse_hostname_value(ast.value)


HostnameScalar = GraphQLScalarType(
    name="Hostname",
    description=(
        "A valid DNS hostname (RFC 1123 compliant). "
        "Must be 1-253 characters, contain only letters, numbers, hyphens and dots. "
        "Labels cannot start or end with hyphens."
    ),
    serialize=serialize_hostname,
    parse_value=parse_hostname_value,
    parse_literal=parse_hostname_literal,
)


class HostnameField(str, ScalarMarker):
    """DNS hostname following RFC 1123 specifications.

    This scalar validates that the hostname follows DNS naming conventions:
    - Total length: 1-253 characters
    - Each label: 1-63 characters
    - Valid characters: a-z, A-Z, 0-9, hyphen (-), dot (.)
    - Labels cannot start or end with hyphens
    - Case-insensitive (normalized to lowercase)

    Example:
        >>> from fraiseql.types import Hostname
        >>>
        >>> @fraiseql.input
        ... class ServerConfig:
        ...     hostname: Hostname
        ...     alias: Hostname | None
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "HostnameField":
        """Create a new HostnameField instance with validation."""
        value_lower = value.lower()
        if not _HOSTNAME_REGEX.match(value_lower):
            raise ValueError(
                f"Invalid hostname: {value}. Must be a valid DNS hostname (RFC 1123 compliant)"
            )
        return super().__new__(cls, value_lower)
