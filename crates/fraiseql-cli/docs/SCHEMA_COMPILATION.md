# Schema Compilation Guide

The FraiseQL schema compiler transforms an intermediate schema definition into an optimized compiled schema ready for runtime execution.

## Compilation Pipeline

```
Input: schema.json           ┌─────────────────────────────┐
(intermediate format)   ──→  │  fraiseql-cli compile       │
                             │  schema.json                │
Input: fraiseql.toml    ──→  │  --output schema.compiled   │
(configuration)              └────────┬────────────────────┘
                                      │
                            Output: schema.compiled.json
                            (compiled format)
                                      │
                                      ↓
                            ┌─────────────────────────────┐
                            │  Load in Application        │
                            │  FragileQL Server           │
                            └─────────────────────────────┘
```

## Compilation Stages

### Stage 1: Parsing

The compiler reads the intermediate schema JSON and validates syntax:

```bash
$ fraiseql-cli compile schema.json --output schema.compiled.json
```

Validates:
- ✓ All required fields present
- ✓ Type names unique
- ✓ Field types are known GraphQL types
- ✓ No circular type references
- ✓ Query/mutation signatures valid

### Stage 2: Type Conversion

Converts intermediate type definitions to compiled format:

**Input (intermediate)**:
```json
{
  "types": [
    {
      "name": "User",
      "fields": [
        {
          "name": "id",
          "type": "ID"
        },
        {
          "name": "email",
          "type": "EmailAddress"
        }
      ]
    }
  ]
}
```

**Output (compiled)**:
```json
{
  "types": [
    {
      "name": "User",
      "fields": [
        {
          "name": "id",
          "field_type": "ID",
          "nullable": false
        },
        {
          "name": "email",
          "field_type": "EmailAddress",
          "nullable": false
        }
      ]
    }
  ]
}
```

### Stage 3: Rich Type Generation

**All 49 rich scalar types are auto-generated**, including their `WhereInput` types:

```
EmailAddress type → EmailAddressWhereInput (auto-generated)
  fields:
    - eq: String
    - neq: String
    - contains: String
    - domainEq: String
    - domainIn: [String]
    - domainEndswith: String
```

Automatically generated types:
1. Contact: EmailAddress, PhoneNumber, URL (3)
2. Financial: CurrencyCode, IBAN, ISIN, BIC (4)
3. Geographic: CountryCode, Coordinates, PostalCode (3)
4. Temporal: DateRange, Duration, TimeRange (3)
5. Personal IDs: SSID, VIN, CUSIP, SEDOL, UUID, ISBN, ISSN, EAN (8)
6. Cryptographic: MD5Hash, SHA1Hash, SHA256Hash (3)
7. Encoding: Base64, Hex, JWT, Base32 (4)
8. Network: IPv4, IPv6, MAC, CIDR, ASN (5)
9. Miscellaneous: JSON, Slug, SemanticVersion, ISO8601, Color, Latitude, Longitude, Timezone, LanguageCode (9)

### Stage 4: SQL Template Embedding

