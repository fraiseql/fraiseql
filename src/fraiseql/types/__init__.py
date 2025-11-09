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
from .scalars.airport_code import AirportCodeField as AirportCode
from .scalars.api_key import ApiKeyField as ApiKey
from .scalars.cidr import CIDRField as CIDR  # noqa: N814
from .scalars.color import ColorField as Color
from .scalars.container_number import ContainerNumberField as ContainerNumber
from .scalars.coordinates import CoordinateField as Coordinate
from .scalars.currency_code import CurrencyCodeField as CurrencyCode
from .scalars.cusip import CUSIPField as CUSIP  # noqa: N814
from .scalars.date import DateField as Date
from .scalars.daterange import DateRangeField as DateRange
from .scalars.datetime import DateTimeField as DateTime
from .scalars.domain_name import DomainNameField as DomainName
from .scalars.duration import DurationField as Duration
from .scalars.email_address import EmailAddressField as EmailAddress
from .scalars.exchange_code import ExchangeCodeField as ExchangeCode
from .scalars.exchange_rate import ExchangeRateField as ExchangeRate
from .scalars.file import FileField as File
from .scalars.flight_number import FlightNumberField as FlightNumber
from .scalars.graphql_utils import convert_scalar_to_graphql
from .scalars.hash_sha256 import HashSHA256Field as HashSHA256
from .scalars.hostname import HostnameField as Hostname
from .scalars.html import HTMLField as HTML  # noqa: N814
from .scalars.iban import IBANField as IBAN  # noqa: N814
from .scalars.image import ImageField as Image
from .scalars.ip_address import IpAddressField as IpAddress
from .scalars.isin import ISINField as ISIN  # noqa: N814
from .scalars.json import JSONField as JSON  # noqa: N814
from .scalars.language_code import LanguageCodeField as LanguageCode
from .scalars.latitude import LatitudeField as Latitude
from .scalars.lei import LEIField as LEI  # noqa: N814
from .scalars.license_plate import LicensePlateField as LicensePlate
from .scalars.locale_code import LocaleCodeField as LocaleCode
from .scalars.longitude import LongitudeField as Longitude
from .scalars.ltree import LTreeField as LTree
from .scalars.mac_address import MacAddressField as MacAddress
from .scalars.markdown import MarkdownField as Markdown
from .scalars.mic import MICField as MIC  # noqa: N814
from .scalars.mime_type import MimeTypeField as MimeType
from .scalars.money import MoneyField as Money
from .scalars.percentage import PercentageField as Percentage
from .scalars.phone_number import PhoneNumberField as PhoneNumber
from .scalars.port import PortField as Port
from .scalars.port_code import PortCodeField as PortCode
from .scalars.postal_code import PostalCodeField as PostalCode
from .scalars.sedol import SEDOLField as SEDOL  # noqa: N814
from .scalars.semantic_version import SemanticVersionField as SemanticVersion
from .scalars.slug import SlugField as Slug
from .scalars.stock_symbol import StockSymbolField as StockSymbol
from .scalars.time import TimeField as Time
from .scalars.timezone import TimezoneField as Timezone
from .scalars.tracking_number import TrackingNumberField as TrackingNumber
from .scalars.url import URLField as URL  # noqa: N814
from .scalars.uuid import UUIDField as UUID  # noqa: N814
from .scalars.vin import VINField as VIN  # noqa: N814

# Aliases for decorators
type = fraise_type  # noqa: A001
input = fraise_input  # noqa: A001

__all__ = [
    "CIDR",
    "CUSIP",
    "HTML",
    "IBAN",
    "ISIN",
    "JSON",
    "LEI",
    "MIC",
    "SEDOL",
    "URL",
    "UUID",
    "VIN",
    "AirportCode",
    "ApiKey",
    "Color",
    "Connection",
    "ContainerNumber",
    "Coordinate",
    "CurrencyCode",
    "Date",
    "DateRange",
    "DateRangeValidatable",
    "DateRangeValidationMixin",
    "DateTime",
    "DomainName",
    "Duration",
    "Edge",
    "EmailAddress",
    "Error",
    "ExchangeCode",
    "ExchangeRate",
    "File",
    "FlightNumber",
    "HashSHA256",
    "Hostname",
    "Image",
    "IpAddress",
    "LTree",
    "LanguageCode",
    "Latitude",
    "LicensePlate",
    "LocaleCode",
    "Longitude",
    "MacAddress",
    "Markdown",
    "MimeType",
    "Money",
    "PageInfo",
    "PaginatedResponse",
    "Percentage",
    "PhoneNumber",
    "Port",
    "PortCode",
    "PostalCode",
    "SemanticVersion",
    "Slug",
    "StockSymbol",
    "Time",
    "Timezone",
    "TrackingNumber",
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
