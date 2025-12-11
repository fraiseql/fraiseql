# FraiseQL Custom Datatypes and Filter Operators - Architecture Exploration

## Overview

FraiseQL implements a sophisticated type system for PostgreSQL-specific datatypes combined with a strategy-pattern-based filter operator system. This enables type-safe GraphQL queries with custom validators and specialized SQL operators for advanced PostgreSQL types.

---

## 1. Custom Type System Architecture

### 1.1 Type Definition Pattern

FraiseQL uses a **scalar marker pattern** where custom types are defined as:

```python
class FieldType(ScalarMarker):
    """Base class for all custom scalar types."""
    __slots__ = ()

    def __repr__(self) -> str:
        return "FieldType"
```

Types inherit from `ScalarMarker` (a marker class) and typically also inherit from a built-in type for storage:

```python
class IpAddressField(str, ScalarMarker):
    """Represents a validated IP address."""
    __slots__ = ()
```

### 1.2 Supported Custom Types

Located in: `/home/lionel/code/fraiseql/src/fraiseql/types/scalars/`

| Type | Location | Purpose | PostgreSQL Type |
|------|----------|---------|-----------------|
| **IpAddressField** | `ip_address.py` | IPv4/IPv6 validation | `inet` / `CIDR` |
| **LTreeField** | `ltree.py` | Hierarchical paths | `ltree` |
| **DateRangeField** | `daterange.py` | Range values | `daterange` |
| **MacAddressField** | `mac_address.py` | Hardware addresses | `macaddr` |
| **PortField** | `port.py` | Network ports (1-65535) | `smallint` |
| **CIDRField** | `cidr.py` | Network notation | `cidr` |
| **DateField** | `date.py` | ISO 8601 dates | `date` |
| **DateTimeField** | `datetime.py` | ISO 8601 timestamps | `timestamp` |
| **EmailAddressField** | `email_address.py` | Email validation | `text` |
| **HostnameField** | `hostname.py` | DNS hostnames | `text` |
| **UUIDField** | `uuid.py` | RFC 4122 UUIDs | `uuid` |
| **JSONField** | `json.py` | JSON objects | `jsonb` |

### 1.3 Type Definition Pattern Example

Each scalar type follows this pattern:

```python
# 1. GraphQL Scalar Type Definition
DateRangeScalar = GraphQLScalarType(
    name="DateRange",
    description="Date range values",
    serialize=serialize_date_range,      # Python -> JSON
    parse_value=parse_date_range_value,  # JSON -> Python
    parse_literal=parse_date_range_literal,  # GraphQL AST -> Python
)

# 2. Python Marker Class
class DateRangeField(str, ScalarMarker):
    """Python-side marker for the DateRange scalar."""
    __slots__ = ()

    def __repr__(self) -> str:
        return "DateRange"

# 3. Validation Functions
def serialize_date_range(value: Any) -> str:
    """Convert Python value to serializable form."""
    if isinstance(value, str):
        return value
    raise GraphQLError(f"Invalid value: {value!r}")

def parse_date_range_value(value: Any) -> str:
    """Convert JSON input to Python type."""
    if isinstance(value, str):
        # Validate format: [YYYY-MM-DD, YYYY-MM-DD] or (YYYY-MM-DD, YYYY-MM-DD)
        pattern = r"^[\[\(](\d{4}-\d{2}-\d{2}),\s*(\d{4}-\d{2}-\d{2})[\]\)]$"
        if not re.match(pattern, value):
            raise GraphQLError(f"Invalid format: {value}")
        return value
    raise GraphQLError(f"Expected string, got {type(value)}")

def parse_date_range_literal(ast: ValueNode, variables: dict[str, Any] | None = None) -> str:
    """Convert GraphQL AST literal to Python type."""
    if isinstance(ast, StringValueNode):
        return parse_date_range_value(ast.value)
    raise GraphQLError("Expected string literal")
```

### 1.4 Type Registration

Types are exported from `/home/lionel/code/fraiseql/src/fraiseql/types/__init__.py`:

```python
from .scalars.ip_address import IpAddressField as IpAddress
from .scalars.ltree import LTreeField as LTree
from .scalars.daterange import DateRangeField as DateRange
# ... etc
```

Available as both GraphQL types and Python type hints:

