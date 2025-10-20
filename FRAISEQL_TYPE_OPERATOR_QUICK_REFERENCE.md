# FraiseQL Custom Types & Operators - Quick Reference

## Custom Types Available

All in `/home/lionel/code/fraiseql/src/fraiseql/types/scalars/`:

```python
from fraiseql.types import (
    IpAddress,      # IPv4/IPv6 - PostgreSQL inet/cidr
    LTree,          # Hierarchical paths - PostgreSQL ltree
    DateRange,      # Date ranges - PostgreSQL daterange
    MacAddress,     # MAC addresses - PostgreSQL macaddr
    Port,           # Network ports (1-65535) - smallint
    CIDR,           # CIDR notation - cidr type
    Date,           # ISO 8601 dates - date
    DateTime,       # ISO 8601 timestamps - timestamp
    EmailAddress,   # Email validation - text
    Hostname,       # DNS hostnames - text
    UUID,           # UUIDs - uuid
    JSON,           # JSON objects - jsonb
)
```

## Filter Operators by Type

### IP Address (NetworkOperatorStrategy)
```python
# Basic
"eq", "neq", "in", "notin", "nin"

# Network operations
"inSubnet",     # IP is in CIDR subnet
"inRange",      # IP in range {"from": "...", "to": "..."}
"isPrivate",    # RFC 1918 private
"isPublic",     # Non-private
"isIPv4",       # IPv4 only
"isIPv6",       # IPv6 only

# Classification (RFC-based)
"isLoopback",       # 127.0.0.0/8, ::1
"isLinkLocal",      # 169.254.0.0/16, fe80::/10
"isMulticast",      # 224.0.0.0/4, ff00::/8
"isDocumentation",  # RFC 3849/5737
"isCarrierGrade",   # RFC 6598 (100.64.0.0/10)
```

### LTree Hierarchical Paths (LTreeOperatorStrategy)
```python
# Basic
"eq", "neq", "in", "notin"

# Hierarchical
"ancestor_of",     # path1 @> path2
"descendant_of",   # path1 <@ path2

# Pattern matching
"matches_lquery",      # path ~ lquery
"matches_ltxtquery"    # path ? ltxtquery

# RESTRICTED (throws error)
"contains", "startswith", "endswith"
```

### DateRange (DateRangeOperatorStrategy)
```python
# Basic
"eq", "neq", "in", "notin"

# Range relationships
"contains_date",   # range @> date
"overlaps",        # range1 && range2
"adjacent",        # range1 -|- range2
"strictly_left",   # range1 << range2
"strictly_right",  # range1 >> range2
"not_left",        # range1 &> range2
"not_right"        # range1 &< range2

# RESTRICTED (throws error)
"contains", "startswith", "endswith"
```

### MAC Address (MacAddressOperatorStrategy)
```python
# Supported
"eq", "neq", "in", "notin", "isnull"

# RESTRICTED (throws error)
"contains", "startswith", "endswith"
```

### Generic Types (ComparisonOperatorStrategy)
```python
"eq", "neq", "gt", "gte", "lt", "lte"
```

### String Operations (PatternMatchingStrategy)
```python
"matches",      # Regex pattern
"startswith",   # LIKE 'prefix%'
"contains",     # LIKE '%substr%'
"endswith"      # LIKE '%suffix'
```

### List Operations (ListOperatorStrategy)
```python
"in",   # Value in list
"notin" # Value not in list
```

### All Types
```python
"isnull"  # IS NULL / IS NOT NULL
```

## Type Detection Priority

1. **Explicit type hint** (from @fraise_type decorator)
2. **Field name patterns** (contains "ip_address", "mac", "ltree", "daterange", etc.)
3. **Value heuristics** (IP address patterns, MAC formats, LTree notation, DateRange format)
4. **Default to STRING**

## SQL Generation Examples

### IP Address Filter
```python
query = {
    "ipAddress": {"isPrivate": True}
}
# Generates:
# (data->>'ip_address')::inet <<= '10.0.0.0/8'::inet
# OR (data->>'ip_address')::inet <<= '172.16.0.0/12'::inet
# -- ... more private ranges
```

