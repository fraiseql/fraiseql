# FraiseQL 50 Scalars - Master Implementation Plan

**Project**: FraiseQL Scalar Library Expansion
**Target**: 50 Enterprise-Grade Scalar Types
**Status**: Ready for Implementation
**Date**: 2025-11-09
**Version**: 2.0 (Updated with Complete Inventory)

---

## Executive Summary

This document provides a **complete implementation plan** for expanding FraiseQL's scalar library from 16 types to **66 types** (50 new scalars), providing comprehensive enterprise-grade GraphQL support.

### Vision

Transform FraiseQL into the **most comprehensive type-safe GraphQL framework** with best-in-class scalar validation for:
- ✅ Global SaaS platforms (i18n support)
- ✅ Financial & trading systems (securities, currencies, exchanges)
- ✅ Logistics & e-commerce (shipping, tracking, vehicles)
- ✅ Content management (media, publishing, communication)
- ✅ Technical infrastructure (APIs, networks, security)

---

## Current State vs. Target State

### Currently Implemented in FraiseQL ✅
**16 scalars**:

| Category | Scalars |
|----------|---------|
| **Network** | CIDR, Hostname, IpAddress, MacAddress, Port |
| **Date/Time** | Date, DateTime, DateRange |
| **Geographic** | Coordinates |
| **Identifiers** | UUID |
| **Data** | JSON, LTree |
| **Communication** | EmailAddress |
| **i18n** | LanguageCode, LocaleCode, Timezone (NEW #127) |

### Target: 50 New Scalars by Category

| Category | Count | Examples |
|----------|-------|----------|
| **i18n Types** | 0 | ✅ Already implemented (#127) |
| **Financial Types** | 11 | CurrencyCode, Money, StockSymbol, ISIN, CUSIP, LEI |
| **Geographic Types** | 3 | AirportCode, PortCode, PostalCode |
| **Network Types** | 4 | DomainName, ApiKey, HashSHA256, SemanticVersion |
| **Logistics Types** | 6 | TrackingNumber, ContainerNumber, VIN, IBAN |
| **Content Types** | 3 | PhoneNumber, URL, Image, File |
| **Data Types** | 3 | Color, Slug, MimeType |
| **Date/Time Types** | 2 | Time, Duration |
| **Structured Types** | 2 | Markdown, HTML |
| **Existing Overlap** | 16 | Date, DateTime, Email, UUID, JSON, etc. |

**Net New Scalars**: **34 scalars** (50 total minus 16 already implemented)

---

## Complete Scalar Inventory (50 Types)

### Category 1: i18n Types (3 types) ✅ COMPLETE

| Scalar | SpecQL | PostgreSQL | GraphQL | Status |
|--------|--------|------------|---------|--------|
| LanguageCode | languageCode | TEXT | LanguageCode | ✅ Implemented #127 |
| LocaleCode | localeCode | TEXT | LocaleCode | ✅ Implemented #127 |
| Timezone | timezone | TEXT | Timezone | ✅ Implemented #127 |

---

### Category 2: Financial Types (11 types) ❌ NEW

| # | Scalar | SpecQL | PostgreSQL | GraphQL | Priority | Time |
|---|--------|--------|------------|---------|----------|------|
| 1 | **CurrencyCode** | currencyCode | TEXT | CurrencyCode | P1 | 1.5h |
| 2 | **Money** | money | NUMERIC(19,4) | Money | P1 | 2h |
| 3 | **Percentage** | percentage | NUMERIC(5,2) | Percentage | P1 | 1.5h |
| 4 | **ExchangeRate** | exchangeRate | NUMERIC(20,8) | ExchangeRate | P1 | 2h |
| 5 | **StockSymbol** | stockSymbol | TEXT | StockSymbol | P2 | 1.5h |
| 6 | **ISIN** | isin | TEXT | ISIN | P2 | 1.5h |
| 7 | **CUSIP** | cusip | TEXT | CUSIP | P3 | 1.5h |
| 8 | **SEDOL** | sedol | TEXT | SEDOL | P3 | 1.5h |
| 9 | **LEI** | lei | TEXT | LEI | P3 | 1.5h |
| 10 | **MIC** | mic | TEXT | MIC | P3 | 1.5h |
| 11 | **ExchangeCode** | exchangeCode | TEXT | ExchangeCode | P2 | 1.5h |

**Subtotal**: 11 types, ~18 hours

**Validation Specs**:
- **CurrencyCode**: ISO 4217, 3 uppercase letters (USD, EUR, GBP, JPY)
- **Money**: NUMERIC(19,4), supports negative, no currency info
- **Percentage**: NUMERIC(5,2), range 0-100 (or 0-1 with scale)
- **ExchangeRate**: NUMERIC(20,8), positive only, high precision for crypto
- **StockSymbol**: 1-5 uppercase + optional class suffix (.A, .B)
- **ISIN**: 12 characters (2 country + 9 security + 1 check digit)
- **CUSIP**: 9 characters (US/Canada securities)
- **SEDOL**: 7 characters (UK securities)
- **LEI**: 20 characters (global legal entity identifier)
- **MIC**: 4 characters (ISO 10383 market identifier)
- **ExchangeCode**: 2-6 uppercase letters (NYSE, NASDAQ, LSE)

---

### Category 3: Geographic Types (6 types)

| # | Scalar | SpecQL | PostgreSQL | GraphQL | Status | Priority | Time |
|---|--------|--------|------------|---------|--------|----------|------|
| 1 | **Coordinates** | coordinates | POINT | Coordinates | ✅ Exists | - | - |
| 2 | **Latitude** | latitude | NUMERIC(10,8) | Latitude | ❌ New | P2 | 1.5h |
| 3 | **Longitude** | longitude | NUMERIC(11,8) | Longitude | ❌ New | P2 | 1.5h |
| 4 | **AirportCode** | airportCode | TEXT | AirportCode | ❌ New | P2 | 1.5h |
| 5 | **PortCode** | portCode | TEXT | PortCode | ❌ New | P3 | 1.5h |
| 6 | **PostalCode** | postalCode | TEXT | PostalCode | ❌ New | P1 | 1.5h |

**Subtotal**: 5 new types, ~7.5 hours

**Validation Specs**:
- **Latitude**: -90 to 90, up to 8 decimal places
- **Longitude**: -180 to 180, up to 8 decimal places
- **AirportCode**: IATA format, 3 uppercase letters (LAX, JFK, LHR)
- **PortCode**: UN/LOCODE, 5 letters (2 country + 3 location)
- **PostalCode**: International format, alphanumeric + spaces/hyphens

---

### Category 4: Network Types (6 types)

| # | Scalar | SpecQL | PostgreSQL | GraphQL | Status | Priority | Time |
|---|--------|--------|------------|---------|--------|----------|------|
| 1 | **IpAddress** | ipAddress | INET | IpAddress | ✅ Exists | - | - |
| 2 | **MacAddress** | macAddress | MACADDR | MacAddress | ✅ Exists | - | - |
| 3 | **DomainName** | domainName | TEXT | DomainName | ❌ New | P2 | 1.5h |
| 4 | **ApiKey** | apiKey | TEXT | ApiKey | ❌ New | P2 | 1.5h |
| 5 | **HashSHA256** | hashSHA256 | TEXT | HashSHA256 | ❌ New | P3 | 1.5h |
| 6 | **SemanticVersion** | semanticVersion | TEXT | SemanticVersion | ❌ New | P2 | 1.5h |

**Subtotal**: 4 new types, ~6 hours

**Validation Specs**:
- **DomainName**: RFC compliant, similar to Hostname but stricter
- **ApiKey**: Alphanumeric + hyphens/underscores, 16-128 chars
- **HashSHA256**: 64 hexadecimal characters
- **SemanticVersion**: semver format (MAJOR.MINOR.PATCH[-prerelease][+build])

---

### Category 5: Logistics Types (6 types) ❌ NEW

| # | Scalar | SpecQL | PostgreSQL | GraphQL | Priority | Time |
|---|--------|--------|------------|---------|----------|------|
| 1 | **TrackingNumber** | trackingNumber | TEXT | TrackingNumber | P2 | 1.5h |
| 2 | **ContainerNumber** | containerNumber | TEXT | ContainerNumber | P3 | 2h |
| 3 | **LicensePlate** | licensePlate | TEXT | LicensePlate | P3 | 1.5h |
| 4 | **VIN** | vin | TEXT | VIN | P2 | 2h |
| 5 | **FlightNumber** | flightNumber | TEXT | FlightNumber | P2 | 1.5h |
| 6 | **IBAN** | iban | TEXT | IBAN | P2 | 2h |

**Subtotal**: 6 types, ~10.5 hours

**Validation Specs**:
- **TrackingNumber**: 8-30 alphanumeric (UPS, FedEx, DHL, USPS formats)
- **ContainerNumber**: ISO 6346 (3 letters + U/J/Z + 6 digits + check digit)
- **LicensePlate**: International format, alphanumeric + spaces/hyphens
- **VIN**: 17 characters, ISO 3779/3780, check digit validation
- **FlightNumber**: IATA airline code (2 letters) + 1-4 digits + optional letter
- **IBAN**: ISO 13616, up to 34 alphanumeric, country-specific validation

---

### Category 6: Content Types (5 types)

| # | Scalar | SpecQL | PostgreSQL | GraphQL | Status | Priority | Time |
|---|--------|--------|------------|---------|--------|----------|------|
| 1 | **Email** | email | TEXT | Email | ✅ Exists | - | - |
| 2 | **PhoneNumber** | phoneNumber | TEXT | PhoneNumber | ❌ New | P1 | 1.5h |
| 3 | **URL** | url | TEXT | URL | ❌ New | P1 | 1.5h |
| 4 | **Image** | image | TEXT | Image | ❌ New | P1 | 1.5h |
| 5 | **File** | file | TEXT | File | ❌ New | P2 | 1.5h |

**Subtotal**: 4 new types, ~6 hours

**Validation Specs**:
- **PhoneNumber**: E.164 format, +[country][number], 7-15 digits
- **URL**: HTTP/HTTPS only, RFC 3986
- **Image**: URL or path with image extensions (jpg, png, gif, webp, svg)
- **File**: URL or path, any extension or no extension

---

### Category 7: Data Types (4 types)

| # | Scalar | SpecQL | PostgreSQL | GraphQL | Status | Priority | Time |
|---|--------|--------|------------|---------|--------|----------|------|
| 1 | **UUID** | uuid | UUID | UUID | ✅ Exists | - | - |
| 2 | **Color** | color | TEXT | Color | ❌ New | P2 | 1h |
| 3 | **Slug** | slug | TEXT | Slug | ❌ New | P1 | 1.5h |
| 4 | **MimeType** | mimeType | TEXT | MimeType | ❌ New | P2 | 1.5h |

**Subtotal**: 3 new types, ~4 hours

**Validation Specs**:
- **Color**: Hex color code (#RRGGBB or #RGB), case-insensitive
- **Slug**: Lowercase, hyphens, alphanumeric, no leading/trailing hyphens
- **MimeType**: type/subtype format (e.g., application/json, image/png)

---

### Category 8: Date/Time Types (4 types)

| # | Scalar | SpecQL | PostgreSQL | GraphQL | Status | Priority | Time |
|---|--------|--------|------------|---------|--------|----------|------|
| 1 | **Date** | date | DATE | Date | ✅ Exists | - | - |
| 2 | **DateTime** | datetime | TIMESTAMPTZ | DateTime | ✅ Exists | - | - |
| 3 | **Time** | time | TIME | Time | ❌ New | P2 | 1.5h |
| 4 | **Duration** | duration | INTERVAL | Duration | ❌ New | P2 | 2h |

**Subtotal**: 2 new types, ~3.5 hours

**Validation Specs**:
- **Time**: HH:MM:SS or HH:MM format, 00:00:00 to 23:59:59
- **Duration**: ISO 8601 duration (P[n]Y[n]M[n]DT[n]H[n]M[n]S) or PostgreSQL interval

---

### Category 9: Structured Types (3 types)

| # | Scalar | SpecQL | PostgreSQL | GraphQL | Status | Priority | Time |
|---|--------|--------|------------|---------|--------|----------|------|
| 1 | **Markdown** | markdown | TEXT | Markdown | ❌ New | P2 | 1h |
| 2 | **HTML** | html | TEXT | HTML | ❌ New | P2 | 1h |
| 3 | **JSON** | json | JSONB | JSON | ✅ Exists | - | - |

**Subtotal**: 2 new types, ~2 hours

**Validation Specs**:
- **Markdown**: Minimal validation, store as-is
- **HTML**: Minimal validation (optional: sanitization on input)

---

## Summary Statistics

### Total Scope

| Metric | Count |
|--------|-------|
| **Total Inventory** | 50 types |
| **Already Implemented** | 16 types |
| **Net New Required** | 34 types |
| **Estimated Hours** | 58-62 hours |
| **Estimated Weeks** | 8-10 weeks (at ~6h/week) |

### By Priority

| Priority | Count | Hours | Use Case |
|----------|-------|-------|----------|
| **P1** | 11 | 17h | Core functionality (money, phone, url, slug, postal) |
| **P2** | 14 | 21h | Enhanced features (financial, logistics, geographic) |
| **P3** | 9 | 14h | Specialized (securities codes, containers) |

### By Category

| Category | New Types | Hours | Business Impact |
|----------|-----------|-------|-----------------|
| Financial | 11 | 18h | High (trading, fintech) |
| Logistics | 6 | 10.5h | High (e-commerce, shipping) |
| Content | 4 | 6h | High (CMS, communications) |
| Geographic | 5 | 7.5h | Medium (location services) |
| Network | 4 | 6h | Medium (APIs, infrastructure) |
| Data | 3 | 4h | Medium (general utilities) |
| Date/Time | 2 | 3.5h | Medium (scheduling) |
| Structured | 2 | 2h | Low (content formatting) |

---

## Phased Implementation Plan

### Phase 1: Foundation (Weeks 1-2) - Priority 1
**Goal**: Core scalars for immediate business needs

**Scalars** (11 types, ~17 hours):
1. CurrencyCode (1.5h)
2. Money (2h)
3. Percentage (1.5h)
4. ExchangeRate (2h)
5. PhoneNumber (1.5h)
6. URL (1.5h)
7. Image (1.5h)
8. Slug (1.5h)
9. PostalCode (1.5h)
10. Color (1h)
11. MimeType (1.5h)

**Deliverables**:
- 11 scalar implementations
- 11 comprehensive test suites
- Module exports updated
- Integration tests
- Documentation

**Business Value**:
- Contact management (phone, url)
- E-commerce (money, currency, postal code)
- Content management (slug, image)
- Financial tracking (percentage, exchange rate)

---

### Phase 2: Financial & Logistics (Weeks 3-5) - Priority 2
**Goal**: Enterprise financial and logistics support

**Scalars** (14 types, ~21 hours):
1. StockSymbol (1.5h)
2. ISIN (1.5h)
3. ExchangeCode (1.5h)
4. TrackingNumber (1.5h)
5. VIN (2h)
6. FlightNumber (1.5h)
7. IBAN (2h)
8. Latitude (1.5h)
9. Longitude (1.5h)
10. AirportCode (1.5h)
11. DomainName (1.5h)
12. ApiKey (1.5h)
13. SemanticVersion (1.5h)
14. File (1.5h)

**Additional Scalars**:
15. Time (1.5h)
16. Duration (2h)
17. Markdown (1h)
18. HTML (1h)

**Deliverables**:
- 14+ scalar implementations
- Comprehensive test coverage
- Financial system integration examples
- Logistics system integration examples
- API documentation

**Business Value**:
- Trading platforms (stock symbols, ISIN, exchange codes)
- E-commerce logistics (tracking, flights, vehicles)
- Banking integration (IBAN)
- Geographic services (lat/long, airports)
- API management (keys, versioning)

---

### Phase 3: Specialized (Weeks 6-8) - Priority 3
**Goal**: Specialized industry-specific scalars

**Scalars** (9 types, ~14 hours):
1. CUSIP (1.5h)
2. SEDOL (1.5h)
3. LEI (1.5h)
4. MIC (1.5h)
5. ContainerNumber (2h)
6. LicensePlate (1.5h)
7. PortCode (1.5h)
8. HashSHA256 (1.5h)

**Deliverables**:
- 9 scalar implementations
- Specialized validation logic
- Industry-specific examples
- Complete documentation
- Migration guides

**Business Value**:
- Securities trading (CUSIP, SEDOL, LEI, MIC)
- Shipping/logistics (containers, ports)
- Vehicle tracking (license plates)
- Security (hash validation)

---

### Phase 4: Testing & Documentation (Weeks 9-10)
**Goal**: Comprehensive validation and documentation

**Activities**:
- Integration testing across all 34 new scalars
- Performance benchmarking
- Documentation completion
- Usage examples for all categories
- Migration guides
- Release preparation

**Deliverables**:
- 100% test coverage verified
- Performance validated
- Complete API documentation
- Category-specific guides
- Migration scripts
- Release notes

---

## Implementation Template (Standard Pattern)

### File Structure for Each Scalar

```python
"""[Scalar Name] scalar type for [purpose] validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# Validation regex/logic
_[SCALAR]_REGEX = re.compile(r"^[pattern]$")


def serialize_[scalar](value: Any) -> str | None:
    """Serialize [scalar] to string."""
    if value is None:
        return None

    value_str = str(value)

    if not _[SCALAR]_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid [scalar]: {value}. [Requirements]"
        )

    return value_str


def parse_[scalar]_value(value: Any) -> str:
    """Parse [scalar] from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"[Scalar] must be a string, got {type(value).__name__}")

    if not _[SCALAR]_REGEX.match(value):
        raise GraphQLError(
            f"Invalid [scalar]: {value}. [Requirements]"
        )

    return value


def parse_[scalar]_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse [scalar] from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("[Scalar] must be a string")

    return parse_[scalar]_value(ast.value)


[Scalar]Scalar = GraphQLScalarType(
    name="[ScalarName]",
    description="[Description with examples and references]",
    serialize=serialize_[scalar],
    parse_value=parse_[scalar]_value,
    parse_literal=parse_[scalar]_literal,
)


class [Scalar]Field(str, ScalarMarker):
    """[Documentation]

    Example:
        >>> from fraiseql.types import [Scalar]
        >>>
        >>> @fraiseql.type
        ... class [Entity]:
        ...     [field]: [Scalar]
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "[Scalar]Field":
        """Create a new [Scalar]Field instance with validation."""
        if not _[SCALAR]_REGEX.match(value):
            raise ValueError(f"Invalid [scalar]: {value}. [Requirements]")
        return super().__new__(cls, value)
```

---

## Detailed Specifications by Category

### Financial Types (11 scalars)

#### 1. CurrencyCode

**Validation**: ISO 4217 three-letter currency code
- **Format**: 3 uppercase letters
- **Regex**: `^[A-Z]{3}$`
- **Examples**: USD, EUR, GBP, JPY, CHF, CAD, AUD

```python
# Validation
_CURRENCY_CODE_REGEX = re.compile(r"^[A-Z]{3}$")

# Optional: Validate against known ISO 4217 codes
_VALID_CURRENCY_CODES = {
    "USD", "EUR", "GBP", "JPY", "CHF", "CAD", "AUD", "NZD",
    "CNY", "HKD", "SGD", "INR", "BRL", "RUB", "ZAR", "MXN",
    # ... full ISO 4217 list
}
```

#### 2. StockSymbol

**Validation**: Stock ticker symbol
- **Format**: 1-5 uppercase letters + optional class suffix
- **Regex**: `^[A-Z]{1,5}(\.[A-Z])?$`
- **Examples**: AAPL, MSFT, GOOGL, BRK.A, BRK.B

```python
_STOCK_SYMBOL_REGEX = re.compile(r"^[A-Z]{1,5}(\.[A-Z])?$")
```

#### 3. ISIN

**Validation**: International Securities Identification Number
- **Format**: 2 country code + 9 alphanumeric + 1 check digit
- **Length**: 12 characters
- **Regex**: `^[A-Z]{2}[A-Z0-9]{9}[0-9]$`
- **Examples**: US0378331005 (Apple), GB0002374006 (BP)

```python
_ISIN_REGEX = re.compile(r"^[A-Z]{2}[A-Z0-9]{9}[0-9]$")

def _validate_isin_check_digit(isin: str) -> bool:
    """Validate ISIN check digit using Luhn algorithm."""
    # Implementation of Luhn mod-10 algorithm
    pass
```

#### 4. CUSIP

**Validation**: Committee on Uniform Security Identification Procedures
- **Format**: 9 characters (8 alphanumeric + 1 check digit)
- **Regex**: `^[0-9]{3}[A-Z0-9]{5}[0-9]$`
- **Examples**: 037833100 (Apple)

```python
_CUSIP_REGEX = re.compile(r"^[0-9]{3}[A-Z0-9]{5}[0-9]$")
```

#### 5. SEDOL

**Validation**: Stock Exchange Daily Official List
- **Format**: 7 characters (6 alphanumeric + 1 check digit)
- **Regex**: `^[B-DF-HJ-NP-TV-Z0-9]{6}[0-9]$`
- **Examples**: 0263494 (BP)

```python
_SEDOL_REGEX = re.compile(r"^[B-DF-HJ-NP-TV-Z0-9]{6}[0-9]$")
```

#### 6. LEI

**Validation**: Legal Entity Identifier
- **Format**: 20 alphanumeric characters
- **Regex**: `^[A-Z0-9]{18}[0-9]{2}$`
- **Examples**: 549300E9PC51EN656011

```python
_LEI_REGEX = re.compile(r"^[A-Z0-9]{18}[0-9]{2}$")
```

#### 7. MIC

**Validation**: Market Identifier Code (ISO 10383)
- **Format**: 4 uppercase letters
- **Regex**: `^[A-Z]{4}$`
- **Examples**: XNYS (NYSE), XNAS (NASDAQ), XLON (London)

```python
_MIC_REGEX = re.compile(r"^[A-Z]{4}$")
```

#### 8. ExchangeCode

**Validation**: Stock exchange code
- **Format**: 2-6 uppercase letters
- **Regex**: `^[A-Z]{2,6}$`
- **Examples**: NYSE, NASDAQ, LSE, TSE, HKEX

```python
_EXCHANGE_CODE_REGEX = re.compile(r"^[A-Z]{2,6}$")
```

---

### Logistics Types (6 scalars)

#### 1. TrackingNumber

**Validation**: Shipping tracking number
- **Format**: 8-30 alphanumeric characters
- **Regex**: `^[A-Z0-9]{8,30}$`
- **Examples**: 1Z999AA10123456784 (UPS), 123456789012 (FedEx)

```python
_TRACKING_NUMBER_REGEX = re.compile(r"^[A-Z0-9]{8,30}$")
```

#### 2. ContainerNumber

**Validation**: Shipping container number (ISO 6346)
- **Format**: 3 letters + U/J/Z + 6 digits + check digit
- **Regex**: `^[A-Z]{3}[UJZ][0-9]{6}[0-9]$`
- **Examples**: CSQU3054383

```python
_CONTAINER_NUMBER_REGEX = re.compile(r"^[A-Z]{3}[UJZ][0-9]{6}[0-9]$")

def _validate_container_check_digit(container: str) -> bool:
    """Validate ISO 6346 check digit."""
    # Implementation
    pass
```

#### 3. VIN

**Validation**: Vehicle Identification Number (ISO 3779/3780)
- **Format**: 17 characters (no I, O, Q)
- **Regex**: `^[A-HJ-NPR-Z0-9]{17}$`
- **Examples**: 1HGBH41JXMN109186

```python
_VIN_REGEX = re.compile(r"^[A-HJ-NPR-Z0-9]{17}$")

def _validate_vin_check_digit(vin: str) -> bool:
    """Validate VIN check digit (position 9)."""
    # Implementation
    pass
```

#### 4. FlightNumber

**Validation**: Flight number (IATA format)
- **Format**: 2 letters (airline) + 1-4 digits + optional letter
- **Regex**: `^[A-Z]{2}[0-9]{1,4}[A-Z]?$`
- **Examples**: AA100, BA2276, LH400

```python
_FLIGHT_NUMBER_REGEX = re.compile(r"^[A-Z]{2}[0-9]{1,4}[A-Z]?$")
```

#### 5. IBAN

**Validation**: International Bank Account Number (ISO 13616)
- **Format**: 2 country code + 2 check digits + up to 30 alphanumeric
- **Length**: 15-34 characters (country-specific)
- **Regex**: `^[A-Z]{2}[0-9]{2}[A-Z0-9]{11,30}$`
- **Examples**: GB82WEST12345698765432, DE89370400440532013000

```python
_IBAN_REGEX = re.compile(r"^[A-Z]{2}[0-9]{2}[A-Z0-9]{11,30}$")

def _validate_iban_check_digits(iban: str) -> bool:
    """Validate IBAN check digits using mod-97."""
    # Implementation
    pass
```

#### 6. LicensePlate

**Validation**: Vehicle license plate (international)
- **Format**: Alphanumeric with optional spaces/hyphens
- **Length**: 2-12 characters
- **Regex**: `^[A-Z0-9 -]{2,12}$`
- **Examples**: ABC-1234, XYZ 789, 12AB34CD

```python
_LICENSE_PLATE_REGEX = re.compile(r"^[A-Z0-9 -]{2,12}$")
```

---

### Geographic Types (5 new scalars)

#### 1. AirportCode

**Validation**: IATA airport code
- **Format**: 3 uppercase letters
- **Regex**: `^[A-Z]{3}$`
- **Examples**: LAX, JFK, LHR, CDG, NRT

```python
_AIRPORT_CODE_REGEX = re.compile(r"^[A-Z]{3}$")
```

#### 2. PortCode

**Validation**: UN/LOCODE port code
- **Format**: 5 letters (2 country + 3 location)
- **Regex**: `^[A-Z]{2}[A-Z0-9]{3}$`
- **Examples**: USNYC (New York), CNSHA (Shanghai), NLRTM (Rotterdam)

```python
_PORT_CODE_REGEX = re.compile(r"^[A-Z]{2}[A-Z0-9]{3}$")
```

#### 3. PostalCode

**Validation**: International postal/ZIP code
- **Format**: Alphanumeric with spaces/hyphens
- **Length**: 3-10 characters
- **Regex**: `^[A-Z0-9][A-Z0-9 -]{1,8}[A-Z0-9]$`
- **Examples**: 90210 (US), SW1A 1AA (UK), 75001 (France), 100-0001 (Japan)

```python
_POSTAL_CODE_REGEX = re.compile(r"^[A-Z0-9][A-Z0-9 -]{1,8}[A-Z0-9]$", re.IGNORECASE)
```

---

### Network Types (4 new scalars)

#### 1. DomainName

**Validation**: Domain name (RFC compliant)
- **Format**: Labels separated by dots (stricter than Hostname)
- **Regex**: Similar to Hostname but enforces TLD
- **Examples**: example.com, subdomain.example.co.uk

```python
_DOMAIN_NAME_REGEX = re.compile(
    r"^(?=.{1,253}$)"
    r"(?!-)[a-zA-Z0-9-]{1,63}(?<!-)"
    r"(\."
    r"(?!-)[a-zA-Z0-9-]{1,63}(?<!-))*"
    r"\.[a-zA-Z]{2,}$"
)
```

#### 2. ApiKey

**Validation**: API key or access token
- **Format**: Alphanumeric with hyphens/underscores
- **Length**: 16-128 characters
- **Regex**: `^[A-Za-z0-9_-]{16,128}$`
- **Examples**: test_key_4eC39HqLyjWDarjtT1zdp7dc

```python
_API_KEY_REGEX = re.compile(r"^[A-Za-z0-9_-]{16,128}$")
```

#### 3. HashSHA256

**Validation**: SHA256 hash
- **Format**: 64 hexadecimal characters
- **Regex**: `^[a-fA-F0-9]{64}$`
- **Examples**: e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855

```python
_HASH_SHA256_REGEX = re.compile(r"^[a-fA-F0-9]{64}$")
```

#### 4. SemanticVersion

**Validation**: Semantic versioning (semver)
- **Format**: MAJOR.MINOR.PATCH[-prerelease][+build]
- **Regex**: Complex semver pattern
- **Examples**: 1.0.0, 2.3.4-alpha.1, 3.0.0-beta+20130313144700

```python
_SEMVER_REGEX = re.compile(
    r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)"
    r"(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?"
    r"(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$"
)
```

---

## Module Export Updates

### `src/fraiseql/types/scalars/__init__.py`

**Complete `__all__` list** (alphabetically ordered with all 66 scalars):

```python
__all__ = [
    # Network Types
    "CIDRScalar",
    "DomainNameScalar",          # NEW
    "HostnameScalar",
    "IpAddressScalar",
    "MacAddressScalar",
    "PortScalar",
    "SubnetMaskScalar",

    # Financial Types
    "CurrencyCodeScalar",        # NEW
    "CUSIPScalar",               # NEW
    "ExchangeCodeScalar",        # NEW
    "ExchangeRateScalar",        # NEW
    "ISINScalar",                # NEW
    "LEIScalar",                 # NEW
    "MICScalar",                 # NEW
    "MoneyScalar",               # NEW
    "PercentageScalar",          # NEW
    "SEDOLScalar",               # NEW
    "StockSymbolScalar",         # NEW

    # Geographic Types
    "AirportCodeScalar",         # NEW
    "CoordinateScalar",
    "LatitudeScalar",            # NEW
    "LongitudeScalar",           # NEW
    "PortCodeScalar",            # NEW
    "PostalCodeScalar",          # NEW

    # Logistics Types
    "ContainerNumberScalar",     # NEW
    "FlightNumberScalar",        # NEW
    "IBANScalar",                # NEW
    "LicensePlateScalar",        # NEW
    "TrackingNumberScalar",      # NEW
    "VINScalar",                 # NEW

    # Content Types
    "EmailAddressScalar",
    "FileScalar",                # NEW
    "ImageScalar",               # NEW
    "PhoneNumberScalar",         # NEW
    "URLScalar",                 # NEW

    # Data Types
    "ColorScalar",               # NEW
    "JSONScalar",
    "MimeTypeScalar",            # NEW
    "SlugScalar",                # NEW
    "UUIDScalar",

    # Date/Time Types
    "DateRangeScalar",
    "DateScalar",
    "DateTimeScalar",
    "DurationScalar",            # NEW
    "TimeScalar",                # NEW

    # i18n Types
    "LanguageCodeScalar",        # NEW (Issue #127)
    "LocaleCodeScalar",          # NEW (Issue #127)
    "TimezoneScalar",            # NEW (Issue #127)

    # Structured Types
    "HTMLScalar",                # NEW
    "LTreeScalar",
    "MarkdownScalar",            # NEW

    # Network/Security
    "ApiKeyScalar",              # NEW
    "HashSHA256Scalar",          # NEW
    "SemanticVersionScalar",     # NEW
]
```

---

## Testing Strategy

### Test Coverage Requirements

Each scalar must have:

1. **Serialization Tests** (5-10 tests):
   - Valid values (multiple examples)
   - None handling
   - Invalid format errors
   - Edge cases (min/max length, boundary values)

2. **Parsing Tests** (5-10 tests):
   - Valid values
   - Invalid format errors
   - Type validation (non-string inputs)

3. **Field Marker Tests** (3-5 tests):
   - Constructor validation
   - Type checking
   - Invalid constructor errors

4. **Literal Parsing Tests** (3-5 tests):
   - AST node handling
   - Invalid literal formats
   - Non-string literal errors

**Total per scalar**: 16-30 test cases
**Total for 34 scalars**: ~544-1020 test cases

### Test Execution Commands

```bash
# Individual scalar
uv run pytest tests/unit/core/type_system/test_currency_code_scalar.py -v

# By category
uv run pytest tests/unit/core/type_system/ -k "financial" -v
uv run pytest tests/unit/core/type_system/ -k "logistics" -v

# All new scalars
uv run pytest tests/unit/core/type_system/ -v

# Coverage report
uv run pytest --cov=src/fraiseql/types/scalars --cov-report=html

# Full test suite (verify no regressions)
uv run pytest --tb=short

# Quality checks
uv run ruff check
uv run mypy
```

---

## Quality Assurance

### Per-Scalar Checklist

- [ ] Scalar file created (`src/fraiseql/types/scalars/[scalar].py`)
- [ ] Test file created (`tests/unit/core/type_system/test_[scalar]_scalar.py`)
- [ ] All test categories covered (serialize, parse, field, literal)
- [ ] Tests pass: `uv run pytest tests/unit/core/type_system/test_[scalar]_scalar.py -v`
- [ ] Export added to `scalars/__init__.py`
- [ ] Export added to `types/__init__.py`
- [ ] Linting passes: `uv run ruff check`
- [ ] Type checking passes: `uv run mypy`
- [ ] Documentation complete with examples
- [ ] Integration test created

### Phase Completion Checklist

- [ ] All phase scalars implemented
- [ ] All tests passing
- [ ] No regressions: `uv run pytest --tb=short`
- [ ] Code coverage ≥95%: `uv run pytest --cov`
- [ ] Linting clean
- [ ] Type checking clean
- [ ] Integration tests passing
- [ ] Documentation updated
- [ ] Usage examples added
- [ ] Git commits with clear messages

---

## Success Criteria

### Functional Success

✅ **All 34 New Scalars Implemented**:
- 11 Financial types
- 6 Logistics types
- 4 Content types
- 5 Geographic types
- 4 Network types
- 3 Data types
- 2 Date/Time types
- 2 Structured types

✅ **Quality Standards**:
- 100% test coverage
- All tests passing
- Zero regressions
- Linting clean
- Type checking clean

✅ **Integration Success**:
- Works with `@fraiseql.type` and `@fraiseql.input`
- PostgreSQL integration working
- GraphQL schema generation working
- Documentation complete

### Business Impact

✅ **Enterprise-Ready FraiseQL**:
- 66 total scalars (16 existing + 50 new)
- Comprehensive coverage for:
  - Global SaaS platforms
  - Financial/trading systems
  - E-commerce/logistics platforms
  - Content management systems
  - Technical infrastructure

✅ **Industry Leadership**:
- Most comprehensive GraphQL scalar library
- Best-in-class type safety
- Production-ready validation
- Well-documented with examples

---

## Timeline Summary

| Phase | Duration | Scalars | Hours | Deliverable |
|-------|----------|---------|-------|-------------|
| **Phase 1** | Weeks 1-2 | 11 P1 | 17h | Foundation scalars |
| **Phase 2** | Weeks 3-5 | 14 P2 | 21h | Financial & logistics |
| **Phase 3** | Weeks 6-8 | 9 P3 | 14h | Specialized scalars |
| **Phase 4** | Weeks 9-10 | Testing | 10h | Documentation & QA |
| **Total** | 10 weeks | 34 scalars | 62h | Complete library |

**At 6 hours/week**: 10-11 weeks
**At 10 hours/week**: 6-7 weeks

---

## References

### Standards
- **ISO 639-1**: Language codes
- **ISO 4217**: Currency codes
- **ISO 6346**: Container numbers
- **ISO 3779/3780**: Vehicle identification
- **ISO 10383**: Market identifier codes
- **ISO 13616**: IBAN
- **BCP 47**: Locale tags
- **RFC 3986**: URI syntax
- **RFC 5322**: Email addresses
- **E.164**: Phone numbers

### Implementation
- **FraiseQL Patterns**: `/home/lionel/code/fraiseql/src/fraiseql/types/scalars/`
- **Test Patterns**: `/home/lionel/code/fraiseql/tests/unit/core/type_system/`
- **i18n Implementation**: `/home/lionel/code/fraiseql/docs/implementation-plans/I18N_SCALARS_IMPLEMENTATION_PLAN.md`
- **Inventory**: `/home/lionel/code/printoptim_backend_poc/fraiseql_scalar_types_inventory.md`

---

**Document Version**: 2.0
**Last Updated**: 2025-11-09
**Status**: Ready for Implementation
**Scope**: 34 new scalars (50 total inventory, 16 already implemented)
**Total Effort**: 58-62 hours over 10 weeks
