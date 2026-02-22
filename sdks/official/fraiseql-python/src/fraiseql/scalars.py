"""FraiseQL scalar type markers for schema authoring.

These are type markers used in Python type annotations to generate the correct
GraphQL scalar types in schema.json. They have no runtime behavior - validation
and serialization happen in the Rust runtime after compilation.

Architecture:
    Python type annotation → schema.json type string → Rust FieldType → codegen/introspection

Example:
    ```python
    import fraiseql
    from fraiseql.scalars import ID, DateTime, Email, URL

    @fraiseql.type
    class User:
        id: ID                    # → "ID" in schema.json → FieldType::Id
        name: str                 # → "String"
        email: Email              # → "Email" → FieldType::Scalar("Email")
        website: URL | None       # → "URL" (nullable)
        created_at: DateTime      # → "DateTime" → FieldType::DateTime
    ```

FraiseQL Convention:
    - `id` fields should ALWAYS use `ID` type (UUID v4 at runtime)
    - Foreign keys (e.g., `author_id`) should also use `ID`

Custom Scalars:
    You can define your own custom scalars using NewType:

    ```python
    from typing import NewType
    MyCustomScalar = NewType("MyCustomScalar", str)
    ```

    The scalar name will pass through to schema.json and be validated at runtime.
"""

from typing import Any, NewType


# =============================================================================
# CustomScalar Base Class
# =============================================================================


class CustomScalar:
    """Base class for custom GraphQL scalars with validation.

    Subclasses must define a `name` class attribute and implement
    the three validation methods (serialize, parse_value, parse_literal).

    Use with the @scalar decorator to register custom scalars with the schema.

    Example:
        ```python
        from fraiseql import CustomScalar, scalar

        @scalar
        class Email(CustomScalar):
            name = "Email"

            def serialize(self, value: str) -> str:
                return str(value)

            def parse_value(self, value: str) -> str:
                if "@" not in str(value):
                    raise ValueError("Invalid email address")
                return str(value)

            def parse_literal(self, ast: Any) -> str:
                if hasattr(ast, 'value'):
                    return self.parse_value(ast.value)
                raise ValueError("Invalid email literal")
        ```
    """

    name: str
    """Scalar name (e.g., "Email"). Must be unique in schema."""

    def serialize(self, value: Any) -> Any:
        """Convert value to output format (schema → response).

        Called when serializing a field value in GraphQL response.

        Args:
            value: The internal representation (from database/object)

        Returns:
            The value formatted for GraphQL response

        Raises:
            ValueError: If value cannot be serialized
        """
        raise NotImplementedError(f"{self.__class__.__name__}.serialize() not implemented")

    def parse_value(self, value: Any) -> Any:
        """Validate and convert input value (client input → internal).

        Called when a scalar is passed as a variable in GraphQL query.

        Args:
            value: Raw input value from client

        Returns:
            Validated/converted value

        Raises:
            ValueError: If validation fails
        """
        raise NotImplementedError(f"{self.__class__.__name__}.parse_value() not implemented")

    def parse_literal(self, ast: Any) -> Any:
        """Parse GraphQL literal (hardcoded value in query).

        Called when a scalar is hardcoded in the GraphQL query string
        (not as a variable).

        Args:
            ast: GraphQL AST node representing the literal
                (has `value` attribute for string/number/etc)

        Returns:
            Validated/converted value

        Raises:
            ValueError: If literal cannot be parsed
        """
        raise NotImplementedError(f"{self.__class__.__name__}.parse_literal() not implemented")


# =============================================================================
# Core GraphQL Scalars
# =============================================================================

ID = NewType("ID", str)
"""GraphQL ID scalar - used for unique identifiers.

FraiseQL enforces UUID v4 format for all ID fields at runtime.
This is the REQUIRED type for `id` fields and foreign key references.
"""

# =============================================================================
# Date/Time Scalars
# =============================================================================

DateTime = NewType("DateTime", str)
"""ISO 8601 DateTime scalar (e.g., "2025-01-10T12:00:00Z")."""

Date = NewType("Date", str)
"""ISO 8601 Date scalar (e.g., "2025-01-10")."""

Time = NewType("Time", str)
"""ISO 8601 Time scalar (e.g., "12:00:00")."""

DateRange = NewType("DateRange", str)
"""Date range scalar (e.g., "[2025-01-01,2025-12-31)")."""

Duration = NewType("Duration", str)
"""ISO 8601 Duration scalar (e.g., "P1Y2M3D")."""

# =============================================================================
# Complex Core Scalars
# =============================================================================

Json = NewType("Json", object)
"""Arbitrary JSON value scalar. Maps to PostgreSQL JSONB."""

UUID = NewType("UUID", str)
"""UUID scalar (explicit UUID type, distinct from ID)."""

Decimal = NewType("Decimal", str)
"""Decimal/BigDecimal scalar for precise numeric values."""

Vector = NewType("Vector", list)
"""Vector scalar for pgvector embeddings."""

# =============================================================================
# Contact/Communication Scalars
# =============================================================================

Email = NewType("Email", str)
"""Email address scalar with RFC 5322 validation."""

PhoneNumber = NewType("PhoneNumber", str)
"""Phone number scalar (E.164 format recommended)."""

URL = NewType("URL", str)
"""URL scalar with RFC 3986 validation."""

DomainName = NewType("DomainName", str)
"""Domain name scalar."""

Hostname = NewType("Hostname", str)
"""Hostname scalar."""

# =============================================================================
# Location/Address Scalars
# =============================================================================

PostalCode = NewType("PostalCode", str)
"""Postal/ZIP code scalar."""

Latitude = NewType("Latitude", float)
"""Latitude coordinate (-90 to 90)."""

