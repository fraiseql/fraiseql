"""Rich type filters for specialized scalar types.

This module defines GraphQL filter input types for all 44 rich scalar types
including Email, VIN, IBAN, CountryCode, PhoneNumber, and many others.

Each filter class is organized by category and provides operators specific
to that type, enabling powerful queries like:
- email.domain_eq('example.com')
- vin.wmi_eq('1HG')
- country.continent_eq('Europe')
"""

from fraiseql import fraise_input
from fraiseql.fields import fraise_field


# ============================================================================
# CONTACT/COMMUNICATION FILTERS (5 types)
# ============================================================================

@fraise_input
class EmailAddressFilter:
    """Email address filtering with domain and local part operators."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    contains: str | None = None
    icontains: str | None = None
    isnull: bool | None = None

    # Rich operators
    domain_eq: str | None = None
    domain_in: list[str] | None = None
    domain_endswith: str | None = None
    local_startswith: str | None = None


@fraise_input
class PhoneNumberFilter:
    """International phone number filtering."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    country_code_eq: str | None = None
    country_code_in: list[str] | None = None
    is_valid: bool | None = None
    type_eq: str | None = None


@fraise_input
class URLFilter:
    """URL filtering with protocol, host, and path operators."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    contains: str | None = None
    isnull: bool | None = None

    # Rich operators
    protocol_eq: str | None = None
    host_eq: str | None = None
    path_startswith: str | None = None


@fraise_input
class DomainNameFilter:
    """Domain name filtering with TLD support."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    contains: str | None = None
    isnull: bool | None = None

    # Rich operators
    tld_eq: str | None = None
    tld_in: list[str] | None = None


@fraise_input
class HostnameFilter:
    """Hostname filtering with FQDN and depth operators."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    contains: str | None = None
    isnull: bool | None = None

    # Rich operators
    is_fqdn: bool | None = None
    depth_eq: int | None = None


# ============================================================================
# LOCATION/ADDRESS FILTERS (8 types)
# ============================================================================

@fraise_input
class PostalCodeFilter:
    """Postal code filtering with country support."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    country_eq: str | None = None


@fraise_input
class LatitudeFilter:
    """Latitude filtering with range support."""
    eq: float | None = None
    neq: float | None = None
    gt: float | None = None
    gte: float | None = None
    lt: float | None = None
    lte: float | None = None
    isnull: bool | None = None

    # Rich operators
    hemisphere_eq: str | None = None


@fraise_input
class LongitudeFilter:
    """Longitude filtering with range support."""
    eq: float | None = None
    neq: float | None = None
    gt: float | None = None
    gte: float | None = None
    lt: float | None = None
    lte: float | None = None
    isnull: bool | None = None

    # Rich operators
    hemisphere_eq: str | None = None


@fraise_input
class CoordinatesFilter:
    """Geographic coordinates filtering with distance operators."""
    eq: str | None = None
    neq: str | None = None
    isnull: bool | None = None

    # Rich operators (PostGIS)
    distance_within_lat_lng_radius: str | None = None
    within_bbox: str | None = None
    within_polygon: str | None = None


@fraise_input
class TimezoneFilter:
    """Timezone filtering with offset and DST support."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    offset_eq: int | None = None
    has_dst: bool | None = None
    region_eq: str | None = None


@fraise_input
class LocaleCodeFilter:
    """Locale code (BCP 47) filtering."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    language_eq: str | None = None
    country_eq: str | None = None
    script_eq: str | None = None


@fraise_input
class LanguageCodeFilter:
    """Language code (ISO 639-1) filtering."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    family_eq: str | None = None


@fraise_input
class CountryCodeFilter:
    """Country code (ISO 3166-1) filtering with geographic operators."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    continent_eq: str | None = None
    region_eq: str | None = None
    in_eu: bool | None = None
    in_schengen: bool | None = None


# ============================================================================
# FINANCIAL FILTERS (11 types)
# ============================================================================

@fraise_input
class IbanFilter:
    """IBAN filtering with country and validity operators."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    country_eq: str | None = None
    country_in: list[str] | None = None
    is_valid: bool | None = None


@fraise_input
class CusipFilter:
    """CUSIP filtering with issuer type operators."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    issuer_type_eq: str | None = None


@fraise_input
class IsinFilter:
    """ISIN filtering with country and asset class operators."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    country_eq: str | None = None
    asset_class_eq: str | None = None


@fraise_input
class SedolFilter:
    """SEDOL filtering with country operators."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    country_eq: str | None = None


