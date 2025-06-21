"""GraphQL scalar mapping utilities for FraiseQL.

This module defines a mapping between common Python types (e.g., `uuid.UUID`,
`datetime.datetime`) and their corresponding GraphQL scalar types. It supports both
standard GraphQL scalars (e.g., `GraphQLString`, `GraphQLInt`, `GraphQLID`) and custom
FraiseQL scalars with serialization logic.

The core function `convert_scalar_to_graphql()` is used to translate a Python type
annotation into its GraphQL scalar equivalent during schema generation.

Raises:
    TypeError: If the provided Python type has no corresponding GraphQL scalar.
"""

import datetime
import ipaddress
import uuid

from graphql import (
    GraphQLBoolean,
    GraphQLFloat,
    GraphQLID,
    GraphQLInt,
    GraphQLScalarType,
    GraphQLString,
)

from .date import DateScalar
from .datetime import DateTimeScalar
from .ip_address import IpAddressScalar, SubnetMaskScalar
from .json import JSONField, JSONScalar
from .uuid import UUIDField


def convert_scalar_to_graphql(typ: type) -> GraphQLScalarType:
    """Convert a Python type to a corresponding GraphQL scalar type."""
    scalar_map: dict[type, GraphQLScalarType] = {
        str: GraphQLString,
        int: GraphQLInt,
        float: GraphQLFloat,
        bool: GraphQLBoolean,
        JSONField: JSONScalar,
        dict: JSONScalar,
        uuid.UUID: GraphQLID,
        UUIDField: GraphQLID,
        datetime.date: DateScalar,
        datetime.datetime: DateTimeScalar,
        datetime.time: GraphQLString,
        ipaddress.IPv4Address: IpAddressScalar,
        ipaddress.IPv4Network: SubnetMaskScalar,
        # Note: tuple and list are too generic to map to specific scalars
        # DateRangeScalar and LTreeScalar should be used via specific marker types
    }

    if typ in scalar_map:
        return scalar_map[typ]

    msg = f"Unsupported scalar type: {typ}"
    raise TypeError(msg)
