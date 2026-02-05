# Scalar Types Cheat Sheet

**Status:** ✅ Production Ready
**Audience:** Developers, DBAs
**Reading Time:** 5-8 minutes
**Last Updated:** 2026-02-05

Quick reference for all FraiseQL scalar types, mappings, and examples.

## String Types

| Scalar | SQL Type | Size | Use Case | Example |
|--------|----------|------|----------|---------|
| `String` | VARCHAR | Unlimited | Text, names, emails | `"John Doe"` |
| `ID` | UUID | 36 bytes | Unique identifiers | `"550e8400-e29b-41d4-a716-446655440000"` |
| `Email` | VARCHAR | 254 bytes | Email validation | `"user@example.com"` |
| `URL` | VARCHAR | Unlimited | Web addresses | `"https://example.com/path"` |
| `Phone` | VARCHAR | 20 bytes | Phone numbers | `"+1-555-123-4567"` |
| `Slug` | VARCHAR | 255 | URL-friendly text | `"my-awesome-post"` |

## Numeric Types

| Scalar | SQL Type | Range | Use Case | Example |
|--------|----------|-------|----------|---------|
| `Int` | INTEGER | -2.1B to 2.1B | Counts, ages | `42` |
| `BigInt` | BIGINT | ±9.2 quintillion | Large numbers | `9223372036854775807` |
| `Float` | FLOAT | IEEE 754 | Approximate decimals | `3.14159` |
| `Decimal` | NUMERIC | Arbitrary | Money, precise | `"99.99"` |

## Date & Time Types

| Scalar | SQL Type | Format | Use Case | Example |
|--------|----------|--------|----------|---------|
| `DateTime` | TIMESTAMP | ISO 8601 | Full date+time | `"2024-01-15T14:30:00Z"` |
| `Date` | DATE | YYYY-MM-DD | Date only | `"2024-01-15"` |
| `Time` | TIME | HH:MM:SS | Time only | `"14:30:00"` |
| `Duration` | INTERVAL | ISO 8601 | Time spans | `"PT1H30M"` |

## JSON Types

| Scalar | SQL Type | Structure | Use Case | Example |
|--------|----------|-----------|----------|---------|
| `JSON` | JSON | Any JSON | Flexible data | `{"key": "value"}` |
| `JSONB` | JSONB | Any JSON | Indexed JSON | `{"nested": {"data": "value"}}` |

## Binary Types

| Scalar | SQL Type | Encoding | Use Case | Example |
|--------|----------|----------|----------|---------|
| `Binary` | BYTEA | Base64 | File data | `"aGVsbG8gd29ybGQ="` |
| `Hash` | VARCHAR | Hex | Checksums | `"5d41402abc4b2a76b9719d911017c592"` |

## Boolean & Special

| Scalar | SQL Type | Values | Use Case | Example |
|--------|----------|--------|----------|---------|
| `Boolean` | BOOLEAN | true/false | Flags | `true` |
| `Void` | N/A | null | No value | `null` |

---

## Type Mappings by Database

### PostgreSQL

```text
String       → VARCHAR
Int          → INTEGER
Float        → DOUBLE PRECISION
DateTime     → TIMESTAMP WITH TIME ZONE
Date         → DATE
Boolean      → BOOLEAN
JSON         → JSONB
Binary       → BYTEA
Decimal      → NUMERIC
```text

### MySQL

```text
String       → VARCHAR(255)
Int          → INT
Float        → DOUBLE
DateTime     → TIMESTAMP
Date         → DATE
Boolean      → TINYINT(1)
JSON         → JSON
Binary       → BLOB
Decimal      → DECIMAL
```text

### SQLite

```text
String       → TEXT
Int          → INTEGER
Float        → REAL
DateTime     → TEXT (ISO 8601)
Date         → TEXT (YYYY-MM-DD)
Boolean      → INTEGER (0/1)
JSON         → TEXT
Binary       → BLOB
Decimal      → REAL
```text

### SQL Server

```text
String       → NVARCHAR(MAX)
Int          → INT
Float        → FLOAT
DateTime     → DATETIMEOFFSET
Date         → DATE
Boolean      → BIT
JSON         → NVARCHAR(MAX)
Binary       → VARBINARY(MAX)
Decimal      → NUMERIC
```text

---

## Size Limits

| Type | Max Size | Warning Level |
|------|----------|--------------|
| `String` | Depends on DB | >1MB is large |
| `Email` | 254 bytes | Don't exceed RFC spec |
| `Phone` | 20 bytes | International format |
| `Slug` | 255 bytes | URL safe |
| `JSON` | Depends on DB | >10MB is huge |
| `Binary` | Depends on DB | Prefer external storage >100MB |

---

## Schema Examples

### User Table

```python
from fraiseql import type, field

@type
class User:
    id: ID              # UUID primary key
    email: Email        # Email with validation
    name: String        # User's full name
    age: Int            # Years old
    created_at: DateTime # Account creation time
    is_active: Boolean  # Account status
    preferences: JSON   # User settings
```text

### Product Table

```python
@type
class Product:
    id: ID
    name: String
    price: Decimal      # Use Decimal for money!
    stock_count: Int
    description: String
    release_date: Date
    is_available: Boolean
    metadata: JSON      # Flexible data
```text

### Event Table

```python
@type
class Event:
    id: ID
    event_name: String
    timestamp: DateTime  # Use DateTime for events
    duration: Duration   # How long it lasted
    data: JSONB          # Event details
    created_at: DateTime
```text

---

## Query Examples

### Filtering

```graphql
# String
{ users(where: { name: { equals: "John" } }) }
{ users(where: { email: { contains: "@example.com" } }) }

# Numbers
{ products(where: { price: { greaterThan: 100 } }) }
{ users(where: { age: { between: 18, 65 } }) }

# Dates
{ orders(where: { created_at: { after: "2024-01-01" } }) }

# Boolean
{ users(where: { is_active: { equals: true } }) }
```text

### Sorting

```graphql
# Numbers
{ products(order_by: { price: DESC }) }

# Dates
{ events(order_by: { timestamp: ASC }) }

# Strings
{ users(order_by: { name: ASC }) }
```text

### Aggregation

```graphql
# Count
{ users_aggregate { count } }

# Sum (numbers only)
{ orders_aggregate { total_price_sum: price_sum } }

# Average (numbers only)
{ products_aggregate { avg_price: price_avg } }

# Min/Max
{ orders_aggregate { min_price: price_min, max_price: price_max } }
```text

---

## Common Mistakes

### ❌ Using Float for Money

```python
# WRONG
@type
class Order:
    total: Float  # Rounding errors!
```text

### ✅ Using Decimal for Money

```python
# RIGHT
@type
class Order:
    total: Decimal  # Exact precision
```text

---

### ❌ Using String for Boolean

```python
# WRONG
@type
class User:
    is_active: String  # "true" or "false"?
```text

### ✅ Using Boolean

```python
# RIGHT
@type
class User:
    is_active: Boolean  # true or false, unambiguous
```text

---

### ❌ DateTime Without Time Zone

```python
# WRONG (ambiguous)
created_at: DateTime  # Which timezone?
```text

### ✅ DateTime With Time Zone

```python
# RIGHT (unambiguous)
created_at: DateTime  # Always UTC, explicit
```text

---

## See Also

- **[WHERE Operators Cheatsheet](./where-operators-cheatsheet.md)** - Filtering syntax
- **[Configuration Parameters Cheatsheet](./configuration-parameters-cheatsheet.md)** - TOML settings
- **[CLI Commands Cheatsheet](./cli-commands-cheatsheet.md)** - Command-line reference