@fraise_input
class LeiFilter:
    """LEI (Legal Entity Identifier) filtering."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    entity_category_eq: str | None = None


@fraise_input
class MicFilter:
    """Market Identifier Code filtering."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    country_eq: str | None = None
    segment_eq: str | None = None


@fraise_input
class CurrencyCodeFilter:
    """Currency code (ISO 4217) filtering."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    region_eq: str | None = None
    decimal_places_eq: int | None = None


@fraise_input
class MoneyFilter:
    """Money filtering with currency operators."""
    eq: float | None = None
    neq: float | None = None
    gt: float | None = None
    gte: float | None = None
    lt: float | None = None
    lte: float | None = None
    isnull: bool | None = None

    # Rich operators
    in_currency: str | None = None


@fraise_input
class ExchangeCodeFilter:
    """Exchange code filtering."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    country_eq: str | None = None


@fraise_input
class ExchangeRateFilter:
    """Exchange rate filtering."""
    eq: float | None = None
    neq: float | None = None
    gt: float | None = None
    gte: float | None = None
    lt: float | None = None
    lte: float | None = None
    isnull: bool | None = None

    # Rich operators
    currency_pair_eq: str | None = None


@fraise_input
class StockSymbolFilter:
    """Stock symbol filtering with exchange operators."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    exchange_eq: str | None = None
    sector_eq: str | None = None


# ============================================================================
# IDENTIFIER FILTERS (8 types)
# ============================================================================

@fraise_input
class SlugFilter:
    """URL slug filtering with path segment operators."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    contains: str | None = None
    isnull: bool | None = None

    # Rich operators
    depth_eq: int | None = None
    segment_eq: str | None = None


@fraise_input
class SemanticVersionFilter:
    """Semantic version filtering with version part operators."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    major_eq: int | None = None
    minor_eq: int | None = None
    patch_eq: int | None = None
    has_prerelease: bool | None = None


@fraise_input
class HashSha256Filter:
    """SHA256 hash filtering."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    length_eq: int | None = None


@fraise_input
class ApiKeyFilter:
    """API key filtering with length and prefix operators."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    length_eq: int | None = None
    prefix_eq: str | None = None


@fraise_input
class LicensePlateFilter:
    """License plate filtering with country operators."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    country_eq: str | None = None


@fraise_input
class VinFilter:
    """VIN filtering with manufacturer and year operators."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    wmi_eq: str | None = None
    wmi_in: list[str] | None = None
    country_eq: str | None = None
    model_year_eq: int | None = None
    is_valid: bool | None = None


@fraise_input
class TrackingNumberFilter:
    """Tracking number filtering with carrier operators."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    carrier_eq: str | None = None


@fraise_input
class ContainerNumberFilter:
    """ISO shipping container number filtering."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    owner_eq: str | None = None
    is_valid: bool | None = None


# ============================================================================
# NETWORKING FILTERS (6 types)
# ============================================================================

@fraise_input
class IpAddressFilter:
    """IP address (v4/v6) filtering."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    version_eq: int | None = None
    is_private: bool | None = None


@fraise_input
class Ipv4Filter:
    """IPv4 address filtering."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    cidr_contains: str | None = None
    is_multicast: bool | None = None
    is_reserved: bool | None = None


@fraise_input
class Ipv6Filter:
    """IPv6 address filtering."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    cidr_contains: str | None = None
    is_multicast: bool | None = None


@fraise_input
class CidrFilter:
    """CIDR notation filtering with overlap operators."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    overlaps: str | None = None
    contains_ip: str | None = None
    version_eq: int | None = None


@fraise_input
class PortFilter:
    """Network port filtering with service operators."""
    eq: int | None = None
    neq: int | None = None
    gt: int | None = None
    gte: int | None = None
    lt: int | None = None
    lte: int | None = None
    in_: list[int] | None = fraise_field(default=None, graphql_name="in")
    nin: list[int] | None = None
    isnull: bool | None = None

    # Rich operators
    service_eq: str | None = None
    is_well_known: bool | None = None
    is_registered: bool | None = None


# ============================================================================
# TRANSPORTATION FILTERS (3 types)
# ============================================================================

@fraise_input
class AirportCodeFilter:
    """Airport code filtering with country and major airport operators."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    country_eq: str | None = None
    is_major: bool | None = None


