"""Custom GraphQL scalar types for FraiseQL.

This module exposes reusable scalar implementations that extend GraphQL's
capabilities to support domain-specific values such as IP addresses, UUIDs,
date ranges, JSON objects, and more.

Each export is a `GraphQLScalarType` used directly in schema definitions.

Exports:
- CIDRScalar: CIDR notation for IP network ranges.
- DateRangeScalar: PostgreSQL daterange values.
- DateScalar: ISO 8601 calendar date.
- DateTimeScalar: ISO 8601 datetime with timezone awareness.
- HostnameScalar: DNS hostnames (RFC 1123 compliant).
- IpAddressScalar: IPv4 and IPv6 addresses as strings.
- SubnetMaskScalar: CIDR-style subnet masks.
- JSONScalar: Arbitrary JSON-serializable values.
- LTreeScalar: PostgreSQL ltree path type.
- MacAddressScalar: Hardware MAC addresses.
- PortScalar: Network port number (1-65535).
- UUIDScalar: RFC 4122 UUID values.
"""

from .cidr import CIDRScalar
from .date import DateScalar
from .daterange import DateRangeScalar
from .datetime import DateTimeScalar
from .hostname import HostnameScalar
from .ip_address import IpAddressScalar, SubnetMaskScalar
from .json import JSONScalar
from .ltree import LTreeScalar
from .mac_address import MacAddressScalar
from .port import PortScalar
from .uuid import UUIDScalar

__all__ = [
    "CIDRScalar",
    "DateRangeScalar",
    "DateScalar",
    "DateTimeScalar",
    "HostnameScalar",
    "IpAddressScalar",
    "JSONScalar",
    "LTreeScalar",
    "MacAddressScalar",
    "PortScalar",
    "SubnetMaskScalar",
    "UUIDScalar",
]
