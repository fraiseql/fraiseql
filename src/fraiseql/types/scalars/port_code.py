"""Port code scalar type for UN/LOCODE validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# UN/LOCODE regex: 2 country code letters + 3 location code letters/digits
_PORT_CODE_REGEX = re.compile(r"^[A-Z]{2}[A-Z0-9]{3}$")


def serialize_port_code(value: Any) -> str | None:
    """Serialize port code to string."""
    if value is None:
        return None

    value_str = str(value).upper()

    if not _PORT_CODE_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid port code: {value}. Must be UN/LOCODE format "
            "(2 country letters + 3 location characters, e.g., 'USNYC', 'CNSHA')"
        )

    return value_str


def parse_port_code_value(value: Any) -> str:
    """Parse port code from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Port code must be a string, got {type(value).__name__}")

    value_upper = value.upper()

    if not _PORT_CODE_REGEX.match(value_upper):
        raise GraphQLError(
            f"Invalid port code: {value}. Must be UN/LOCODE format "
            "(2 country letters + 3 location characters, e.g., 'USNYC', 'CNSHA')"
        )

    return value_upper


def parse_port_code_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse port code from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Port code must be a string")

    return parse_port_code_value(ast.value)


PortCodeScalar = GraphQLScalarType(
    name="PortCode",
    description=(
        "UN/LOCODE port code. Format: 2 country letters + 3 location characters. "
        "Examples: USNYC (New York), CNSHA (Shanghai), NLRTM (Rotterdam). "
        "See: https://www.unece.org/cefact/locode/welcome.html"
    ),
    serialize=serialize_port_code,
    parse_value=parse_port_code_value,
    parse_literal=parse_port_code_literal,
)


class PortCodeField(str, ScalarMarker):
    """UN/LOCODE port code.

    This scalar validates that the port code follows UN/LOCODE standard:
    - 5 characters total
    - First 2: Country code (letters only)
    - Last 3: Location code (letters and digits)
    - Case-insensitive (normalized to uppercase)

    Example:
        >>> from fraiseql.types import PortCode
        >>>
        >>> @fraiseql.input
        ... class ShippingInput:
        ...     origin_port: PortCode
        ...     destination_port: PortCode
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "PortCodeField":
        """Create a new PortCodeField instance with validation."""
        value_upper = value.upper()
        if not _PORT_CODE_REGEX.match(value_upper):
            raise ValueError(
                f"Invalid port code: {value}. Must be UN/LOCODE format "
                "(2 country letters + 3 location characters, e.g., 'USNYC', 'CNSHA')"
            )
        return super().__new__(cls, value_upper)