```python
from fraiseql.types import IpAddress, LTree, DateRange

@fraise_type(sql_source="network_devices")
@dataclass
class NetworkDevice:
    id: UUID
    ip_address: IpAddress           # Custom type hint
    path: LTree                      # Hierarchical path
    availability: DateRange          # Date range
```

---

## 2. Filter Operator System Architecture

### 2.1 Operator Strategy Pattern

FraiseQL uses the **Strategy Pattern** for operator implementations. Located in:
`/home/lionel/code/fraiseql/src/fraiseql/sql/operator_strategies.py`

**Base Protocol:**
```python
class OperatorStrategy(Protocol):
    def can_handle(self, op: str) -> bool:
        """Check if this strategy can handle the given operator."""

    def build_sql(
        self,
        path_sql: SQL,
        op: str,
        val: Any,
        field_type: type | None = None,
    ) -> Composed:
        """Build the SQL for this operator."""
```

### 2.2 Core Operator Strategies

| Strategy | Location | Operators | Purpose |
|----------|----------|-----------|---------|
| **NullOperatorStrategy** | L371 | `isnull` | NULL checks |
| **ComparisonOperatorStrategy** | L390 | `eq, neq, gt, gte, lt, lte` | Numeric/text comparison |
| **PatternMatchingStrategy** | L484 | `matches, startswith, contains, endswith` | String patterns (regex/LIKE) |
| **ListOperatorStrategy** | L524 | `in, notin` | Membership tests |
| **JsonOperatorStrategy** | L453 | `overlaps, strictly_contains` | JSONB operators |
| **PathOperatorStrategy** | L588 | `depth_eq, depth_gt, depth_lt, isdescendant` | Generic path queries |

### 2.3 Specialized Type Strategies

#### **NetworkOperatorStrategy** (L1004-1398)
For IP addresses with network-aware operators:

```python
# Basic operators
"eq", "neq", "in", "notin", "nin"

# Subnet/range operations
"inSubnet",     # IP is in CIDR subnet (<<= operator)
"inRange",      # IP is in range (>= and <=)

# Classification (RFC-based)
"isPrivate"     # RFC 1918 private addresses
"isPublic"      # Non-private addresses
"isIPv4"        # IPv4-specific (family() = 4)
"isIPv6"        # IPv6-specific (family() = 6)

# Enhanced classification (v0.6.1+)
"isLoopback"        # 127.0.0.0/8, ::1
"isLinkLocal"       # 169.254.0.0/16, fe80::/10
"isMulticast"       # 224.0.0.0/4, ff00::/8
"isDocumentation"   # RFC 3849/5737
"isCarrierGrade"    # RFC 6598 (100.64.0.0/10)
```

#### **LTreeOperatorStrategy** (L773-905)
For hierarchical paths:

```python
# Basic operators
"eq", "neq", "in", "notin"

# Hierarchical relationships
"ancestor_of"       # path1 @> path2 (ancestor contains descendant)
"descendant_of"     # path1 <@ path2 (descendant is contained)

# Pattern matching
"matches_lquery"    # path ~ lquery (wildcard patterns)
"matches_ltxtquery" # path ? ltxtquery (text queries)

# Restricted
"contains", "startswith", "endswith"  # THROWS ERROR - not valid for ltree
```

#### **DateRangeOperatorStrategy** (L613-771)
For PostgreSQL daterange type:

```python
# Basic operators
"eq", "neq", "in", "notin"

# Range relationships
"contains_date"     # range @> date
"overlaps"          # range1 && range2
"adjacent"          # range1 -|- range2
"strictly_left"     # range1 << range2
"strictly_right"    # range1 >> range2
"not_left"          # range1 &> range2
"not_right"         # range1 &< range2

# Restricted
"contains", "startswith", "endswith"  # THROWS ERROR - not valid for daterange
```

#### **MacAddressOperatorStrategy** (L907-1002)
For MAC addresses:

```python
# Supported operators
"eq", "neq", "in", "notin"
"isnull"

# Restricted - THROWS ERROR
"contains", "startswith", "endswith"  # Not supported due to macaddr normalization
```

### 2.4 Operator Registry

The `OperatorRegistry` (L1400-1458) coordinates strategy selection:

