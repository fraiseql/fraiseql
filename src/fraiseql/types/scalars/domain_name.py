"""Domain name scalar type for RFC-compliant domain validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# RFC-compliant domain name regex (stricter than hostname, requires TLD with letters)
_DOMAIN_NAME_REGEX = re.compile(
    r"^(?=.{1,253}$)"
    r"(?!.*--)"
    r"(?!-)[a-zA-Z0-9-]{1,63}(?<!-)"
    r"(\."
    r"(?!-)[a-zA-Z0-9-]{1,63}(?<!-))*"
    r"\.[a-zA-Z]{2,}$"
)


def serialize_domain_name(value: Any) -> str | None:
    """Serialize domain name to string."""
    if value is None:
        return None

    value_str = str(value).lower()

    if not _DOMAIN_NAME_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid domain name: {value}. Must be RFC-compliant domain name "
            "(e.g., 'example.com', 'subdomain.example.co.uk')"
        )

    return value_str


def parse_domain_name_value(value: Any) -> str:
    """Parse domain name from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Domain name must be a string, got {type(value).__name__}")

    value_lower = value.lower()

    if not _DOMAIN_NAME_REGEX.match(value_lower):
        raise GraphQLError(
            f"Invalid domain name: {value}. Must be RFC-compliant domain name "
            "(e.g., 'example.com', 'subdomain.example.co.uk')"
        )

    return value_lower


def parse_domain_name_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse domain name from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Domain name must be a string")

    return parse_domain_name_value(ast.value)


DomainNameScalar = GraphQLScalarType(
    name="DomainName",
    description=(
        "RFC-compliant domain name with top-level domain. "
        "Valid examples: example.com, subdomain.example.co.uk, api.github.com. "
        "See: https://tools.ietf.org/html/rfc1035"
    ),
    serialize=serialize_domain_name,
    parse_value=parse_domain_name_value,
    parse_literal=parse_domain_name_literal,
)


class DomainNameField(str, ScalarMarker):
    """RFC-compliant domain name with top-level domain.

    This scalar validates that the domain name follows RFC standards:
    - Labels separated by dots
    - Each label: 1-63 chars, alphanumeric + hyphens, no leading/trailing hyphens
    - Must include top-level domain (TLD)
    - Case-insensitive (normalized to lowercase)
    - Maximum total length: 253 characters

    Example:
        >>> from fraiseql.types import DomainName
        >>>
        >>> @fraiseql.input
        ... class WebsiteInput:
        ...     domain: DomainName
        ...     ssl_enabled: bool
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "DomainNameField":
        """Create a new DomainNameField instance with validation."""
        value_lower = value.lower()
        if not _DOMAIN_NAME_REGEX.match(value_lower):
            raise ValueError(
                f"Invalid domain name: {value}. Must be RFC-compliant domain name "
                "(e.g., 'example.com', 'subdomain.example.co.uk')"
            )
        return super().__new__(cls, value_lower)
