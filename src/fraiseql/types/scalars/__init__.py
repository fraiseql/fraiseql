"""Custom GraphQL scalar types for FraiseQL.

This module exposes reusable scalar implementations that extend GraphQL's
capabilities to support domain-specific values such as IP addresses, UUIDs,
date ranges, JSON objects, and more.

Each export is a `GraphQLScalarType` used directly in schema definitions.

Exports:
- DateRangeScalar: PostgreSQL daterange values.
- DateScalar: ISO 8601 calendar date.
- DateTimeScalar: ISO 8601 datetime with timezone awareness.
- IpAddressScalar: IPv4 and IPv6 addresses as strings.
- SubnetMaskScalar: CIDR-style subnet masks.
- JSONScalar: Arbitrary JSON-serializable values.
- LTreeScalar: PostgreSQL ltree path type.
- UUIDScalar: RFC 4122 UUID values.
"""

from .date import DateScalar
from .daterange import DateRangeScalar
from .datetime import DateTimeScalar
from .ip_address import IpAddressScalar, SubnetMaskScalar
from .json import JSONScalar
from .ltree import LTreeScalar
from .uuid import UUIDScalar

__all__ = [
    "DateRangeScalar",
    "DateScalar",
    "DateTimeScalar",
    "IpAddressScalar",
    "JSONScalar",
    "LTreeScalar",
    "SubnetMaskScalar",
    "UUIDScalar",
]