```python
class OperatorRegistry:
    def __init__(self) -> None:
        """Initialize with all available strategies in precedence order."""
        self.strategies: list[OperatorStrategy] = [
            NullOperatorStrategy(),
            DateRangeOperatorStrategy(),        # Must come BEFORE ComparisonOperatorStrategy
            LTreeOperatorStrategy(),            # Must come BEFORE ComparisonOperatorStrategy
            MacAddressOperatorStrategy(),       # Must come BEFORE ComparisonOperatorStrategy
            NetworkOperatorStrategy(),         # Must come BEFORE ComparisonOperatorStrategy
            ComparisonOperatorStrategy(),
            PatternMatchingStrategy(),
            JsonOperatorStrategy(),
            ListOperatorStrategy(),
            PathOperatorStrategy(),
        ]

    def get_strategy(self, op: str, field_type: type | None = None) -> OperatorStrategy:
        """Get the appropriate strategy for an operator."""
        # Tries specialized strategies first, then falls back to generic ones
```

**Key Insight:** Specialized type strategies must be registered BEFORE generic strategies. This allows type-specific strategies to intercept and validate operators for their types.

---

## 3. Type Casting and JSONB Handling

### 3.1 Type Casting Strategy

The `BaseOperatorStrategy._apply_type_cast()` method (L54-126) handles PostgreSQL type casting:

```python
def _apply_type_cast(
    self, path_sql: SQL, val: Any, op: str, field_type: type | None = None
) -> SQL | Composed:
    """Apply appropriate type casting to the JSONB path."""

    # IP address types - special handling
    if field_type and is_ip_address_type(field_type) and op in ("eq", "neq", ...):
        return Composed([SQL("host("), path_sql, SQL("::inet)")])

    # MAC addresses - detect from value when field_type missing
    if looks_like_mac_address_value(val, op):
        return Composed([SQL("("), path_sql, SQL(")::macaddr")])

    # IP addresses - detect from value (production CQRS pattern)
    if looks_like_ip_address_value(val, op):
        return Composed([SQL("("), path_sql, SQL(")::inet")])

    # LTree paths - detect from value
    if looks_like_ltree_value(val, op):
        return Composed([SQL("("), path_sql, SQL(")::ltree")])

    # DateRange values - detect from value
    if looks_like_daterange_value(val, op):
        return Composed([SQL("("), path_sql, SQL(")::daterange")])

    # Numeric values
    if isinstance(val, (int, float, Decimal)):
        return Composed([SQL("("), path_sql, SQL(")::numeric")])

    # Datetime values
    if isinstance(val, datetime):
        return Composed([SQL("("), path_sql, SQL(")::timestamp")])
```

**Critical:** When `field_type` is not provided (common in production CQRS patterns), the system falls back to **value heuristics** to detect types.

### 3.2 Production-Mode Type Detection

When field type information is lost (production CQRS queries), FraiseQL detects types from values:

#### IP Address Detection:
```python
def _looks_like_ip_address_value(self, val: Any, op: str) -> bool:
    """Detect IP addresses (fallback when field_type missing)."""
    if isinstance(val, str):
        try:
            ipaddress.ip_address(val)      # Try parse
            return True
        except ValueError:
            try:
                ipaddress.ip_network(val, strict=False)  # Try CIDR
                return True
            except ValueError:
                pass

        # Heuristic: IPv4-like pattern
        if val.count(".") == 3 and all(0 <= int(p) <= 255 for p in val.split(".")):
            return True

        # Heuristic: IPv6-like pattern (contains hex + colons)
        if ":" in val and val.count(":") >= 2:
            return all(c in "0123456789abcdefABCDEF:" for c in val)

    return False
```

#### MAC Address Detection:
```python
def _looks_like_mac_address_value(self, val: Any, op: str) -> bool:
    """Detect MAC addresses."""
    mac_clean = val.replace(":", "").replace("-", "").replace(" ", "").upper()

    # MAC is exactly 12 hex characters
    if len(mac_clean) == 12 and all(c in "0123456789ABCDEF" for c in mac_clean):
        return True

    return False
```

#### LTree Detection:
```python
def _looks_like_ltree_value(self, val: Any, op: str) -> bool:
    """Detect LTree hierarchical paths."""
    # Pattern: dots separating alphanumeric/underscore/hyphen segments
    # Exclude: domain names, IP addresses, .local domains

    if not (val.startswith(("[", "(")) and val.endswith(("]", ")"))):
        return False

    # Check: at least one dot, no consecutive dots, valid chars
    ltree_pattern = r"^[a-zA-Z0-9_-]+(\.[a-zA-Z0-9_-]+)+$"

    # Avoid false positives: domain extensions, .local, IP-like patterns
    last_part = val.split(".")[-1].lower()
    if last_part in {"com", "net", "org", "local", "dev", "app", ...}:
        return False

    return bool(re.match(ltree_pattern, val))
```

