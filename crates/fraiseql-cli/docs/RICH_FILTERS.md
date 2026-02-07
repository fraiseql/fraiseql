# Rich Filters: Advanced Query Operators

FraiseQL's rich filter system provides specialized query operators for 49 semantic scalar types, enabling sophisticated filtering beyond basic string matching and numeric comparisons.

## Overview

Rich filters are **auto-generated GraphQL `WhereInput` types** for semantic scalar types like `EmailAddress`, `PhoneNumber`, `Coordinates`, etc. Each type provides both:

1. **Standard operators**: `eq`, `neq`, `contains`, `isnull`
2. **Type-specific operators**: Domain matching for emails, distance for coordinates, etc.

### Example Query

```graphql
query FindUsers {
  users(
    where: {
      email: {
        domainEq: "example.com"  # Rich email operator
      }
      location: {
        distanceWithin: {        # Rich geospatial operator
          latitude: 40.7128
          longitude: -74.0060
          radiusKm: 5
        }
      }
    }
  ) {
    id
    email
    location {
      latitude
      longitude
    }
  }
}
```

## Supported Rich Types (49 Total)

### Contact Information (3)
- **EmailAddress**: Domain validation, exact match
- **PhoneNumber**: Country code extraction, E.164 validation
- **URL**: Protocol matching, domain validation

### Financial (4)
- **CurrencyCode**: ISO 4217 codes
- **IBAN**: International bank account validation
- **ISIN**: International securities identification
- **BIC**: Bank identifier code validation

### Geographic (3)
- **CountryCode**: ISO 3166 country codes
- **Coordinates**: Geospatial queries (distance, bounding box)
- **PostalCode**: Postal code validation

### Temporal (3)
- **DateRange**: Range overlap, duration queries
- **Duration**: ISO 8601 duration parsing
- **TimeRange**: Time-of-day range validation

### Personal Identifiers (8)
- **SSID**: Social security identification
- **VIN**: Vehicle identification number
- **CUSIP**: Securities identifier
- **SEDOL**: Stock exchange daily official list
- **UUID**: Universally unique identifier
- **ISBN**: Book identification
- **ISSN**: Serial publication number
- **EAN**: European article numbering

### Cryptographic (3)
- **MD5Hash**: MD5 checksum validation
- **SHA1Hash**: SHA1 checksum validation
- **SHA256Hash**: SHA256 checksum validation

### Encoding (4)
- **Base64**: Base64-encoded data
- **Hex**: Hexadecimal data
- **JWT**: JSON Web Token
- **Base32**: Base32-encoded data

### Network (5)
- **IPv4Address**: IPv4 address validation
- **IPv6Address**: IPv6 address validation
- **MACAddress**: Media access control address
- **CIDR**: Classless inter-domain routing
- **ASN**: Autonomous system number

### Miscellaneous (9)
- **JSON**: JSON document validation
- **Slug**: URL-friendly slug format
- **Semantic Version**: Semantic versioning
- **ISO8601**: ISO 8601 date/time format
- **Color**: Hex color code validation
- **Latitude**: Latitude coordinate validation
- **Longitude**: Longitude coordinate validation
- **Timezone**: IANA timezone validation
- **LanguageCode**: ISO 639 language codes

## Type-Specific Operators

### EmailAddress Operators

| Operator | Input | SQL Pattern | Use Case |
|----------|-------|------------|----------|
| `eq` | String | `=` | Exact email match |
| `neq` | String | `!=` | Exclude email |
| `contains` | String | `LIKE %...%` | Partial email match |
| `domainEq` | String | Extract domain and compare | Find all users at domain |
| `domainIn` | [String] | Extract domain and match list | Multiple domains |
| `domainEndswith` | String | Domain suffix match | Company domain patterns |

**PostgreSQL Implementation**:
```sql
-- domainEq operator
WHERE SUBSTRING(email FROM POSITION('@' IN email) + 1) = $1

-- distanceWithin operator
WHERE ST_DWithin(
  location::geography,
  ST_Point($2, $1)::geography,
  $3 * 1000  -- convert km to meters
)
```

