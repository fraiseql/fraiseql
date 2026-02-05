# Custom Scalar Types Reference

**Status:** ✅ Production Ready
**Version:** FraiseQL v2.0.0-alpha.1+
**Categories**: 18 domain categories
**Total Scalars**: 56 types

## Table of Contents

1. [Overview](#overview)
2. [Scalar Categories](#scalar-categories)
3. [Database Mappings](#database-mappings)
4. [Using Scalars in Type Definitions](#using-scalars-in-type-definitions)
5. [Performance Considerations](#performance-considerations)
6. [Best Practices](#best-practices)
7. [Scalar Import Locations](#scalar-import-locations)
8. [Summary](#summary)

---

## Overview

FraiseQL provides 56 domain-specific custom scalar types beyond GraphQL's built-in scalars. These scalars provide:

- **Type safety**: Validation at the GraphQL layer
- **Database mapping**: Native PostgreSQL column types
- **Format standardization**: ISO standards, RFC compliance, domain conventions
- **Serialization**: Consistent JSON representations
- **Error handling**: Clear validation error messages

All scalars are imported from the main FraiseQL package and available for use in type definitions:

```python
from fraiseql.types import Date, DateTime, UUID, Money, Vector, IpAddressString
```

---

## Scalar Categories

### 1. Core System Scalars

#### **UUID**

- **GraphQL Type**: `UUID`
- **Format**: RFC 4122 UUID format
- **Validation**: Standard UUID v4 validation
- **Database Type**: `uuid`
- **Examples**: `550e8400-e29b-41d4-a716-446655440000`
- **Use Cases**: Entity identifiers, unique resource IDs
- **Notes**: Hyphenated format required (xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx)

#### **ID**

- **GraphQL Type**: `ID` (built-in GraphQL scalar)
- **Format**: Any string or number
- **Python Marker**: `IDField`, `ID` (NewType)
- **Notes**:
  - GraphQL specification allows any serializable string-like value
  - UUID validation handled at input level via `SchemaConfig.id_policy`
  - Can represent database integers, strings, UUIDs, or custom formats
- **Use Cases**: Primary keys, entity identifiers

#### **JSON**

- **GraphQL Type**: `JSON`
- **Format**: Arbitrary JSON-serializable values
- **Python Marker**: `JSONField`
- **Accepts**: Objects, arrays, strings, numbers, booleans, null
- **Database Type**: `jsonb` (PostgreSQL JSONB with operators)
- **Examples**: `{"key": "value", "nested": {"deep": true}}`
- **Use Cases**: Flexible data storage, metadata, semi-structured data

---

### 2. Temporal Scalars

Temporal scalars represent dates, times, durations, and date ranges with ISO 8601 compliance.

#### **Date**

- **GraphQL Type**: `Date`
- **Format**: ISO 8601 date format
- **Pattern**: `YYYY-MM-DD`
- **Database Type**: `date`
- **Examples**: `2025-01-11`, `2024-12-25`, `1999-05-15`
- **Validation**: Valid calendar dates (accounts for leap years)
- **Timezone**: Date only (no timezone)
- **Use Cases**: Birthdate, event dates, contract dates

#### **DateTime**

- **GraphQL Type**: `DateTime`
- **Format**: ISO 8601 datetime with timezone
- **Pattern**: `YYYY-MM-DDTHH:mm:ssZ` (always UTC)
- **Timezone**: Always UTC, serialized with 'Z' suffix
- **Database Type**: `timestamp with time zone`
- **Examples**:
  - `2025-01-11T15:30:00Z`
  - `2024-12-25T00:00:00Z`
  - `2025-01-10T23:45:30Z`
- **Precision**: Milliseconds supported
- **Use Cases**: Created timestamps, event times, log timestamps
- **Notes**: Input accepts timezone offsets (converted to UTC), output always in UTC with Z

#### **Time**

- **GraphQL Type**: `Time`
- **Format**: 24-hour time format
- **Pattern**: `HH:MM` or `HH:MM:SS`
- **Database Type**: `time`
- **Examples**: `14:30`, `09:15:30`, `23:59:00`, `00:00:00`
- **Validation**: Hours 0-23, Minutes 0-59, Seconds 0-59
- **Timezone**: No timezone (wall clock time)
- **Use Cases**: Business hours, meeting times, scheduled tasks
- **Notes**: Seconds and milliseconds optional in input

#### **Duration**

- **GraphQL Type**: `Duration`
- **Format**: ISO 8601 duration format
- **Pattern**: `P[n]Y[n]M[n]DT[n]H[n]M[n]S`
- **Components**:
  - `P`: Period marker (required)
  - `Y`: Years
  - `M`: Months (before T) or Minutes (after T)
  - `D`: Days
  - `T`: Time separator (required if time components present)
  - `H`: Hours
  - `S`: Seconds
- **Examples**:
  - `P1Y2M3DT4H5M6S` (1 year, 2 months, 3 days, 4 hours, 5 minutes, 6 seconds)
  - `PT30M` (30 minutes)
  - `P1D` (1 day)
  - `PT2H` (2 hours)
  - `P6M` (6 months)
- **Use Cases**: Subscription periods, retention policies, SLA durations
- **Notes**: No timezone, represents an abstract duration

#### **DateRange**

- **GraphQL Type**: `DateRange`
- **Format**: Range notation with inclusive/exclusive boundaries
- **Database Type**: `daterange` (PostgreSQL range type)
- **Syntax**:
  - Inclusive: `[YYYY-MM-DD, YYYY-MM-DD]` (square brackets)
  - Exclusive: `(YYYY-MM-DD, YYYY-MM-DD)` (parentheses)
  - Mixed: `[YYYY-MM-DD, YYYY-MM-DD)` or `(YYYY-MM-DD, YYYY-MM-DD]`
- **Examples**:
  - `[2025-01-01, 2025-12-31]` (all of 2025, inclusive)
  - `(2024-12-31, 2026-01-01)` (all of 2025, exclusive boundaries)
  - `[2025-01-01, 2025-06-30)` (Jan 1 inclusive to Jun 30 exclusive)
- **Use Cases**: Project timelines, availability windows, contract periods
- **Database Operators**: Can use PostgreSQL range operators (`,`@>`,`<@`, etc.)

#### **Timezone**

- **GraphQL Type**: `Timezone`
- **Format**: IANA timezone identifier
- **Pattern**: `Region/City` (case-sensitive)
- **Examples**:
  - `America/New_York` (EST/EDT)
  - `Europe/Paris` (CET/CEST)
  - `Asia/Tokyo` (JST)
  - `Australia/Sydney` (AEDT/AEST)
  - `UTC` (Coordinated Universal Time)
- **Validation**: Must be valid IANA timezone
- **Case-Sensitive**: Yes, standard IANA format is case-sensitive
- **Use Cases**: User timezone preferences, timestamp interpretation, scheduling
- **Notes**: PostgreSQL supports IANA timezones for timestamp conversion

---

### 3. Geographic Scalars

Geographic scalars represent coordinates, latitude/longitude with GPS precision.

#### **Coordinate**

- **GraphQL Type**: `Coordinate`
- **Format**: Multiple input formats supported, single canonical format
- **Input Formats**:
  - String: `"37.7749,-122.4194"` or `"(37.7749,-122.4194)"`
  - Array: `[37.7749, -122.4194]`
  - Object: `{lat: 37.7749, lng: -122.4194}`
- **Output Format**: `{lat: float, lng: float}`
- **Validation**:
  - Latitude: -90.0 to +90.0 degrees
  - Longitude: -180.0 to +180.0 degrees
  - Precision: Up to 8 decimal places (≈1.1 mm accuracy)
- **Database Type**: PostgreSQL `POINT` (internally stores longitude, latitude in that order)
- **Examples**:
  - `37.7749,-122.4194` (San Francisco)
  - `-33.8688,151.2093` (Sydney)
  - `40.7128,-74.0060` (New York)
  - `0.0,0.0` (Null Island)
- **Use Cases**: Location tracking, geolocation, map markers, geographic queries
- **Database Operators**: PostgreSQL point operators (`<->`, distance calculations)

#### **Latitude**

- **GraphQL Type**: `Latitude`
- **Format**: Decimal degrees
- **Range**: -90.0 to +90.0 degrees
  - Negative: Southern hemisphere
  - Positive: Northern hemisphere
  - 0: Equator
- **Precision**: Up to 8 decimal places
- **Resolution per Decimal Place**:
  - 1 decimal: ≈11.1 km
  - 2 decimals: ≈1.1 km
  - 3 decimals: ≈111 m
  - 4 decimals: ≈11.1 m
  - 5 decimals: ≈1.1 m
  - 6 decimals: ≈0.11 m (11 cm)
  - 7 decimals: ≈1.1 cm
  - 8 decimals: ≈1.1 mm
- **Examples**: `40.7128`, `-33.8688`, `0.0`, `51.5074`
- **Use Cases**: Geolocation filtering, distance calculations, map visualization
- **Database Type**: `numeric` or `double precision` depending on precision needs

#### **Longitude**

- **GraphQL Type**: `Longitude`
- **Format**: Decimal degrees
- **Range**: -180.0 to +180.0 degrees
  - Negative: Western hemisphere
  - Positive: Eastern hemisphere
  - Prime meridian: ±0 or ±180
- **Precision**: Up to 8 decimal places
- **Examples**: `-74.0060`, `151.2093`, `0.0`, `-0.1278`
- **Use Cases**: Geolocation filtering, distance calculations, geographic boundaries
- **Database Type**: `numeric` or `double precision`
- **Notes**: Value wrapping at ±180 (cyclical coordinate)

---

### 4. Network Scalars

Network scalars represent IP addresses, MAC addresses, domains, and URLs.

#### **IpAddressString**

- **GraphQL Type**: `IpAddressString`
- **Format**: IP address (IPv4 or IPv6)
- **IPv4 Examples**: `192.168.1.1`, `10.0.0.1`, `8.8.8.8`
- **IPv6 Examples**: `2001:db8::1`, `::1`, `fe80::1`
- **CIDR Notation**: Accepted with `/prefix` (e.g., `192.168.1.0/24`)
- **Database Type**: `inet` (PostgreSQL, supports both address and netmask)
- **Validation**: Valid IPv4 or IPv6 address format
- **Use Cases**:
  - Server IP addresses
  - Client IP logging
  - Network configuration
  - ACL (Access Control List) rules
- **Notes**: PostgreSQL `inet` type supports:
  - IP address + optional netmask
  - Comparison operators
  - Network containment queries

#### **SubnetMaskString**

- **GraphQL Type**: `SubnetMaskString`
- **Format**: IPv4 netmask notation (CIDR prefix format also accepted)
- **Examples**: `255.255.255.0`, `255.255.0.0`, `255.0.0.0`
- **CIDR Equivalents**:
  - `/24` = `255.255.255.0`
  - `/16` = `255.255.0.0`
  - `/8` = `255.0.0.0`
  - `/32` = `255.255.255.255`
- **Validation**: Valid IPv4 netmask (contiguous 1-bits from left)
- **Use Cases**: Network configuration, subnet planning, ACL specification
- **Notes**: Only IPv4 (IPv6 uses CIDR prefix notation directly)

#### **CIDR**

- **GraphQL Type**: `CIDR`
- **Format**: CIDR notation (Classless Inter-Domain Routing)
- **Pattern**: `address/prefix`
- **IPv4 Example**: `192.168.1.0/24`, `10.0.0.0/8`
- **IPv6 Examples**: `2001:db8::/32`, `fe80::/10`
- **Prefix Length**:
  - IPv4: 0-32 bits
  - IPv6: 0-128 bits
- **Validation**:
  - `strict=False` (default): Allows host bits (e.g., `192.168.1.5/24`)
  - `strict=True`: Rejects host bits (requires `192.168.1.0/24`)
- **Database Type**: `cidr` (PostgreSQL, network address type)
- **Use Cases**:
  - Network ranges
  - IP allowlisting/blocklisting
  - VPC/subnet definitions
  - Firewall rules
- **Database Operators**: PostgreSQL supports network operators (`<<`, `>>`, `&&`)

#### **MacAddress**

- **GraphQL Type**: `MacAddress`
- **Format**: Media Access Control (MAC/Physical) address
- **Input Formats** (all equivalent, normalized to canonical):
  - Colon: `00:11:22:33:44:55`
  - Hyphen: `00-11-22-33-44-55`
  - Cisco: `0011.2233.4455`
  - Bare: `001122334455`
- **Canonical Output**: Uppercase colon-separated (e.g., `00:11:22:33:44:55`)
- **Database Type**: `macaddr` (PostgreSQL)
- **Validation**:
  - 48 bits total (6 octets)
  - Valid hexadecimal digits
  - Proper separators
- **Examples**: `00:11:22:33:44:55`, `FF:FF:FF:FF:FF:FF`
- **Use Cases**:
  - Device identification
  - Network interface discovery
  - DHCP configuration
  - ARP (Address Resolution Protocol) tables
- **Notes**: PostgreSQL `macaddr` type also supports network functions

#### **Hostname**

- **GraphQL Type**: `Hostname`
- **Format**: RFC 1123 hostname format
- **Rules**:
  - Labels: 1-63 characters each
  - Max total length: 253 characters
  - Valid characters: `a-z`, `0-9`, hyphen (`-`)
  - No leading or trailing hyphens per label
  - No TLD required (single labels allowed)
- **Examples**:
  - `localhost`
  - `api-server`
  - `my-app.local`
  - `server-1`
  - `db.internal`
- **Case**: Normalized to lowercase
- **Use Cases**:
  - Server names
  - Container hostnames
  - Internal DNS names
  - Local service discovery
- **Notes**: Less strict than DomainName (doesn't require TLD)

#### **DomainName**

- **GraphQL Type**: `DomainName`
- **Format**: RFC-compliant fully qualified domain name (FQDN)
- **Rules**:
  - Labels: 1-63 characters each
  - Max total length: 253 characters
  - Valid characters: `a-z`, `0-9`, hyphen (`-`)
  - No leading or trailing hyphens per label
  - TLD required (at least 2 labels)
  - Valid TLDs: `.com`, `.org`, `.co.uk`, etc.
- **Examples**:
  - `example.com`
  - `api.example.com`
  - `subdomain.example.co.uk`
  - `service.internal.company.com`
- **Case**: Normalized to lowercase
- **Use Cases**:
  - Public domain names
  - Email domains
  - API endpoints
  - CORS origins
- **Notes**: Stricter than Hostname (requires TLD)

#### **URL**

- **GraphQL Type**: `URL`
- **Format**: RFC 3986 compliant URL
- **Required Components**: Scheme, domain
- **Scheme**: Must be `http://` or `https://`
- **Structure**: `https://domain.com[/path][?query][#fragment]`
- **Examples**:
  - `https://example.com`
  - `https://api.example.com/v1/users`
  - `https://example.com:8080/path?key=value#section`
  - `http://localhost:3000`
- **Validation**:
  - Valid domain
  - Proper encoding (spaces → %20, etc.)
  - Valid characters in each URL component
- **Use Cases**:
  - Webhooks
  - Redirects
  - External API endpoints
  - Resource URLs
- **Notes**: HTTPS preferred for security

---

### 5. Financial Scalars

Financial scalars represent monetary values, currency codes, and exchange rates with proper precision.

#### **Money**

- **GraphQL Type**: `Money`
- **Format**: Decimal number with exactly 4 decimal places
- **Precision**: Up to 15 digits before decimal
- **Database Type**: `NUMERIC(19,4)` (PostgreSQL)
- **Range**:
  - Min: `-9999999999999.9999`
  - Max: `9999999999999.9999`
- **Examples**:
  - `123.45` (stored as `123.4500`)
  - `-999.9999` (negative amount)
  - `1000000.00` (million)
  - `0.01` (one cent)
- **Precision Purpose**: Accounts for fractional cents in currency conversion
- **Use Cases**:
  - Prices
  - Account balances
  - Transaction amounts
  - Exchange amounts
- **Notes**: Always 4 decimal places for accounting precision

#### **CurrencyCode**

- **GraphQL Type**: `CurrencyCode`
- **Format**: ISO 4217 three-letter currency code
- **Validation**: Must be valid ISO 4217 code
- **Case**: Normalized to uppercase
- **Examples**:
  - `USD` (US Dollar)
  - `EUR` (Euro)
  - `GBP` (British Pound)
  - `JPY` (Japanese Yen)
  - `CHF` (Swiss Franc)
- **Common Codes**: USD, EUR, GBP, JPY, AUD, CAD, CHF, CNY, SEK, NZD, MXN, SGD, HKD, NOK, KRW
- **Use Cases**:
  - Currency specification for Money values
  - Multi-currency transactions
  - Exchange rates
  - Payment processing
- **Notes**: Often paired with Money scalar for complete monetary values

#### **Percentage**

- **GraphQL Type**: `Percentage`
- **Format**: Decimal number 0.00 to 100.00
- **Precision**: Exactly 2 decimal places
- **Database Type**: `NUMERIC(5,2)` (PostgreSQL)
- **Range**: `0.00` to `100.00`
- **Examples**:
  - `25.5` (25.5%)
  - `100` (stored as `100.00`)
  - `0.01` (0.01%)
  - `99.99`
- **Interpretation**: Direct percentage value (25.5 means 25.5%, not 0.255)
- **Use Cases**:
  - Tax rates
  - Discount rates
  - Interest rates
  - Commission percentages
  - Completion percentages
- **Notes**: 2 decimal places for percentage precision

#### **ExchangeRate**

- **GraphQL Type**: `ExchangeRate`
- **Format**: High-precision decimal
- **Database Type**: `NUMERIC(20,8)` (PostgreSQL)
- **Precision**: Up to 12 digits before decimal, 8 after
- **Examples**:
  - `1.23456789` (EUR to USD)
  - `1234.5` (JPY to USD)
  - `0.8765` (GBP to USD)
- **Use Cases**:
  - Currency conversion rates
  - Cross-rate calculations
  - Historical rate storage
- **Notes**: High precision for accurate currency conversions

---

### 6. Financial Identifiers

Financial identifier scalars represent standardized security and financial institution identifiers.

#### **ISIN**

- **GraphQL Type**: `ISIN`
- **Format**: International Securities Identification Number (ISO 6166)
- **Length**: Exactly 12 characters
- **Structure**:
  - Characters 1-2: ISO 3166-1 alpha-2 country code
  - Characters 3-11: 9-character alphanumeric base number
  - Character 12: Check digit (Luhn algorithm)
- **Validation**: Luhn algorithm check digit verification
- **Examples**:
  - `US0378331005` (Apple Inc.)
  - `GB0002374006` (BP p.l.c.)
  - `US5949181045` (Microsoft Corporation)
  - `IE00B4L5Y983` (iShares MSCI World UCITS ETF)
- **Use Cases**:
  - Equity identification
  - Bond identification
  - Fund identification
  - International securities trading
- **Notes**: Globally unique identifier for any security worldwide

#### **CUSIP**

- **GraphQL Type**: `CUSIP`
- **Format**: Committee on Uniform Security Identification Procedures
- **Length**: Exactly 9 characters
- **Structure**:
  - Characters 1-3: Issuer code (alphanumeric)
  - Characters 4-8: Issue code (alphanumeric)
  - Character 9: Check digit
- **Coverage**: US and Canadian securities
- **Validation**: Check digit verification
- **Examples**:
  - `037833100` (Apple Inc.)
  - `594918104` (Microsoft Corporation)
  - `000481022` (AT&T Inc.)
- **Use Cases**:
  - North American security identification
  - Settlement processing
  - Trading systems
- **Notes**: CUSIP number is proprietary; ISIN is international equivalent

#### **SEDOL**

- **GraphQL Type**: `SEDOL`
- **Format**: Stock Exchange Daily Official List (ISO 10149)
- **Length**: Exactly 7 characters
- **Structure**:
  - Characters 1-6: Base code (alphanumeric, excluding A, E, I, O, U, I, O, Q)
  - Character 7: Check digit
- **Excluded Characters**: A, E, I, O, U, and letters I, O, Q
- **Coverage**: UK securities
- **Validation**: Check digit verification
- **Examples**:
  - `0263494` (BP p.l.c.)
  - `0540528` (HSBC Holdings plc)
  - `0540814` (Shell plc)
- **Use Cases**:
  - UK and European securities trading
  - London Stock Exchange
  - UK fund identification
- **Notes**: Vowel restriction reduces confusion with numbers

#### **LEI**

- **GraphQL Type**: `LEI`
- **Format**: Legal Entity Identifier (ISO 17442)
- **Length**: Exactly 20 characters
- **Structure**:
  - Characters 1-4: Local Operating Unit (LOU) code
  - Characters 5-18: Entity-specific code
  - Characters 19-20: Check digits (mod-97 algorithm)
- **Scope**: Globally unique legal entities
- **Validation**: Check digit verification
- **Examples**:
  - `549300E9PC51EN656011` (Apple Inc.)
  - `5493001KJTIIGC8K1162` (Microsoft Corporation)
- **Use Cases**:
  - Banking and finance regulations
  - EMIR/Dodd-Frank compliance
  - Counter-party identification
  - Financial reporting
- **Notes**: Required for large financial transactions in many jurisdictions

---

### 7. Stock Market Scalars

Stock market scalars represent ticker symbols, exchange codes, and market identifiers.

#### **StockSymbol**

- **GraphQL Type**: `StockSymbol`
- **Format**: Stock ticker symbol
- **Length**: 1-5 uppercase letters
- **Class Suffix**: Optional (`.A`, `.B`, `.C` for multiple share classes)
- **Case**: Normalized to uppercase
- **Examples**:
  - `AAPL` (Apple)
  - `MSFT` (Microsoft)
  - `BRK.A` (Berkshire Hathaway Class A)
  - `BRK.B` (Berkshire Hathaway Class B)
  - `GOOGL` (Alphabet/Google)
  - `IBM` (IBM)
- **Validation**: 1-5 alphanumeric, hyphen, or period
- **Use Cases**:
  - Stock identification
  - Trading systems
  - Market data feeds
  - Financial analysis
- **Notes**: Same symbol can exist on different exchanges

#### **ExchangeCode**

- **GraphQL Type**: `ExchangeCode`
- **Format**: Market exchange identifier
- **Length**: 2-6 uppercase letters
- **Case**: Normalized to uppercase
- **Common Examples**:
  - `NYSE` (New York Stock Exchange)
  - `NASDAQ` (NASDAQ)
  - `LSE` (London Stock Exchange)
  - `TSE` (Tokyo Stock Exchange)
  - `HKE` (Hong Kong Exchanges)
  - `SGX` (Singapore Exchange)
- **Use Cases**:
  - Market specification
  - Trading venue identification
  - Regional market routing
  - Compliance reporting
- **Notes**: Symbol+Exchange combo uniquely identifies a tradable security

#### **MIC**

- **GraphQL Type**: `MIC`
- **Format**: Market Identifier Code (ISO 10383)
- **Length**: Exactly 4 alphanumeric characters
- **Standard**: ISO 10383
- **Examples**:
  - `XNYS` (NYSE - New York Stock Exchange)
  - `XNAS` (NASDAQ)
  - `XETRA` (Xetra - Deutsche Börse)
  - `XLSE` (LSE - London Stock Exchange)
  - `XTSE` (Toronto Stock Exchange)
  - `XHKG` (Hong Kong Stock Exchange)
- **Use Cases**:
  - Standardized market identification
  - ISO-compliant systems
  - International trading systems
  - Market data standards
- **Notes**: Official ISO standard for market identification

---

### 8. Banking & Payment Scalars

Banking and payment scalars represent account numbers, phone numbers, and payment identifiers.

#### **IBAN**

- **GraphQL Type**: `IBAN`
- **Format**: International Bank Account Number (ISO 13616)
- **Structure**:
  - Positions 1-2: Country code (ISO 3166-1 alpha-2)
  - Positions 3-4: Check digits (mod-97)
  - Positions 5+: Country-specific BBAN (Basic Bank Account Number)
- **Length**: Varies by country (15-32 characters)
- **Validation**:
  - Check digit verification (mod-97)
  - Country-specific format and length
  - Valid country codes
- **Case**: Normalized to uppercase
- **Examples**:
  - `GB82WEST12345698765432` (UK, Barclays)
  - `DE89370400440532013000` (Germany)
  - `FR1420041010050500013M02606` (France)
  - `IT60X0542811101000000123456` (Italy)
  - `ES9121000418450200051332` (Spain)
  - `NL91ABNA0417164300` (Netherlands)
- **Country-Specific Lengths**:
  - UK: 22 characters
  - Germany: 22 characters
  - France: 27 characters
  - Italy: 27 characters
  - Spain: 24 characters
  - Netherlands: 18 characters
- **Use Cases**:
  - International wire transfers
  - Direct debit setup
  - SEPA payments
  - Bank account verification
- **Notes**: More common in Europe; Asia uses SWIFT/BIC

#### **PhoneNumber**

- **GraphQL Type**: `PhoneNumber`
- **Format**: E.164 international format
- **Pattern**: `+[country code][number]`
- **Country Code**: 1-3 digits
- **Local Number**: 6-14 digits
- **Total Length**: 7-15 digits (after + prefix)
- **Examples**:
  - `+14155552671` (San Francisco)
  - `+447911123456` (UK mobile)
  - `+33123456789` (France)
  - `+81312345678` (Japan)
  - `+886212345678` (Taiwan)
- **Validation**:
  - Valid country code
  - Proper length for country
  - Digits only
- **Case**: No spaces or formatting
- **Use Cases**:
  - User contact information
  - SMS notifications
  - Two-factor authentication
  - Contact verification
- **Notes**: E.164 is international standard for phone numbers

---

### 9. Identifier Scalars

Identifier scalars represent unique identifiers and URL-friendly slugs.

#### **Slug**

- **GraphQL Type**: `Slug`
- **Format**: URL-friendly identifier
- **Valid Characters**: `a-z`, `0-9`, hyphen (`-`)
- **Rules**:
  - All lowercase
  - No leading or trailing hyphens
  - No consecutive hyphens
  - Must be non-empty
- **Examples**:
  - `hello-world`
  - `my-blog-post`
  - `api-documentation`
  - `feature-request-123`
- **Use Cases**:
  - URL paths
  - Article titles
  - Resource identifiers
  - Human-readable URLs
- **Notes**: Often generated from title or name fields

---

### 10. Cryptographic & Security Scalars

Cryptographic scalars represent hashes, keys, and security-related values.

#### **HashSHA256**

- **GraphQL Type**: `HashSHA256`
- **Format**: SHA256 hexadecimal hash
- **Length**: Exactly 64 hexadecimal characters
- **Validation**:
  - Exactly 64 hex digits (0-9, a-f, A-F)
  - Valid hexadecimal
- **Case**: Both lowercase and uppercase accepted
- **Examples**:
  - `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` (empty string hash)
  - `2c26b46911185131006745196031e88e3e3ca8bfc5fbc36cb7c97b5d8b282d42` (first hash)
- **Use Cases**:
  - File integrity verification
  - Password hashing (when salted)
  - Document fingerprints
  - Data deduplication
- **Notes**: Always 256 bits = 64 hex characters

#### **ApiKey**

- **GraphQL Type**: `ApiKey`
- **Format**: Access token or API key
- **Length**: 16-128 characters
- **Valid Characters**:
  - Alphanumeric (`a-z`, `A-Z`, `0-9`)
  - Hyphen (`-`)
  - Underscore (`_`)
- **Examples**:
  - `test_key_4eC39HqLyjWDarjtT1zdp7dc`
  - `sk_live_ABC123xyz-def456`
  - `api_key_abcdef0123456789`
- **Use Cases**:
  - API authentication
  - Service credentials
  - OAuth tokens (simplified format)
  - Third-party integrations
- **Notes**: Should be treated as sensitive data (never log, use HTTPS)

---

### 11. Vehicle & Transportation Scalars

Vehicle and transportation scalars represent vehicle identifiers, license plates, and tracking numbers.

#### **VIN**

- **GraphQL Type**: `VIN`
- **Format**: Vehicle Identification Number (ISO 3779/3780)
- **Length**: Exactly 17 characters
- **Valid Characters**: A-H, J-N, P, R-Z, 0-9 (no I, O, Q)
- **Structure**:
  - Positions 1-3: World Manufacturer Identifier (WMI)
  - Positions 4-8: Vehicle Descriptor Section (VDS)
  - Position 9: Check digit
  - Positions 10-17: Vehicle Identifier Section (VIS)
- **Validation**: Check digit at position 9
- **Examples**:
  - `1HGBH41JXMN109186` (Honda)
  - `JH4KA8260MC000000` (Acura)
  - `WBADT63492G942742` (BMW)
  - `JTDKN3AU5E0022619` (Toyota)
- **Use Cases**:
  - Vehicle registration
  - Insurance claims
  - Recall identification
  - Used car market
- **Notes**: Unique for each vehicle, used globally

#### **LicensePlate**

- **GraphQL Type**: `LicensePlate`
- **Format**: International vehicle license plate
- **Length**: 2-12 characters
- **Valid Characters**:
  - Alphanumeric (`A-Z`, `0-9`)
  - Space
  - Hyphen (`-`)
- **Examples**:
  - `ABC123` (US standard)
  - `NY 1234 AB` (US with state)
  - `ABC-1234` (Hyphenated)
  - `AB19 CDZ` (UK format)
  - `75 ABC 123` (French format)
- **Format Variation**: Highly country-specific
- **Use Cases**:
  - Vehicle identification
  - Parking systems
  - Traffic enforcement
  - Vehicle tracking
- **Notes**: Format varies significantly by country

#### **AirportCode**

- **GraphQL Type**: `AirportCode`
- **Format**: IATA airport code
- **Length**: Exactly 3 uppercase letters
- **Standard**: IATA (International Air Transport Association)
- **Case**: Normalized to uppercase
- **Examples**:
  - `LAX` (Los Angeles International)
  - `JFK` (John F. Kennedy, New York)
  - `LHR` (London Heathrow)
  - `CDG` (Paris Charles de Gaulle)
  - `NRT` (Narita, Tokyo)
  - `SYD` (Sydney)
- **Use Cases**:
  - Flight bookings
  - Travel information
  - Routing
  - Airline systems
- **Notes**: Different from ICAO codes (4 letters)

#### **TrackingNumber**

- **GraphQL Type**: `TrackingNumber`
- **Format**: Shipping tracking number
- **Length**: 8-30 characters
- **Valid Characters**: Alphanumeric (`A-Z`, `0-9`)
- **Examples**:
  - `1Z999AA10123456784` (UPS format)
  - `123456789012` (FedEx format)
  - `9400111899223456789012345678` (USPS format)
  - `DHL tracking example`
- **Use Cases**:
  - Shipment tracking
  - Logistics
  - Package delivery
  - Return management
- **Notes**: Format varies by carrier

---

### 12. Localization Scalars

Localization scalars represent language and locale information.

#### **LanguageCode**

- **GraphQL Type**: `LanguageCode`
- **Format**: ISO 639-1 two-letter language code
- **Standard**: ISO 639-1
- **Length**: Exactly 2 letters
- **Case**: Normalized to lowercase
- **Examples**:
  - `en` (English)
  - `fr` (French)
  - `de` (German)
  - `es` (Spanish)
  - `ja` (Japanese)
  - `zh` (Chinese)
  - `ru` (Russian)
  - `pt` (Portuguese)
  - `ar` (Arabic)
  - `hi` (Hindi)
- **Use Cases**:
  - UI language selection
  - Content localization
  - Multi-language support
  - User preferences
- **Notes**: 2-letter codes; 3-letter codes (ISO 639-2/3) also exist

#### **LocaleCode**

- **GraphQL Type**: `LocaleCode`
- **Format**: BCP 47 locale identifier
- **Structure**: `language[-script][-region][-variant]`
- **Components**:
  - `language`: ISO 639-1 code (required, 2 letters)
  - `script`: ISO 15924 code (optional, 4 letters)
  - `region`: ISO 3166-1 alpha-2 code (optional, 2 letters)
  - `variant`: Custom variant (optional)
- **Simple Examples**: `en-US`, `fr-FR`, `de-DE`, `ja-JP`, `zh-CN`
- **Complex Examples**:
  - `zh-Hans-CN` (Simplified Chinese, China)
  - `zh-Hant-TW` (Traditional Chinese, Taiwan)
  - `sr-Cyrl-RS` (Serbian in Cyrillic, Serbia)
  - `pt-BR` (Portuguese, Brazil)
- **Use Cases**:
  - Full localization (language + region)
  - Currency/date/time formatting
  - Regional variants
  - Content targeting
- **Notes**: More specific than LanguageCode, includes region

---

### 13. Versioning Scalar

#### **SemanticVersion**

- **GraphQL Type**: `SemanticVersion`
- **Format**: Semantic Versioning (semver) specification
- **Pattern**: `MAJOR.MINOR.PATCH[-prerelease][+build]`
- **Components**:
  - `MAJOR`: Major version (breaking changes)
  - `MINOR`: Minor version (backwards-compatible features)
  - `PATCH`: Patch version (bug fixes)
  - `prerelease` (optional): Pre-release identifier (alpha, beta, rc)
  - `build` (optional): Build metadata
- **Examples**:
  - `1.0.0` (Production release)
  - `2.3.4` (Standard semver)
  - `1.0.0-alpha` (Alpha pre-release)
  - `2.0.0-beta.1` (Beta pre-release)
  - `3.0.0-rc.1` (Release candidate)
  - `1.0.0+20130313144700` (Build metadata)
  - `2.1.3-beta+build.123` (Combined)
- **Validation**:
  - Numeric MAJOR.MINOR.PATCH
  - Valid pre-release format
  - Valid build metadata format
- **Use Cases**:
  - Version tracking
  - Dependency management
  - API versioning
  - Software releases
- **Notes**: Industry standard for versioning

---

### 14. Content & Format Scalars

Content scalars represent rich content and file formats.

#### **Color**

- **GraphQL Type**: `Color`
- **Format**: Hexadecimal color code
- **Formats**:
  - Short: `#RGB` (3 hex digits)
  - Long: `#RRGGBB` (6 hex digits)
- **Validation**: Valid hexadecimal digits
- **Case**: Normalized to lowercase
- **Examples**:
  - `#ff0000` (Red)
  - `#00ff00` (Green)
  - `#0000ff` (Blue)
  - `#f00` (Red, short form)
  - `#3366cc` (Blue)
  - `#ffffff` (White)
  - `#000000` (Black)
- **Use Cases**:
  - Brand colors
  - UI customization
  - Theme configuration
  - Design systems
- **Notes**: RGB color space (not CMYK or HSL)

#### **HTMLScalar**

- **GraphQL Type**: `HTML`
- **Format**: HTML content (any valid HTML)
- **Database Type**: `text`
- **Validation**: Minimal (raw HTML allowed)
- **Use Cases**:
  - Rich text content
  - Page templates
  - Email templates
  - Blog content with formatting
- **Security Notes**: Use with Content Security Policy (CSP)
- **Notes**: No validation; sanitization recommended on display

#### **MarkdownScalar**

- **GraphQL Type**: `Markdown`
- **Format**: Markdown content (GitHub Flavored Markdown compatible)
- **Database Type**: `text`
- **Validation**: Minimal (raw markdown allowed)
- **Use Cases**:
  - Documentation
  - Blog posts
  - User-generated content
  - Comments with formatting
- **Notes**: No validation; render to HTML on display

#### **MimeType**

- **GraphQL Type**: `MimeType`
- **Format**: MIME media type (RFC 6838)
- **Pattern**: `type/subtype[+suffix]`
- **Examples**:
  - `text/plain`
  - `text/html`
  - `application/json`
  - `application/xml`
  - `image/png`
  - `image/jpeg`
  - `audio/mpeg`
  - `video/mp4`
  - `application/pdf`
  - `text/csv`
- **Type Categories**:
  - `text/*`: Text-based (plain, html, css)
  - `image/*`: Images (png, jpeg, gif, webp, svg)
  - `audio/*`: Audio (mpeg, wav, ogg)
  - `video/*`: Video (mp4, webm, avi)
  - `application/*`: Applications (json, pdf, xml, octet-stream)
- **Use Cases**:
  - File upload validation
  - Content-Type headers
  - API request/response types
  - File type detection
- **Notes**: Case-insensitive (normalized to lowercase)

#### **Image**

- **GraphQL Type**: `Image`
- **Format**: Image file URL or path
- **Supported Formats**:
  - Extensions: `.jpg`, `.jpeg`, `.png`, `.gif`, `.webp`, `.svg`, `.bmp`, `.tiff`, `.tif`
  - Case-insensitive matching
- **Examples**:
  - `https://example.com/image.jpg`
  - `/uploads/avatar.png`
  - `s3://bucket/images/photo.webp`
- **Validation**: Valid image file extension
- **Use Cases**:
  - User avatars
  - Product images
  - Gallery images
  - Thumbnails
- **Notes**: Validation is extension-based; actual image validation recommended

#### **FileScalar**

- **GraphQL Type**: `File`
- **Format**: File reference (URL or file path)
- **Can Represent**:
  - HTTP URLs
  - File system paths
  - Cloud storage URLs (S3, GCS, Azure)
- **Use Cases**:
  - File downloads
  - Document references
  - Attachment storage
  - File links
- **Notes**: Generic file type; specific formats use specialized scalars

---

### 15. Postal & Address Scalars

Postal scalars represent address-related information.

#### **PostalCode**

- **GraphQL Type**: `PostalCode`
- **Format**: International postal code
- **Validation**: Flexible format (no strict validation)
- **Examples**:
  - `90210` (US ZIP code)
  - `75001` (French postal code)
  - `SW1A 1AA` (UK postcode)
  - `M5V 3A8` (Canadian postal code)
  - `1000` (Austrian postal code)
- **Use Cases**:
  - Shipping addresses
  - Billing addresses
  - Location identification
  - Delivery routing
- **Notes**: Highly country-specific formats

#### **PortCode**

- **GraphQL Type**: `PortCode`
- **Format**: Shipping port identifier
- **Length**: Typically 3-5 characters
- **Standard**: UN/LOCODE (sometimes)
- **Examples**:
  - `USLA` (Port of Los Angeles)
  - `USNY` (Port of New York)
  - `GBFXT` (Southampton)
  - `JPTYO` (Tokyo)
- **Use Cases**:
  - Shipping routes
  - Port operations
  - Logistics planning
  - International trade
- **Notes**: Various coding standards exist (UN/LOCODE, IATA)

---

### 16. Networking Port

#### **Port**

- **GraphQL Type**: `Port`
- **Format**: Network port number
- **Type**: Integer
- **Range**: 1-65535
  - Reserved: 0 (not allowed)
  - Well-known: 1-1023
  - Registered: 1024-49151
  - Dynamic: 49152-65535
- **Examples**:
  - `80` (HTTP)
  - `443` (HTTPS/SSL)
  - `22` (SSH)
  - `3306` (MySQL)
  - `5432` (PostgreSQL)
  - `8080` (Alternative HTTP)
  - `6379` (Redis)
  - `27017` (MongoDB)
  - `3000` (Node.js default)
- **Validation**: Integer 1-65535
- **Use Cases**:
  - Network configuration
  - Service endpoints
  - Docker port mapping
  - Database connections
- **Notes**: Port 0 reserved; beyond 65535 invalid

---

### 17. Hierarchical Scalar

#### **LTreePath**

- **GraphQL Type**: `LTreePath`
- **Format**: PostgreSQL `ltree` hierarchical path
- **Structure**: Dot-separated labels
- **Pattern**: `label.sublabel.subsubLabel`
- **Validation**:
  - Labels 1-255 characters
  - Valid characters: `a-z`, `A-Z`, `0-9`, `_`
  - Dots separate labels
- **Database Type**: `ltree` (PostgreSQL extension)
- **Examples**:
  - `top` (root)
  - `top.science` (category)
  - `top.science.physics` (subcategory)
  - `top.science.physics.quantum` (sub-subcategory)
  - `org.company.department.team`
  - `products.electronics.computers.laptops`
- **Database Operators**:
  - `@>` (contains)
  - `<@` (is contained by)
  - `~` (matches pattern)
  - `?` (matches pattern)
- **Use Cases**:
  - Category hierarchies
  - Organizational structures
  - File system-like data
  - Taxonomy navigation
  - Menu structures
- **Notes**: PostgreSQL `ltree` extension must be installed

---

### 18. Vector Scalars (pgvector)

Vector scalars represent embeddings and vector data for semantic search and RAG (Retrieval-Augmented Generation).

#### **Vector**

- **GraphQL Type**: `Vector`
- **Format**: List of floating-point numbers
- **Precision**: 32-bit floats (IEEE 754)
- **Database Type**: PostgreSQL `vector` (pgvector extension)
- **Dimension**: Variable (specified at column creation)
- **Examples**:
  - `[0.1, 0.2, 0.3]` (3-dimensional)
  - `[0.5, -0.3, 0.1, 0.2, -0.1]` (5-dimensional)
  - Typical use: 384-1536 dimensions for embeddings
- **Validation**: Array of floats
- **Distance Operators** (pgvector):
  - `<->` (L2/Euclidean distance)
  - `<#>` (Inner product)
  - `<=>` (Cosine similarity)
- **Use Cases**:
  - Semantic search
  - Recommendation engines
  - Similarity matching
  - Vector embeddings (OpenAI, Cohere, etc.)
  - RAG (Retrieval-Augmented Generation)
- **Notes**:
  - pgvector extension required
  - Common sizes: 384 (small), 768 (medium), 1536 (GPT-3)
  - Memory: ~4 bytes per dimension

#### **HalfVector**

- **GraphQL Type**: `HalfVector`
- **Format**: List of floating-point numbers (16-bit precision)
- **Precision**: 16-bit floats (half precision, bfloat16)
- **Database Type**: PostgreSQL `halfvec` (pgvector extension)
- **Dimension**: Variable
- **Memory Advantage**: 50% memory savings vs Vector (2 bytes per dimension)
- **Examples**:
  - `[0.1, 0.2, 0.3]` (3-dimensional, stored as half precision)
- **Precision Trade-off**:
  - Full precision: 32-bit (±3.4e±38)
  - Half precision: 16-bit (±6.5e±4)
  - Suitable for: Large-scale similarity search where slight precision loss is acceptable
- **Distance Operators**: Same as Vector
- **Use Cases**:
  - Large-scale embeddings (millions of vectors)
  - Cost-optimized similarity search
  - Memory-constrained systems
  - Batch similarity scoring
- **Notes**:
  - pgvector extension required
  - Requires PostgreSQL 11+
  - Recommended: 384-1536 dimensions

#### **SparseVector**

- **GraphQL Type**: `SparseVector`
- **Format**: Dictionary with indices and values
- **Structure**: `{indices: [int], values: [float]}`
- **Database Type**: PostgreSQL `sparsevec` (pgvector extension)
- **Dimension**: Implicit from max index
- **Example**:

  ```json
  {
    "indices": [0, 2, 5],
    "values": [0.1, 0.3, 0.2]
  }
  ```

- **Memory Advantage**: Extremely efficient for high-dimensional, sparse data
- **Use Cases**:
  - Text BOW (Bag of Words) embeddings
  - High-dimensional sparse data
  - TF-IDF vectors
  - Categorical embeddings
  - Term frequency vectors
- **Notes**:
  - pgvector extension required
  - Ideal for 10,000+ dimension vectors
  - Only stores non-zero values

#### **QuantizedVector**

- **GraphQL Type**: `QuantizedVector`
- **Format**: Dictionary with quantization metadata
- **Structure**: `{codebook_id: int, code: int, scale: float, offset: [float]}`
- **Database Type**: PostgreSQL `sparsevec` with metadata
- **Compression**: Product quantization (PQ) for extreme compression
- **Memory**: Dramatically reduced vs Vector or HalfVector
- **Example**:

  ```json
  {
    "codebook_id": 1,
    "code": 42,
    "scale": 0.5,
    "offset": [0.1, 0.2, 0.3]
  }
  ```

- **Use Cases**:
  - Extremely large-scale embeddings (billions of vectors)
  - Mobile/edge deployment
  - Real-time similarity search at massive scale
  - Memory-critical environments
- **Trade-offs**:
  - Highest compression
  - Reduced precision (still high for similarity)
  - Suitable for retrieval (not fine-grained comparison)
- **Notes**:
  - Most aggressive compression
  - Requires pgvector extension
  - Recommended for production at scale

---

## Database Mappings

| Scalar | PostgreSQL Type | Notes |
|--------|-----------------|-------|
| **Temporal** | | |
| Date | `date` | ISO 8601 date |
| DateTime | `timestamp with time zone` | Always UTC |
| Time | `time` | Wall clock time |
| Duration | Internal | ISO 8601 duration |
| DateRange | `daterange` | Range with operators |
| Timezone | `text` | IANA timezone |
| **Geographic** | | |
| Coordinate | `point` | (lat, lng) |
| Latitude | `numeric` | -90 to +90 |
| Longitude | `numeric` | -180 to +180 |
| **Network** | | |
| IpAddressString | `inet` | IPv4 or IPv6 |
| SubnetMaskString | `inet` | Subnet notation |
| CIDR | `cidr` | Network range |
| MacAddress | `macaddr` | Hardware address |
| Hostname | `text` | RFC 1123 |
| DomainName | `text` | RFC with TLD |
| URL | `text` | RFC 3986 |
| **Financial** | | |
| Money | `NUMERIC(19,4)` | 4 decimals |
| CurrencyCode | `text` | ISO 4217 |
| Percentage | `NUMERIC(5,2)` | 0.00-100.00 |
| ExchangeRate | `NUMERIC(20,8)` | High precision |
| **Financial IDs** | | |
| ISIN | `text` | 12 chars + check |
| CUSIP | `text` | 9 chars + check |
| SEDOL | `text` | 7 chars + check |
| LEI | `text` | 20 chars + check |
| **Stock Market** | | |
| StockSymbol | `text` | 1-5 chars |
| ExchangeCode | `text` | 2-6 chars |
| MIC | `text` | 4 chars (ISO) |
| **Banking** | | |
| IBAN | `text` | 15-32 chars |
| PhoneNumber | `text` | E.164 format |
| **Identifiers** | | |
| UUID | `uuid` | RFC 4122 |
| ID | `text` or `bigint` | Generic |
| Slug | `text` | URL-friendly |
| **Cryptographic** | | |
| HashSHA256 | `text` | 64 hex chars |
| ApiKey | `text` | 16-128 chars |
| **Vehicle** | | |
| VIN | `text` | 17 chars |
| LicensePlate | `text` | 2-12 chars |
| AirportCode | `text` | 3 chars |
| TrackingNumber | `text` | 8-30 chars |
| **Localization** | | |
| LanguageCode | `text` | ISO 639-1 |
| LocaleCode | `text` | BCP 47 |
| **Versioning** | | |
| SemanticVersion | `text` | MAJOR.MINOR.PATCH |
| **Content** | | |
| Color | `text` | #RRGGBB |
| HTMLScalar | `text` | Raw HTML |
| MarkdownScalar | `text` | Raw Markdown |
| MimeType | `text` | RFC 6838 |
| Image | `text` | URL or path |
| FileScalar | `text` | URL or path |
| **Postal** | | |
| PostalCode | `text` | Variable format |
| PortCode | `text` | 3-5 chars |
| **Port** | | |
| Port | `integer` | 1-65535 |
| **Hierarchical** | | |
| LTreePath | `ltree` | Dot-separated |
| **Vector** | | |
| Vector | `vector` | 32-bit floats |
| HalfVector | `halfvec` | 16-bit floats |
| SparseVector | `sparsevec` | Sparse values |
| QuantizedVector | `sparsevec` | Quantized with metadata |

---

## Using Scalars in Type Definitions

Scalars are used in `@fraiseql.type` decorators:

```python
from fraiseql import type, field
from fraiseql.types import (
    Date, DateTime, UUID, Money, CurrencyCode,
    EmailAddress, PhoneNumber, Vector, LTreePath
)

@type
class User:
    """A user in the system."""
    id: UUID
    email: EmailAddress
    phone: PhoneNumber | None = None
    created_at: DateTime
    born_date: Date
    last_login: DateTime | None = None
    timezone: Timezone = "UTC"

@type
class Product:
    """A product with pricing and location."""
    id: UUID
    name: str
    price: Money
    currency: CurrencyCode = "USD"
    category_path: LTreePath
    location: Coordinate
    vector_embedding: Vector

@type
class BlogPost:
    """A blog post with content and metadata."""
    id: UUID
    title: str
    slug: Slug
    content: Markdown
    created_at: DateTime
    tags: list[str]
```

---

## Performance Considerations

### Validation Overhead

- **Minimal**: Most scalars use simple pattern matching
- **Moderate**: Luhn algorithm (ISIN, CUSIP, LEI, IBAN)
- **Database**: Geographic, network operations via PostgreSQL

### Storage Efficiency

| Type | Bytes | Notes |
|------|-------|-------|
| UUID | 16 | Binary storage in DB |
| DateTime | 8 | UTC timestamp |
| Money (NUMERIC) | Variable | Precision-dependent |
| Vector (1536-dim) | ~6KB | 32-bit floats |
| HalfVector (1536-dim) | ~3KB | 16-bit floats (50% savings) |
| SparseVector | Variable | Only non-zero values |
| QuantizedVector | Minimal | Extreme compression |

---

## Best Practices

### Scalar Selection

1. **Always use specific scalars** instead of generic string:
   - ✅ `email: EmailAddress`
   - ❌ `email: str`

2. **Type-safe identifiers**:
   - ✅ `id: UUID` or `id: Slug`
   - ❌ `id: str`

3. **Precision for financial data**:
   - ✅ `price: Money` (NUMERIC with 4 decimals)
   - ❌ `price: float` (precision loss)

4. **Temporal data**:
   - ✅ `created_at: DateTime`, `birth_date: Date`
   - ❌ `created_at: str`, `birth_date: str`

5. **Geographic data**:
   - ✅ `location: Coordinate`, `latitude: Latitude`
   - ❌ `location: str`, `latitude: float`

### Validation

- Scalars validate input at GraphQL execution time
- Errors are returned as GraphQL errors (not exceptions)
- Invalid values are rejected before reaching database

### Database Operations

- Use PostgreSQL native types for indexing
- Take advantage of type-specific operators (ltree, inet, vector)
- Query optimization: index commonly filtered fields

### Security

- **Sensitive data**: Use dedicated scalar types (ApiKey, HashSHA256)
- **Email/Phone**: Validate format and consider additional verification
- **Cryptographic**: Store hashes securely, never store plaintext equivalents
- **Vector data**: Consider privacy implications for embeddings

---

## Scalar Import Locations

All scalars are available from:

```python
from fraiseql.types import (
    # Temporal
    Date, DateTime, Time, Duration, DateRange, Timezone,

    # Geographic
    Coordinate, Latitude, Longitude,

    # Network
    IpAddressString, SubnetMaskString, CIDR, MacAddress,
    Hostname, DomainName, URL,

    # Financial
    Money, CurrencyCode, Percentage, ExchangeRate,

    # Financial IDs
    ISIN, CUSIP, SEDOL, LEI,

    # Stock Market
    StockSymbol, ExchangeCode, MIC,

    # Banking
    IBAN, PhoneNumber,

    # Identifiers
    UUID, Slug,

    # Cryptographic
    HashSHA256, ApiKey,

    # Vehicle
    VIN, LicensePlate, AirportCode, TrackingNumber,

    # Localization
    LanguageCode, LocaleCode,

    # Versioning
    SemanticVersion,

    # Content
    Color, HTMLScalar, MarkdownScalar, MimeType, Image, FileScalar,

    # Postal
    PostalCode, PortCode,

    # Port
    Port,

    # Hierarchical
    LTreePath,

    # Vector
    Vector, HalfVector, SparseVector, QuantizedVector,
)
```

---

## Summary

FraiseQL's 56 custom scalar types provide:

✅ **Type Safety**: Compile-time and runtime validation
✅ **Database Integration**: Native PostgreSQL column types
✅ **Format Standards**: ISO, RFC, and domain-specific compliance
✅ **Developer Experience**: Clear error messages, intuitive validation
✅ **Performance**: Optimized serialization and database operations
✅ **Enterprise Features**: Security (crypto), localization, vector search

Whether you're building financial systems, geospatial applications, or AI-powered semantic search, FraiseQL's scalar library provides domain-specific validation and database integration.