#### DateRange Detection:
```python
def _looks_like_daterange_value(self, val: Any, op: str) -> bool:
    """Detect PostgreSQL daterange format."""
    # Pattern: [2024-01-01,2024-12-31] or (2024-01-01,2024-12-31)

    pattern = r"^\[?\(?(\d{4}-\d{2}-\d{2}),\s*(\d{4}-\d{2}-\d{2})\)?\]?$"

    return bool(re.match(pattern, val))
```

---

## 4. WHERE Clause Generation

### 4.1 WHERE Generator Architecture

Located in: `/home/lionel/code/fraiseql/src/fraiseql/sql/where_generator.py`

```python
def safe_create_where_type(cls: type[object]) -> type[DynamicType]:
    """Create a WHERE clause type for a FraiseQL type.

    Generates a dataclass with:
    - Fields for each type attribute
    - A `to_sql()` method returning parameterized SQL (psycopg Composed)
    """
```

### 4.2 Filter Input Types

Located in: `/home/lionel/code/fraiseql/src/fraiseql/sql/graphql_where_generator.py`

**Generic Filters:**
```python
@fraise_input
class StringFilter:
    eq: str | None = None
    neq: str | None = None
    contains: str | None = None
    startswith: str | None = None
    endswith: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None
```

**Restricted Filters for Complex Types:**

```python
@fraise_input
class NetworkAddressFilter:
    """Enhanced filter for IP addresses - EXCLUDES pattern matching operators."""
    # Basic operations
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Network-specific operations
    inSubnet: str | None = None        # IP is in CIDR subnet
    inRange: IPRange | None = None     # IP is in range
    isPrivate: bool | None = None      # RFC 1918 private
    isPublic: bool | None = None       # Non-private
    isIPv4: bool | None = None         # IPv4-specific
    isIPv6: bool | None = None         # IPv6-specific
    isLoopback: bool | None = None
    isLinkLocal: bool | None = None
    isMulticast: bool | None = None
    isDocumentation: bool | None = None
    isCarrierGrade: bool | None = None
    # NOTE: contains, startswith, endswith are INTENTIONALLY EXCLUDED
```

### 4.3 Field Type Detection

Located in: `/home/lionel/code/fraiseql/src/fraiseql/sql/where/core/field_detection.py`

```python
class FieldType(Enum):
    """Enumeration of field types for where clause generation."""
    ANY = "any"
    STRING = "string"
    INTEGER = "integer"
    IP_ADDRESS = "ip_address"
    MAC_ADDRESS = "mac_address"
    LTREE = "ltree"
    DATE_RANGE = "date_range"
    # ... more types

def detect_field_type(field_name: str, value: Any, field_type: type | None = None) -> FieldType:
    """Detect the type of field based on:
    1. Explicit type hint
    2. Field name patterns (e.g., "ip_address", "mac_address")
    3. Value analysis (heuristics)
    """
```

---

## 5. Integration: Repository to SQL

### 5.1 CQRS Repository Pattern

Located in: `/home/lionel/code/fraiseql/src/fraiseql/cqrs/repository.py`

```python
async def query(
    self,
    view_name: str,
    filters: dict[str, Any] | None = None,
    order_by: str | None = None,
    limit: int = 20,
    offset: int = 0,
) -> list[dict[str, Any]]:
    """Query entities with filtering.

    Converts GraphQL-style filters to SQL WHERE clauses:
    {
        "ip_address": {"isPrivate": True},
        "path": {"ancestor_of": "departments.engineering"}
    }
    """
    query_parts = [SQL("SELECT data FROM {} WHERE 1=1").format(SQL(view_name))]

    if filters:
        for key, value in filters.items():
            if isinstance(value, dict):
                # Map GraphQL field names to operator names
                # e.g., "nin" -> "notin"
                mapped_value = {}
                for op, val in value.items():
                    if op == "nin":
                        mapped_value["notin"] = val
                    else:
                        mapped_value[op] = val

                # Generate WHERE condition using operator strategies
                where_condition = _make_filter_field_composed(key, mapped_value, "data", None)
                if where_condition:
                    query_parts.append(SQL(" AND "))
                    query_parts.append(where_condition)

    return await cursor.execute(Composed(query_parts))
```

