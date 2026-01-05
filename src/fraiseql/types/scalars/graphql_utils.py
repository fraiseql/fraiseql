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

from fraiseql.config.schema_config import SchemaConfig

from .cidr import CIDRField, CIDRScalar
from .coordinates import CoordinateField, CoordinateScalar
from .date import DateScalar
from .daterange import DateRangeField, DateRangeScalar
from .datetime import DateTimeScalar
from .email_address import EmailAddressField, EmailAddressScalar
from .hostname import HostnameField, HostnameScalar
from .id_scalar import ID, IDScalar
from .ip_address import IpAddressField, IpAddressScalar, SubnetMaskScalar
from .json import JSONField, JSONScalar
from .ltree import LTreeField, LTreeScalar
from .mac_address import MacAddressField, MacAddressScalar
from .port import PortField, PortScalar
from .uuid import UUIDField, UUIDScalar


def convert_scalar_to_graphql(typ: type) -> GraphQLScalarType:
    """Convert a Python type to a corresponding GraphQL scalar type.

    The ID scalar behavior is controlled by SchemaConfig.id_policy:
    - IDPolicy.UUID (default): ID type uses IDScalar (enforces UUID format)
    - IDPolicy.OPAQUE: ID type uses GraphQL's built-in ID (accepts any string)

    Note: uuid.UUID always maps to UUIDScalar (name="UUID"), regardless of policy.
    Only the ID type annotation is affected by the policy.
    """
    config = SchemaConfig.get_instance()

    # Determine which scalar to use for ID based on policy
    id_scalar = IDScalar if config.id_policy.enforces_uuid() else GraphQLID

    scalar_map: dict[type, GraphQLScalarType] = {
        str: GraphQLString,
        int: GraphQLInt,
        float: GraphQLFloat,
        bool: GraphQLBoolean,
        JSONField: JSONScalar,
        dict: JSONScalar,
        # uuid.UUID always maps to UUIDScalar (semantic correctness)
        # UUID is a specific format, not necessarily an identifier
        uuid.UUID: UUIDScalar,  # uuid.UUID → "UUID" scalar
        UUIDField: UUIDScalar,  # Explicit UUID field → "UUID" scalar
        # ID type annotation uses policy-dependent scalar
        ID: id_scalar,  # ID → "ID" scalar (policy determines enforcement)
        datetime.date: DateScalar,
        datetime.datetime: DateTimeScalar,
        datetime.time: GraphQLString,
        ipaddress.IPv4Address: IpAddressScalar,
        ipaddress.IPv4Network: SubnetMaskScalar,
        IpAddressField: IpAddressScalar,
        CoordinateField: CoordinateScalar,
        EmailAddressField: EmailAddressScalar,
        CIDRField: CIDRScalar,
        DateRangeField: DateRangeScalar,
        HostnameField: HostnameScalar,
        LTreeField: LTreeScalar,
        MacAddressField: MacAddressScalar,
        PortField: PortScalar,
        # Note: tuple and list are too generic to map to specific scalars
    }

    if typ in scalar_map:
        return scalar_map[typ]

    msg = f"Unsupported scalar type: {typ}"
    raise TypeError(msg)