### Coordinates Operators (PostGIS)

| Operator | Input | Database Support | Use Case |
|----------|-------|------------------|----------|
| `distanceWithin` | {lat, lng, radiusKm} | PostgreSQL (native), MySQL, SQLite (approx), SQL Server | Location-based queries |
| `withinBoundingBox` | {minLat, maxLat, minLng, maxLng} | All databases | Rectangular region queries |
| `withinPolygon` | [[lat, lng], ...] | PostgreSQL (planned) | Custom polygon regions |

**SQLite Implementation** (Haversine approximation):
```sql
-- distanceWithin using Haversine formula
WHERE (
  6371 * 2 * ASIN(
    SQRT(
      SIN(RADIANS(($1 - latitude) / 2)) ^ 2 +
      COS(RADIANS(latitude)) * COS(RADIANS($1)) *
      SIN(RADIANS(($2 - longitude) / 2)) ^ 2
    )
  )
) < $3
```

### DateRange Operators

| Operator | Input | Use Case |
|----------|-------|----------|
| `durationGte` | days: Int | Ranges lasting at least N days |
| `startsAfter` | DateTime | Ranges starting after date |
| `endsBefore` | DateTime | Ranges ending before date |
| `overlaps` | {start, end} | Overlapping date ranges |

## Database Support Matrix

| Type | PostgreSQL | MySQL | SQLite | SQL Server |
|------|-----------|-------|--------|-----------|
| EmailAddress | ✅ Native | ✅ Native | ✅ Native | ✅ Native |
| PhoneNumber | ✅ Regex | ✅ REGEXP | ✅ GLOB | ✅ LIKE |
| Coordinates | ✅ PostGIS | ✅ ST_Distance | ⚠️ Approx | ✅ Geography |
| DateRange | ✅ Intervals | ✅ DATEDIFF | ✅ julianday | ✅ DATEDIFF |
| Duration | ✅ INTERVAL | ✅ Parse PT | ✅ Parse PT | ✅ Parse PT |

### PostGIS Requirements

PostGIS is required on PostgreSQL for geospatial queries:

```bash
# Enable PostGIS extension
CREATE EXTENSION IF NOT EXISTS postgis;

# Create index for performance
CREATE INDEX locations_idx ON users USING GIST (location);
```

## Performance Considerations

### Query Optimization

1. **Index Strategy**: Use GIST/BRIN indexes for geospatial queries
2. **Range Queries**: Index start/end fields separately, not JSON
3. **Domain Matching**: Use string prefix index for efficiency
4. **Lookup Data**: Cached in compiled schema, no database access

### Lookup Data

FraiseQL embeds reference data in `schema.compiled.json`:

```json
{
  "security": {
    "lookup_data": {
      "countries": {
        "US": { "continent": "North America", "in_eu": false, ... },
        "FR": { "continent": "Europe", "in_eu": true, ... }
      },
      "currencies": {
        "USD": { "symbol": "$", "decimal_places": 2, ... }
      },
      "timezones": { "America/New_York": {...} },
      "languages": { "en": {...}, "fr": {...} }
    }
  }
}
```

This approach ensures:
- ✅ No database queries for reference data
- ✅ Deterministic compilation
- ✅ Consistent validation across all instances
- ✅ Fast lookups in application layer

## Compilation Pipeline

### 1. Schema Parsing

FraiseQL parses intermediate schema:

```json
{
  "types": [
    {
      "name": "User",
      "fields": [
        {
          "name": "email",
          "type": "EmailAddress"
        }
      ]
    }
  ]
}
```

### 2. Auto-Generation

Rich types are **automatically generated** during compilation regardless of which types are used:

```
Intermediate Schema
    ↓
SchemaConverter::convert()
    ├─ Auto-generate 49 WhereInput types
    ├─ Generate SQL templates for each operator
    ├─ Embed lookup data
    └─> Compiled Schema
```