### 5.2 SQL Generation Example

For query:
```python
{
    "ipAddress": {"isPrivate": True},
    "path": {"ancestor_of": "departments.engineering"},
    "macAddress": {"eq": "00:11:22:33:44:55"}
}
```

Generates:
```sql
SELECT data FROM network_devices WHERE 1=1
  AND (data->>'ip_address')::inet <<= '10.0.0.0/8'::inet
  OR (data->>'ip_address')::inet <<= '172.16.0.0/12'::inet
  -- ... additional private ranges
  AND (data->>'path')::ltree @> 'departments.engineering'::ltree
  AND (data->>'mac_address')::macaddr = '00:11:22:33:44:55'::macaddr
```

---

## 6. Test Patterns

### 6.1 Operator Strategy Tests

Located in: `/home/lionel/code/fraiseql/tests/unit/sql/where/test_*_operators_sql_building.py`

Pattern:
```python
def test_ltree_ancestor_of_operation(self):
    """Test LTree ancestor_of operation (@>)."""
    registry = get_operator_registry()
    path_sql = SQL("data->>'path'")

    sql = registry.build_sql(
        path_sql=path_sql,
        op="ancestor_of",
        val="departments.engineering.backend",
        field_type=LTree
    )

    sql_str = str(sql)
    assert "::ltree" in sql_str
    assert "@>" in sql_str
    assert "departments.engineering.backend" in sql_str
```

### 6.2 Integration Tests

Located in: `/home/lionel/code/fraiseql/tests/integration/database/sql/where/*/test_*_operations.py`

Test actual database execution with:
- End-to-end IP filtering
- LTree hierarchical queries
- DateRange range operations
- MAC address matching
- Network classification (isPrivate, isPublic, etc.)

### 6.3 Regression Tests

Located in: `/home/lionel/code/fraiseql/tests/regression/`

Tests ensure backward compatibility and fix verification for:
- IP address normalization in JSONB
- LTree path detection vs domain name false positives
- MAC address format normalization
- DateRange parsing edge cases

---

## 7. Key Design Patterns

### 7.1 Strategy Pattern
Each operator type has its own strategy class implementing:
- `can_handle(op, field_type)` - Determine applicability
- `build_sql(path_sql, op, val, field_type)` - Generate SQL

### 7.2 Scalar Marker Pattern
Custom types combine:
- A GraphQL `ScalarType` (serialization/validation)
- A Python marker class for type hints
- Validation functions (serialize, parse_value, parse_literal)

### 7.3 JSONB Path Pattern
- JSONB data stored as `data` column
- Fields accessed via JSONB operators: `data->>'field'`
- Type casting applied: `(data->>'field')::inet`

### 7.4 Fallback Type Detection
When field_type not available:
1. Detect from field name patterns
2. Detect from value heuristics
3. Default to STRING type

### 7.5 Operator Precedence
Specialized strategies registered BEFORE generic ones:
1. NullOperatorStrategy
2. DateRangeOperatorStrategy
3. LTreeOperatorStrategy
4. MacAddressOperatorStrategy
5. NetworkOperatorStrategy
6. ComparisonOperatorStrategy
7. PatternMatchingStrategy
8. JsonOperatorStrategy
9. ListOperatorStrategy
10. PathOperatorStrategy

This ensures type-specific validation before generic operations.

---

## 8. Implementation Checklist for Custom Types

To add a new custom type to FraiseQL:

### Step 1: Create Scalar Type
```python
# src/fraiseql/types/scalars/my_type.py

def serialize_my_type(value: Any) -> str:
    """Serialize to GraphQL output."""
    ...

def parse_my_type_value(value: Any) -> str:
    """Parse from GraphQL input."""
    ...

def parse_my_type_literal(ast: ValueNode, variables: dict | None = None) -> str:
    """Parse from GraphQL literal."""
    ...

MyTypeScalar = GraphQLScalarType(
    name="MyType",
    serialize=serialize_my_type,
    parse_value=parse_my_type_value,
    parse_literal=parse_my_type_literal,
)

class MyTypeField(str, ScalarMarker):
    __slots__ = ()
    def __repr__(self) -> str:
        return "MyType"
```

### Step 2: Export Type
```python
# src/fraiseql/types/__init__.py
from .scalars.my_type import MyTypeField as MyType
```