@fraise_input
class PortCodeFilter:
    """Port code (UN/LOCODE) filtering with country operators."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    country_eq: str | None = None


@fraise_input
class FlightNumberFilter:
    """Flight number filtering with airline operators."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    airline_eq: str | None = None
    aircraft_type_eq: str | None = None


# ============================================================================
# CONTENT FILTERS (6 types)
# ============================================================================

@fraise_input
class MarkdownFilter:
    """Markdown content filtering with validity operators."""
    eq: str | None = None
    neq: str | None = None
    contains: str | None = None
    isnull: bool | None = None

    # Rich operators
    is_valid: bool | None = None


@fraise_input
class HtmlFilter:
    """HTML content filtering with validity operators."""
    eq: str | None = None
    neq: str | None = None
    contains: str | None = None
    isnull: bool | None = None

    # Rich operators
    is_valid: bool | None = None
    contains_tag: str | None = None


@fraise_input
class MimeTypeFilter:
    """MIME type filtering with type/subtype operators."""
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Rich operators
    type_eq: str | None = None
    subtype_eq: str | None = None
    charset_eq: str | None = None


@fraise_input
class ColorFilter:
    """Color filtering with hex and RGB operators."""
    eq: str | None = None
    neq: str | None = None
    isnull: bool | None = None

    # Rich operators
    hex_eq: str | None = None


@fraise_input
class ImageFilter:
    """Image filtering with format and size operators."""
    eq: str | None = None
    neq: str | None = None
    isnull: bool | None = None

    # Rich operators
    format_eq: str | None = None
    width_gte: int | None = None
    height_gte: int | None = None
    size_lte: int | None = None


@fraise_input
class FileFilter:
    """File filtering with extension and MIME type operators."""
    eq: str | None = None
    neq: str | None = None
    isnull: bool | None = None

    # Rich operators
    extension_eq: str | None = None
    mime_type_eq: str | None = None
    size_lte: int | None = None


# ============================================================================
# RANGE FILTERS (3 types)
# ============================================================================

@fraise_input
class DateRangeTypeFilter:
    """Date range filtering with duration operators."""
    eq: str | None = None
    neq: str | None = None
    isnull: bool | None = None

    # Rich operators
    duration_gte: int | None = None
    starts_after: str | None = None
    ends_before: str | None = None


@fraise_input
class DurationFilter:
    """Duration filtering with seconds/minutes operators."""
    eq: str | None = None
    neq: str | None = None
    isnull: bool | None = None

    # Rich operators
    total_seconds_eq: int | None = None
    total_minutes_gte: int | None = None


@fraise_input
class PercentageFilter:
    """Percentage filtering with range operators."""
    eq: float | None = None
    neq: float | None = None
    gt: float | None = None
    gte: float | None = None
    lt: float | None = None
    lte: float | None = None
    isnull: bool | None = None

    # Rich operators
    percentile_eq: float | None = None


__all__ = [
    # Contact/Communication
    "EmailAddressFilter",
    "PhoneNumberFilter",
    "URLFilter",
    "DomainNameFilter",
    "HostnameFilter",
    # Location/Address
    "PostalCodeFilter",
    "LatitudeFilter",
    "LongitudeFilter",
    "CoordinatesFilter",
    "TimezoneFilter",
    "LocaleCodeFilter",
    "LanguageCodeFilter",
    "CountryCodeFilter",
    # Financial
    "IbanFilter",
    "CusipFilter",
    "IsinFilter",
    "SedolFilter",
    "LeiFilter",
    "MicFilter",
    "CurrencyCodeFilter",
    "MoneyFilter",
    "ExchangeCodeFilter",
    "ExchangeRateFilter",
    "StockSymbolFilter",
    # Identifiers
    "SlugFilter",
    "SemanticVersionFilter",
    "HashSha256Filter",
    "ApiKeyFilter",
    "LicensePlateFilter",
    "VinFilter",
    "TrackingNumberFilter",
    "ContainerNumberFilter",
    # Networking
    "IpAddressFilter",
    "Ipv4Filter",
    "Ipv6Filter",
    "CidrFilter",
    "PortFilter",
    # Transportation
    "AirportCodeFilter",
    "PortCodeFilter",
    "FlightNumberFilter",
    # Content
    "MarkdownFilter",
    "HtmlFilter",
    "MimeTypeFilter",
    "ColorFilter",
    "ImageFilter",
    "FileFilter",
    # Ranges
    "DateRangeTypeFilter",
    "DurationFilter",
    "PercentageFilter",
]