### 3. Code Generation

Each rich type generates a GraphQL input type:

```graphql
input EmailAddressWhereInput {
  eq: String
  neq: String
  contains: String
  isnull: Boolean
  domainEq: String
  domainIn: [String!]
  domainEndswith: String
}
```

### 4. SQL Template Embedding

SQL templates are embedded in the compiled schema for runtime execution:

```json
{
  "input_types": [
    {
      "name": "EmailAddressWhereInput",
      "fields": [...],
      "metadata": {
        "operators": {
          "domainEq": {
            "postgres": "SUBSTRING(email FROM POSITION('@' IN email) + 1) = $1",
            "mysql": "SUBSTRING_INDEX(email, '@', -1) = %s",
            "sqlite": "SUBSTR(email, INSTR(email, '@') + 1) = ?",
            "sqlserver": "SUBSTRING(email, CHARINDEX('@', email) + 1, LEN(email)) = @p1"
          }
        }
      }
    }
  ]
}
```

## Testing & Validation

### Unit Tests (181 tests)

- Individual operator validation
- Type parsing and generation
- SQL template correctness

### Integration Tests (6 tests)

- Complete compilation pipeline
- All 49 types generate properly
- SQL templates cover all databases
- Lookup data integrity
- Schema validity

### Benchmark Tests (6 tests)

- Compilation speed: ~205 µs
- Metadata access: <1 ns
- Template lookup: <32 ns
- Full traversal: <7 µs

### E2E Tests (6 tests)

- Complete compilation pipeline
- Multi-type schema handling
- Operator generation
- Deterministic output
- Data comprehensiveness

## API Reference

### SchemaConverter::convert()

```rust
pub fn convert(
    intermediate: IntermediateSchema
) -> Result<CompiledSchema> {
    // 1. Validate intermediate schema
    // 2. Convert to compiled schema
    // 3. Auto-generate 49 rich types
    // 4. Embed SQL templates
    // 5. Embed lookup data
}
```

### Rich Type Compilation

```rust
compile_rich_filters(&mut compiled, &RichFilterConfig::default());
```

Configuration options:

```rust
pub struct RichFilterConfig {
    pub enabled: bool,  // Enable rich filter compilation
    pub validation_overrides: HashMap<String, Value>,  // Custom rules
}
```

## Limitations & Future Work

### Current Limitations

- ✓ Phone validation limited to E.164 format (not carrier type)
- ✓ SQLite geospatial uses Haversine approximation (not exact)
- ✓ No polygon containment without spatial extension
- ✓ ISO 8601 duration parsing requires standard format

### Planned Enhancements

- [ ] Full phone validation via `phonenumber-rs`
- [ ] PostGIS polygon operators
- [ ] Advanced date range operations (gaps, unions)
- [ ] Time zone aware operations
- [ ] Route distance calculation

## Examples

### Email Domain Filtering

```graphql
query {
  users(where: { email: { domainEq: "company.com" } }) {
    id
    email
  }
}
```

### Geographic Proximity

```graphql
query {
  restaurants(
    where: {
      location: {
        distanceWithin: {
          latitude: 40.7128
          longitude: -74.0060
          radiusKm: 5
        }
      }
    }
  ) {
    name
    location { latitude longitude }
  }
}
```

### Date Range Queries

```graphql
query {
  projects(
    where: {
      timeline: {
        durationGte: 90
        overlaps: {
          start: "2024-06-01T00:00:00Z"
          end: "2024-08-31T23:59:59Z"
        }
      }
    }
  ) {
    name
  }
}
```

## References

- [GraphQL Specification §3.10 - Input Object Types](https://spec.graphql.org/June2018/#sec-Input-Object-Type-Validation)
- [PostGIS Documentation](https://postgis.net/documentation/)
- [ISO 8601 Duration Format](https://en.wikipedia.org/wiki/ISO_8601#Durations)
- [E.164 Phone Number Format](https://en.wikipedia.org/wiki/E.164)