### Step 3: Create Operator Strategy (if specialized operators needed)
```python
# src/fraiseql/sql/operator_strategies.py

class MyTypeOperatorStrategy(BaseOperatorStrategy):
    def __init__(self) -> None:
        super().__init__([
            "eq", "neq", "in", "notin",  # Basic
            "my_special_op_1", "my_special_op_2"  # Custom
        ])

    def can_handle(self, op: str, field_type: type | None = None) -> bool:
        if op not in self.operators:
            return False

        # Only handle specialized ops without field_type
        if field_type is None:
            return op in {"my_special_op_1", "my_special_op_2"}

        # With field_type, handle all operators
        return self._is_my_type(field_type)

    def build_sql(self, path_sql: SQL, op: str, val: Any, field_type: type | None = None) -> Composed:
        # Implement custom SQL generation
        ...
```

### Step 4: Register Strategy
```python
# In OperatorRegistry.__init__()
self.strategies: list[OperatorStrategy] = [
    # ... existing strategies ...
    MyTypeOperatorStrategy(),  # Add before ComparisonOperatorStrategy
    # ... remaining strategies ...
]
```

### Step 5: Create Filter Input Type
```python
# src/fraiseql/sql/graphql_where_generator.py

@fraise_input
class MyTypeFilter:
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    my_special_op_1: str | None = None
    my_special_op_2: str | None = None
    isnull: bool | None = None
```

### Step 6: Update Field Detection
```python
# src/fraiseql/sql/where/core/field_detection.py

class FieldType(Enum):
    MY_TYPE = "my_type"

@classmethod
def from_python_type(cls, python_type: type) -> "FieldType":
    try:
        from fraiseql.types.scalars.my_type import MyTypeField
        if python_type == MyTypeField or issubclass(python_type, MyTypeField):
            return cls.MY_TYPE
    except ImportError:
        pass
```

### Step 7: Add Tests
```python
# tests/unit/sql/where/test_my_type_operators_sql_building.py
# tests/integration/database/sql/where/{category}/test_my_type_operations.py
```

---

## 9. File Reference Summary

### Core Type System
- `/home/lionel/code/fraiseql/src/fraiseql/types/fraise_type.py` - @fraise_type decorator
- `/home/lionel/code/fraiseql/src/fraiseql/types/scalars/` - All custom scalar implementations
- `/home/lionel/code/fraiseql/src/fraiseql/types/__init__.py` - Type exports

### Filter Operators
- `/home/lionel/code/fraiseql/src/fraiseql/sql/operator_strategies.py` - Strategy implementations (1458 lines)
- `/home/lionel/code/fraiseql/src/fraiseql/sql/where_generator.py` - WHERE clause generation
- `/home/lionel/code/fraiseql/src/fraiseql/sql/graphql_where_generator.py` - GraphQL filter input types
- `/home/lionel/code/fraiseql/src/fraiseql/sql/where/core/field_detection.py` - Type detection

### Repository Integration
- `/home/lionel/code/fraiseql/src/fraiseql/cqrs/repository.py` - CQRS repository with filtering

### Tests
- `/home/lionel/code/fraiseql/tests/unit/sql/where/test_*_operators_sql_building.py` - Operator unit tests
- `/home/lionel/code/fraiseql/tests/integration/database/sql/where/*/` - Integration tests (organized by operator category)
- `/home/lionel/code/fraiseql/tests/unit/sql/test_all_operator_strategies_coverage.py` - Strategy coverage tests

---

## 10. Production Considerations

### Type Information Loss
In production CQRS queries, field type hints are often unavailable. FraiseQL handles this through:

1. **Value heuristics** - Detect from data values
2. **Field name patterns** - Detect from field names (e.g., "ip_address")
3. **Operator specificity** - Network-specific operators (isPrivate) always indicate IP fields

### Performance Optimization
- Type casting is applied once when building SQL
- Parameterized queries prevent SQL injection
- Strategy pattern allows adding new types without modifying core WHERE generator
- Type detection is cached via `@functools.cache` decorators

### Edge Cases Handled
- MAC address format normalization (multiple formats supported)
- IP address CIDR notation handling
- LTree path vs domain name disambiguation
- DateRange bracket direction (inclusive/exclusive)
- IPv6 link-local zone identifiers
- Boolean JSONB text representation ("true"/"false" strings)
