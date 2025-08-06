"""FraiseQL Types Package.

Provides decorators and common GraphQL types for FraiseQL.

Exports:
- `type`: Decorator to mark a dataclass as a GraphQL object type.
- `input`: Decorator to mark a dataclass as a GraphQL input type.
- `fraise_type` and `fraise_input`: Internal decorator implementations to avoid
  shadowing Python builtins.

Usage:

    from fraiseql.types import type, input

    @type
    class User:
        id: int
        name: str

    @input
    class CreateUserInput:
        name: str
"""

from .date_range_validation import (
    DateRangeValidatable,
    DateRangeValidationMixin,
    date_range_validator,
    get_date_range_validation_errors,
    validate_date_range,
)
from .errors import Error
from .fraise_input import fraise_input
from .fraise_type import fraise_type
from .generic import Connection, Edge, PageInfo, PaginatedResponse, create_connection
from .scalars.cidr import CIDRField as CIDR  # noqa: N814
from .scalars.date import DateField as Date
from .scalars.daterange import DateRangeField as DateRange
from .scalars.datetime import DateTimeField as DateTime
from .scalars.email_address import EmailAddressField as EmailAddress
from .scalars.graphql_utils import convert_scalar_to_graphql
from .scalars.hostname import HostnameField as Hostname
from .scalars.ip_address import IpAddressField as IpAddress
from .scalars.json import JSONField as JSON  # noqa: N814
from .scalars.ltree import LTreeField as LTree
from .scalars.mac_address import MacAddressField as MacAddress
from .scalars.port import PortField as Port
from .scalars.uuid import UUIDField as UUID  # noqa: N814

# Aliases for decorators
type = fraise_type  # noqa: A001
input = fraise_input  # noqa: A001

__all__ = [
    "CIDR",
    "JSON",
    "UUID",
    "Connection",
    "Date",
    "DateRange",
    "DateRangeValidatable",
    "DateRangeValidationMixin",
    "DateTime",
    "Edge",
    "EmailAddress",
    "Error",
    "Hostname",
    "IpAddress",
    "LTree",
    "MacAddress",
    "PageInfo",
    "PaginatedResponse",
    "Port",
    "convert_scalar_to_graphql",
    "create_connection",
    "date_range_validator",
    "fraise_input",
    "fraise_type",
    "get_date_range_validation_errors",
    "input",
    "type",
    "validate_date_range",
]