Each operator gets database-specific SQL templates:

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
          },
          "domainIn": {
            "postgres": "SUBSTRING(email FROM POSITION('@' IN email) + 1) = ANY($1::text[])",
            "mysql": "SUBSTRING_INDEX(email, '@', -1) IN (%s)",
            "sqlite": "SUBSTR(email, INSTR(email, '@') + 1) IN (%s)",
            "sqlserver": "SUBSTRING(email, CHARINDEX('@', email) + 1, LEN(email)) IN (%s)"
          }
        }
      }
    }
  ]
}
```

### Stage 5: Lookup Data Embedding

Reference data is embedded in the compiled schema:

```json
{
  "security": {
    "lookup_data": {
      "countries": {
        "US": {
          "name": "United States",
          "continent": "North America",
          "in_eu": false,
          "in_schengen": false,
          "currency": "USD"
        },
        "FR": {
          "name": "France",
          "continent": "Europe",
          "in_eu": true,
          "in_schengen": true,
          "currency": "EUR"
        }
      },
      "currencies": {
        "USD": {
          "name": "US Dollar",
          "symbol": "$",
          "decimal_places": 2,
          "countries": ["US"]
        },
        "EUR": {
          "name": "Euro",
          "symbol": "€",
          "decimal_places": 2,
          "countries": ["FR", "DE", "IT"]
        }
      },
      "timezones": {
        "America/New_York": {
          "name": "Eastern Time",
          "offset": -5,
          "countries": ["US"]
        }
      },
      "languages": {
        "en": {
          "name": "English",
          "native_name": "English",
          "countries": ["US", "GB"]
        }
      }
    }
  }
}
```

### Stage 6: Security Configuration

Security and operational settings are embedded:

```json
{
  "security": {
    "rate_limiting": {
      "enabled": true,
      "auth_start_max_requests": 100,
      "auth_start_window_secs": 60,
      "authenticated_max_requests": 1000,
      "authenticated_window_secs": 60
    },
    "audit_logging": {
      "enabled": true,
      "log_level": "info"
    },
    "error_sanitization": {
      "enabled": true,
      "hide_implementation_details": true
    },
    "field_encryption": {
      "enabled": false,
      "fields": {}
    }
  }
}
```

## Compiled Schema Structure

The complete compiled schema contains:

```json
{
  "version": "2.0.0",
  "types": [...],                    // User-defined types
  "enums": [...],                    // Enums
  "input_types": [...],              // WhereInput, SortInput, etc.
  "interfaces": [...],               // GraphQL interfaces
  "unions": [...],                   // GraphQL unions
  "queries": [...],                  // Query root fields
  "mutations": [...],                // Mutation root fields
  "subscriptions": [...],            // Subscription root fields
  "security": {
    "lookup_data": {...},            // Embedded reference data
    "rate_limiting": {...},          // Rate limiting config
    "audit_logging": {...},          // Audit logging config
    "error_sanitization": {...},     // Error handling config
    "field_encryption": {...},       // Field encryption config
    "state_encryption": {...}        // OAuth state encryption config
  }
}
```

## Command-Line Usage

### Basic Compilation

```bash
# Compile single schema
fraiseql-cli compile schema.json --output schema.compiled.json

# Check compilation without writing
fraiseql-cli compile schema.json --dry-run

# Verbose output
fraiseql-cli compile schema.json --output schema.compiled.json --verbose
```

### Multiple Files

```bash
# Compile multiple schemas
fraiseql-cli compile schema1.json schema2.json --output combined.compiled.json

# Merge directory
fraiseql-cli compile schemas/ --output schema.compiled.json
```

### Configuration

```bash
# Use configuration file
fraiseql-cli compile schema.json \
  --config fraiseql.toml \
  --output schema.compiled.json
```

## Configuration (fraiseql.toml)

```toml
[fraiseql]
version = "2.0.0"

[fraiseql.security.rate_limiting]
enabled = true
auth_start_max_requests = 100
auth_start_window_secs = 60
authenticated_max_requests = 1000
authenticated_window_secs = 60

[fraiseql.security.audit_logging]
enabled = true
log_level = "info"

[fraiseql.security.error_sanitization]
enabled = true
hide_implementation_details = true

[fraiseql.security.field_encryption]
enabled = false

# Override with environment variable in production:
# FRAISEQL_SECURITY_RATE_LIMITING_ENABLED=false
```

## Output Validation

The compiler validates the output schema:

```bash
# Validate compiled schema
fraiseql-cli validate schema.compiled.json

# Detailed validation report
fraiseql-cli validate schema.compiled.json --detailed