### LTree Filter
```python
query = {
    "path": {"ancestor_of": "departments.engineering"}
}
# Generates:
# (data->>'path')::ltree @> 'departments.engineering'::ltree
```

### DateRange Filter
```python
query = {
    "availability": {"overlaps": "[2024-01-01,2024-12-31]"}
}
# Generates:
# (data->>'availability')::daterange && '[2024-01-01,2024-12-31]'::daterange
```

### MAC Address Filter
```python
query = {
    "macAddress": {"eq": "00:11:22:33:44:55"}
}
# Generates:
# (data->>'mac_address')::macaddr = '00:11:22:33:44:55'::macaddr
```

## Implementation Files

### Core
- `src/fraiseql/types/scalars/` - Type definitions
- `src/fraiseql/types/__init__.py` - Type exports
- `src/fraiseql/types/fraise_type.py` - @fraise_type decorator

### Operators
- `src/fraiseql/sql/operator_strategies.py` - Strategy implementations (1458 lines)
- `src/fraiseql/sql/where_generator.py` - WHERE clause generation
- `src/fraiseql/sql/graphql_where_generator.py` - GraphQL filter types
- `src/fraiseql/sql/where/core/field_detection.py` - Type detection

### Integration
- `src/fraiseql/cqrs/repository.py` - Repository with filtering

### Tests
- `tests/unit/sql/where/test_*_operators_sql_building.py`
- `tests/integration/database/sql/test_*_filter_operations.py`
- `tests/unit/sql/test_all_operator_strategies_coverage.py`

## Key Patterns

### 1. Strategy Pattern
```python
# Strategies registered in order (specialized first):
1. NullOperatorStrategy
2. DateRangeOperatorStrategy
3. LTreeOperatorStrategy
4. MacAddressOperatorStrategy
5. NetworkOperatorStrategy
6. ComparisonOperatorStrategy      # Generic fallback
7. PatternMatchingStrategy
8. JsonOperatorStrategy
9. ListOperatorStrategy
10. PathOperatorStrategy
```

### 2. Scalar Marker Pattern
```python
class CustomField(str, ScalarMarker):
    """Python marker for GraphQL scalar type."""
    __slots__ = ()
```

### 3. JSONB Path Pattern
```python
# Field accessed as: (data->>'field_name')
# Type cast as: (data->>'field_name')::postgresql_type
```

### 4. Type Casting
```python
# When field_type available: Use it
# When field_type unavailable (production CQRS):
#   1. Detect from value
#   2. Detect from field name
#   3. Default to string
```

## Adding a New Type

1. Create scalar in `src/fraiseql/types/scalars/my_type.py`
2. Export from `src/fraiseql/types/__init__.py`
3. Create strategy class in `src/fraiseql/sql/operator_strategies.py`
4. Register strategy in `OperatorRegistry` (BEFORE `ComparisonOperatorStrategy`)
5. Create filter input type in `src/fraiseql/sql/graphql_where_generator.py`
6. Update `FieldType` enum in `src/fraiseql/sql/where/core/field_detection.py`
7. Add tests

## Common Gotchas

1. **Specialized strategies must come BEFORE generic ones** - Order in `OperatorRegistry` matters
2. **RESTRICTED OPERATORS** - Some types explicitly disallow certain operations (e.g., LTree excludes pattern matching)
3. **Type detection loss in production** - Field type hints may be unavailable; fallback to heuristics
4. **JSONB normalization** - PostgreSQL may normalize values (e.g., MAC addresses); avoid pattern matching on these
5. **Boolean JSONB handling** - JSONB stores booleans as "true"/"false" strings; compare text-to-text
6. **IPv6 zone identifiers** - IPv6 link-local addresses may include zone ID (e.g., `fe80::1%eth0`)
7. **LTree vs domain names** - Heuristics exclude common domain extensions to avoid false positives