Longitude = NewType("Longitude", float)
"""Longitude coordinate (-180 to 180)."""

Coordinates = NewType("Coordinates", str)
"""Geographic coordinates scalar (lat,lng or GeoJSON)."""

Timezone = NewType("Timezone", str)
"""IANA timezone identifier (e.g., "America/New_York")."""

LocaleCode = NewType("LocaleCode", str)
"""Locale code scalar (e.g., "en-US")."""

LanguageCode = NewType("LanguageCode", str)
"""ISO 639-1 language code (e.g., "en")."""

CountryCode = NewType("CountryCode", str)
"""ISO 3166-1 alpha-2 country code (e.g., "US")."""

# =============================================================================
# Financial Scalars
# =============================================================================

IBAN = NewType("IBAN", str)
"""International Bank Account Number."""

CUSIP = NewType("CUSIP", str)
"""CUSIP identifier for North American securities."""

ISIN = NewType("ISIN", str)
"""International Securities Identification Number."""

SEDOL = NewType("SEDOL", str)
"""Stock Exchange Daily Official List number."""

LEI = NewType("LEI", str)
"""Legal Entity Identifier."""

MIC = NewType("MIC", str)
"""Market Identifier Code."""

CurrencyCode = NewType("CurrencyCode", str)
"""ISO 4217 currency code (e.g., "USD")."""

Money = NewType("Money", str)
"""Monetary amount with currency (e.g., "USD 100.00")."""

ExchangeCode = NewType("ExchangeCode", str)
"""Stock exchange code."""

ExchangeRate = NewType("ExchangeRate", str)
"""Currency exchange rate."""

StockSymbol = NewType("StockSymbol", str)
"""Stock ticker symbol."""

Percentage = NewType("Percentage", float)
"""Percentage value (0-100 or 0-1 depending on context)."""

# =============================================================================
# Identifier Scalars
# =============================================================================

Slug = NewType("Slug", str)
"""URL-safe slug (lowercase, hyphens, no spaces)."""

SemanticVersion = NewType("SemanticVersion", str)
"""Semantic version string (e.g., "1.2.3")."""

HashSHA256 = NewType("HashSHA256", str)
"""SHA-256 hash string (64 hex characters)."""

APIKey = NewType("APIKey", str)
"""API key string."""

LicensePlate = NewType("LicensePlate", str)
"""Vehicle license plate number."""

VIN = NewType("VIN", str)
"""Vehicle Identification Number."""

TrackingNumber = NewType("TrackingNumber", str)
"""Shipping tracking number."""

ContainerNumber = NewType("ContainerNumber", str)
"""Shipping container number (ISO 6346)."""

# =============================================================================
# Networking Scalars
# =============================================================================

IPAddress = NewType("IPAddress", str)
"""IP address (IPv4 or IPv6)."""

IPv4 = NewType("IPv4", str)
"""IPv4 address."""

IPv6 = NewType("IPv6", str)
"""IPv6 address."""

MACAddress = NewType("MACAddress", str)
"""MAC address."""

CIDR = NewType("CIDR", str)
"""CIDR notation for IP ranges."""

Port = NewType("Port", int)
"""Network port number (0-65535)."""

# =============================================================================
# Transportation Scalars
# =============================================================================

AirportCode = NewType("AirportCode", str)
"""IATA airport code (e.g., "JFK")."""

PortCode = NewType("PortCode", str)
"""UN/LOCODE port code."""

FlightNumber = NewType("FlightNumber", str)
"""Flight number (e.g., "AA123")."""

# =============================================================================
# Content Scalars
# =============================================================================

Markdown = NewType("Markdown", str)
"""Markdown-formatted text."""

HTML = NewType("HTML", str)
"""HTML-formatted text."""

MimeType = NewType("MimeType", str)
"""MIME type (e.g., "application/json")."""

Color = NewType("Color", str)
"""Color value (hex, RGB, or named)."""

Image = NewType("Image", str)
"""Image reference (URL or base64)."""

File = NewType("File", str)
"""File reference (URL or path)."""

# =============================================================================
# Database/PostgreSQL Specific Scalars
# =============================================================================

LTree = NewType("LTree", str)
"""PostgreSQL ltree path (e.g., "root.child.leaf")."""

# =============================================================================
# All exports
# =============================================================================

__all__ = [
    # Core
    "ID",
    "UUID",
    "Json",
    "Decimal",
    "Vector",
    # Date/Time
    "DateTime",
    "Date",
    "Time",
    "DateRange",
    "Duration",
    # Contact/Communication
    "Email",
    "PhoneNumber",
    "URL",
    "DomainName",
    "Hostname",
    # Location/Address
    "PostalCode",
    "Latitude",
    "Longitude",
    "Coordinates",
    "Timezone",
    "LocaleCode",
    "LanguageCode",
    "CountryCode",
    # Financial
    "IBAN",
    "CUSIP",
    "ISIN",
    "SEDOL",
    "LEI",
    "MIC",
    "CurrencyCode",
    "Money",
    "ExchangeCode",
    "ExchangeRate",
    "StockSymbol",
    "Percentage",
    # Identifiers
    "Slug",
    "SemanticVersion",
    "HashSHA256",
    "APIKey",
    "LicensePlate",
    "VIN",
    "TrackingNumber",
    "ContainerNumber",
    # Networking
    "IPAddress",
    "IPv4",
    "IPv6",
    "MACAddress",
    "CIDR",
    "Port",
    # Transportation
    "AirportCode",
    "PortCode",
    "FlightNumber",
    # Content
    "Markdown",
    "HTML",
    "MimeType",
    "Color",
    "Image",
    "File",
    # Database
    "LTree",
]