# Check for unused types
fraiseql-cli validate schema.compiled.json --check-unused
```

## Performance Characteristics

| Operation | Time | Notes |
|-----------|------|-------|
| Parse schema | < 1ms | JSON parsing |
| Convert types | < 10ms | Type conversion and validation |
| Generate rich types | 50-100ms | 49 types with operators |
| Generate SQL templates | 100-200ms | All databases for all operators |
| Embed lookup data | 50-100ms | Countries, currencies, etc. |
| Write compiled schema | < 5ms | JSON serialization |
| **Total compilation** | **~250-400ms** | End-to-end |

### Compilation Speed Benchmarks

```
Empty schema: 204.99 µs per iteration (1000 iterations)
Metadata access: 0.70 ns per lookup
Operator parsing: 4.31 ns per parse
Database template access: 30.39 ns per access
Lookup data access: 53.94 ns per lookup
Full operator traversal: 7.194 µs (188 operators)
```

## Error Handling

The compiler provides detailed error messages:

```bash
$ fraiseql-cli compile invalid.json
Error: Schema validation failed

  ✗ Type 'User' field 'email' has unknown type 'EmailAddressInvalid'
    Did you mean: EmailAddress?

  ✗ Query 'users' returns list but 'is_list' field is missing
    Location: queries[0]

  ✗ Circular reference detected: User → Post → User
    Path: types[0] → types[1] → types[0]

Total errors: 3
Run with --verbose for more details
```

## Schema Evolution

The compiler handles schema evolution:

### Adding New Types

```diff
{
  "types": [
    { "name": "User", ... },
+   { "name": "Post", ... }
  ]
}
```

Result: New `PostWhereInput` type is auto-generated.

### Modifying Existing Types

```diff
{
  "types": [
    {
      "name": "User",
      "fields": [
        { "name": "email", "type": "EmailAddress" },
+       { "name": "phone", "type": "PhoneNumber" }
      ]
    }
  ]
}
```

Result: `PhoneNumberWhereInput` is auto-generated.

### Removing Types

Types can be safely removed. Compiled schema is deterministically rebuilt each time.

## Best Practices

### 1. Version Control

Commit both:
- `schema.json` (intermediate - source of truth)
- `schema.compiled.json` (compiled - for deployment)

### 2. Pre-Deployment Validation

```bash
# Validate schema before deployment
fraiseql-cli validate schema.compiled.json

# Check for breaking changes
fraiseql-cli schema-diff old.compiled.json new.compiled.json
```

### 3. Environment-Specific Configuration

Use environment variables to override compiled settings:

```bash
# In fraiseql.toml
[fraiseql.security.rate_limiting]
enabled = true
authenticated_max_requests = 1000

# Override in production
FRAISEQL_SECURITY_RATE_LIMITING_ENABLED=false
FRAISEQL_SECURITY_RATE_LIMITING_AUTHENTICATED_MAX_REQUESTS=10000
```

### 4. Schema Monitoring

```bash
# Monitor schema size
ls -lh schema.compiled.json

# Count types
jq '.types | length' schema.compiled.json

# Find unused types
fraiseql-cli analyze schema.compiled.json --unused-types
```

## Testing Your Schema

### Unit Testing

The compiler is thoroughly tested:

```bash
# Run all compiler tests
cargo test -p fraiseql-cli

# Run specific test
cargo test -p fraiseql-cli test_rich_filter_compilation_pipeline

# Benchmark compilation
cargo test -p fraiseql-cli bench_compile_empty_schema_rich_types -- --nocapture
```

### Integration Testing

Test end-to-end compilation:

```bash
# Create test schema
echo '{"types": [], "queries": []}' > test.json

# Compile
fraiseql-cli compile test.json --output test.compiled.json

# Validate
fraiseql-cli validate test.compiled.json

# Inspect
jq '.input_types | length' test.compiled.json  # Should be 49
```

## Troubleshooting

### "Unknown type" Error

```
Error: Type 'User' field 'email' has unknown type 'EmailAddres'
```

Solution: Check spelling. All 49 rich types are pre-defined. Use exact case-sensitive names.

### "Circular reference" Error

```
Error: Circular reference detected: User → Post → User
```

Solution: Remove the circular dependency or refactor to use IDs instead.

### Compilation Timeout

If compilation takes > 1 second, check:
- Number of types (should be < 1000)
- Size of schema.json (should be < 10MB)
- Complexity of type definitions

## References

- [GraphQL Schema Language](https://graphql.org/learn/schema/)
- [FraiseQL Architecture](../ARCHITECTURE_PRINCIPLES.md)
- [Rich Filters Documentation](./RICH_FILTERS.md)
