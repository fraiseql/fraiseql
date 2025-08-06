"""CIDR scalar type for network CIDR notation validation."""

import ipaddress
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker


def serialize_cidr(value: Any) -> str | None:
    """Serialize CIDR notation to string."""
    if value is None:
        return None

    # If it's already a string, validate it
    if isinstance(value, str):
        try:
            # Validate CIDR notation
            ipaddress.ip_network(value, strict=False)
            return value
        except ValueError as e:
            raise GraphQLError(f"Invalid CIDR notation: {value}. {e!s}")

    # If it's an IPv4Network or IPv6Network object
    if isinstance(value, (ipaddress.IPv4Network, ipaddress.IPv6Network)):
        return str(value)

    raise GraphQLError(f"CIDR must be a string or IP network object, got {type(value).__name__}")


def parse_cidr_value(value: Any) -> str:
    """Parse CIDR from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"CIDR must be a string, got {type(value).__name__}")

    try:
        # Validate CIDR notation (strict=False allows host bits to be set)
        ipaddress.ip_network(value, strict=False)
        return value
    except ValueError as e:
        raise GraphQLError(f"Invalid CIDR notation: {value}. {e!s}")


def parse_cidr_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse CIDR from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("CIDR must be a string")

    return parse_cidr_value(ast.value)


CIDRScalar = GraphQLScalarType(
    name="CIDR",
    description=(
        "A CIDR (Classless Inter-Domain Routing) notation string "
        "(e.g., '192.168.1.0/24' or '2001:db8::/32')"
    ),
    serialize=serialize_cidr,
    parse_value=parse_cidr_value,
    parse_literal=parse_cidr_literal,
)


class CIDRField(str, ScalarMarker):
    """CIDR notation for IP network ranges.

    This scalar validates CIDR notation for both IPv4 and IPv6 networks.
    Examples include '192.168.1.0/24', '10.0.0.0/8', '2001:db8::/32'.

    The validation uses Python's ipaddress module with strict=False,
    which means host bits can be set (e.g., '192.168.1.1/24' is valid).

    Example:
        >>> from fraiseql.types import CIDR
        >>>
        >>> @fraiseql.input
        ... class NetworkConfig:
        ...     subnet: CIDR
        ...     gateway: CIDR | None
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "CIDRField":
        """Create a new CIDRField instance with validation."""
        try:
            ipaddress.ip_network(value, strict=False)
        except ValueError as e:
            raise ValueError(f"Invalid CIDR notation: {value}. {e!s}")
        return super().__new__(cls, value)
