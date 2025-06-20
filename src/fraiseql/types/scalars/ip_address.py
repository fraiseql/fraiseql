"""Custom GraphQL scalar types for handling IP addresses and subnet masks in FraiseQL.

This module defines custom GraphQL scalar types for IP addresses and subnet masks.
It includes functions to serialize, parse, and validate IP address strings and subnet mask strings.
The module uses Python's `ipaddress` library to handle IPv4 and IPv6 addresses and subnet masks.

Functions:
- serialize_ip_address_string: Serializes an IP address to a string.
- parse_ip_address_value: Parses a string into an IP address object.
- parse_ip_address_literal: Parses a GraphQL AST literal into an IP address object.
- serialize_subnet_mask_string: Serializes a subnet mask to a string.
- parse_subnet_mask_value: Parses a subnet mask string into an IPv4Address object.
- parse_subnet_mask_literal: Parses a GraphQL AST literal into a subnet mask object.

GraphQL Scalar Types:
- IpAddressStringType: A GraphQL scalar type for IPv4 and IPv6 addresses.
- SubnetMaskStringType: A GraphQL scalar type for subnet mask strings.
"""

from ipaddress import IPv4Address, IPv4Network, IPv6Address, ip_address
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode, ValueNode

from fraiseql.types.definitions import ScalarMarker

IPV4_BIT_LENGTH = 32
VALID_MASK_BITS = 32
INVALID_NETMASK_VALUES = {0, (1 << VALID_MASK_BITS) - 1}


def serialize_ip_address_string(value: Any) -> str:
    """Serialize an IP address to string."""
    if isinstance(value, IPv4Address | IPv6Address):
        return str(value)

    if isinstance(value, str):
        try:
            ip_address(value)
        except ValueError:
            pass
        else:
            return value

    msg = f"IpAddressString cannot represent non-IP address value: {value!r}"
    raise GraphQLError(msg)


def parse_ip_address_value(value: Any) -> IPv4Address | IPv6Address:
    """Parse a string into an IP address object."""
    if isinstance(value, str):
        try:
            return ip_address(value)
        except ValueError as e:
            msg = f"Invalid IP address string: {value!r}"
            raise GraphQLError(msg) from e
    msg = f"IpAddressString cannot represent non-string value: {value!r}"
    raise GraphQLError(msg)


def parse_ip_address_literal(
    ast: ValueNode,
    variables: dict[str, Any] | None = None,
) -> IPv4Address | IPv6Address:
    """Parse a literal into an IP address."""
    _ = variables
    if isinstance(ast, StringValueNode):
        return parse_ip_address_value(ast.value)
    msg = f"IpAddressString cannot represent non-string literal: {getattr(ast, 'value', None)!r}"
    raise GraphQLError(msg)


IpAddressScalar = GraphQLScalarType(
    name="IpAddressString",
    description="Scalar for IPv4 and IPv6 addresses.",
    serialize=serialize_ip_address_string,
    parse_value=parse_ip_address_value,
    parse_literal=parse_ip_address_literal,
)


class IpAddressField(str, ScalarMarker):
    """Represents a validated IP address."""

    __slots__ = ()

    def __repr__(self) -> str:
        """Missing docstring."""
        return "IpAddress"


def serialize_subnet_mask_string(value: Any) -> str:
    """Serialize a subnet mask to string."""
    if isinstance(value, IPv4Address):
        _check_netmask(value)
        return str(value)

    if isinstance(value, str):
        try:
            ip_obj = ip_address(value)
            if isinstance(ip_obj, IPv4Address):
                _check_netmask(ip_obj)
                return str(ip_obj)
        except ValueError:
            pass

    msg = f"SubnetMaskString cannot represent non-IPv4Address value: {value!r}"
    raise GraphQLError(msg)


def _check_netmask(ip_obj: IPv4Address) -> str:
    """Check if the given IPv4Address is a valid subnet mask."""
    try:
        # Try constructing a dummy network using the mask
        IPv4Network(f"0.0.0.0/{ip_obj}", strict=False)
        return str(ip_obj)
    except ValueError as e:
        msg = "Invalid subnet mask"
        raise ValueError(msg) from e


def parse_subnet_mask_value(value: Any) -> IPv4Address | None:
    """Parse a subnet mask string into an IPv4Address."""
    if value is None:
        return None
    if isinstance(value, str):
        try:
            ip_obj = ip_address(value)
            if isinstance(ip_obj, IPv4Address):
                _check_netmask(ip_obj)
                return ip_obj
        except ValueError as e:
            msg = f"Invalid subnet mask string: {value!r}"
            raise GraphQLError(msg) from e
    msg = f"SubnetMaskString cannot represent non-string value: {value!r}"
    raise GraphQLError(msg)


def parse_subnet_mask_literal(
    ast: ValueNode,
    variables: dict[str, Any] | None = None,
) -> IPv4Address | None:
    """Parse a literal subnet mask string."""
    _ = variables
    if isinstance(ast, StringValueNode):
        return parse_subnet_mask_value(ast.value)
    msg = f"SubnetMaskString cannot represent non-string literal: {getattr(ast, 'value', None)!r}"
    raise GraphQLError(msg)


SubnetMaskScalar = GraphQLScalarType(
    name="SubnetMaskString",
    description="Scalar for subnet mask strings.",
    serialize=serialize_subnet_mask_string,
    parse_value=parse_subnet_mask_value,
    parse_literal=parse_subnet_mask_literal,
)


class SubnetMaskField(str, ScalarMarker):
    """Python marker for the GraphQL SubnetMask scalar."""

    __slots__ = ()

    def __repr__(self) -> str:
        """Missing docstring."""
        return "SubnetMask"
