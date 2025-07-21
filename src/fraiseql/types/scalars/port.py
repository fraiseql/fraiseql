"""Port scalar type for network port validation (1-65535)."""

from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import IntValueNode

from fraiseql.types.definitions import ScalarMarker


def serialize_port(value: Any) -> int | None:
    """Serialize port number to int."""
    if value is None:
        return None

    try:
        port = int(value)
    except (TypeError, ValueError):
        raise GraphQLError(f"Port must be an integer, got {type(value).__name__}")

    if not (1 <= port <= 65535):
        raise GraphQLError(f"Port must be between 1 and 65535, got {port}")

    return port


def parse_port_value(value: Any) -> int:
    """Parse port from variable value."""
    if not isinstance(value, int):
        raise GraphQLError(f"Port must be an integer, got {type(value).__name__}")

    if not (1 <= value <= 65535):
        raise GraphQLError(f"Port must be between 1 and 65535, got {value}")

    return value


def parse_port_literal(ast: Any, _variables: dict[str, Any] | None = None) -> int:
    """Parse port from AST literal."""
    if not isinstance(ast, IntValueNode):
        raise GraphQLError("Port must be an integer")

    value = int(ast.value)
    if not (1 <= value <= 65535):
        raise GraphQLError(f"Port must be between 1 and 65535, got {value}")

    return value


PortScalar = GraphQLScalarType(
    name="Port",
    description="A valid network port number (1-65535)",
    serialize=serialize_port,
    parse_value=parse_port_value,
    parse_literal=parse_port_literal,
)


class PortField(int, ScalarMarker):
    """Network port number between 1 and 65535.

    This scalar validates that the port number is within the valid range
    for TCP/UDP ports (1-65535). Port 0 is reserved and not allowed.

    Example:
        >>> from fraiseql.types import Port
        >>>
        >>> @fraiseql.input
        ... class ServerConfig:
        ...     hostname: str
        ...     port: Port
    """

    def __new__(cls, value: int) -> "PortField":
        """Create a new PortField instance with validation."""
        if not (1 <= value <= 65535):
            raise ValueError(f"Port must be between 1 and 65535, got {value}")
        return super().__new__(cls, value)
