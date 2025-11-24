# Nested Array Where Filtering in FraiseQL v0.7.10+

## Overview

FraiseQL provides comprehensive nested array where filtering with complete AND/OR/NOT logical operator support. This feature enables sophisticated GraphQL queries to filter nested array elements based on their properties using intuitive WhereInput types.

## Features

- ✅ **Clean Registration-Based API** - No verbose field definitions required
- ✅ **Complete Logical Operators** - Full AND/OR/NOT support with unlimited nesting depth
- ✅ **All Field Operators** - equals, contains, gte, isnull, and more
- ✅ **Convention Over Configuration** - Automatic detection of filterable nested arrays
- ✅ **Performance Optimized** - Client-side filtering with efficient evaluation
- ✅ **Type Safe** - Full TypeScript/Python type safety with generated WhereInput types

## Quick Start

### 1. Clean Registration Approaches

```python
import fraiseql

from fraiseql.fields import fraise_field
from fraiseql.nested_array_filters import (
    auto_nested_array_filters,
    nested_array_filterable,
    register_nested_array_filter,
)
from fraiseql.types import fraise_type

@fraise_type
class PrintServer:
    id: UUID
    hostname: str
    ip_address: str | None = None
    operating_system: str
    n_total_allocations: int = 0

# Option 1: Automatic detection (recommended)
@auto_nested_array_filters
@fraise_type
class NetworkConfiguration:
    id: UUID
    name: str
    print_servers: list[PrintServer] = fraise_field(default_factory=list)

# Option 2: Selective fields
@nested_array_filterable("print_servers", "dns_servers")
@fraise_type
class NetworkConfiguration:
    id: UUID
    name: str
    print_servers: list[PrintServer] = fraise_field(default_factory=list)
    dns_servers: list[DnsServer] = fraise_field(default_factory=list)

# Option 3: Manual registration (maximum control)
@fraise_type
class NetworkConfiguration:
    id: UUID
    name: str
    print_servers: list[PrintServer] = fraise_field(default_factory=list)

register_nested_array_filter(NetworkConfiguration, "print_servers", PrintServer)
```

### 2. Generated GraphQL Schema

```graphql
type NetworkConfiguration {
  id: UUID!
  name: String!
  printServers(where: PrintServerWhereInput): [PrintServer!]!
}

input PrintServerWhereInput {
  # Field operators
  hostname: StringWhereInput
  ipAddress: StringWhereInput
  operatingSystem: StringWhereInput
  nTotalAllocations: IntWhereInput

  # Logical operators
  AND: [PrintServerWhereInput!]  # All conditions must be true
  OR: [PrintServerWhereInput!]   # Any condition can be true
  NOT: PrintServerWhereInput     # Invert condition result
}

input StringWhereInput {
  equals: String
  not: String
  in: [String!]
  notIn: [String!]
  contains: String
  startsWith: String
  endsWith: String
  isnull: Boolean
}

input IntWhereInput {
  equals: Int
  not: Int
  in: [Int!]
  notIn: [Int!]
  lt: Int
  lte: Int
  gt: Int
  gte: Int
  isnull: Boolean
}
```

## Query Examples

### Simple Field Filtering (Implicit AND)

```graphql
query {
  networkConfiguration(id: "some-uuid") {
    printServers(where: {
      operatingSystem: { equals: "Linux" }
      nTotalAllocations: { gte: 50 }
      ipAddress: { isnull: false }
    }) {
      hostname
      operatingSystem
      nTotalAllocations
      ipAddress
    }
  }
}
```

### Explicit AND Operator

```graphql
query {
  networkConfiguration(id: "some-uuid") {
    printServers(where: {
      AND: [
        { operatingSystem: { equals: "Windows Server" } }
        { nTotalAllocations: { gte: 100 } }
        { hostname: { contains: "prod" } }
      ]
    }) {
      hostname
      operatingSystem
      nTotalAllocations
    }
  }
}
```

### OR Operator

```graphql
query {
  networkConfiguration(id: "some-uuid") {
    printServers(where: {
      OR: [
        { operatingSystem: { equals: "Linux" } }
        { nTotalAllocations: { gte: 200 } }
      ]
    }) {
      hostname
      operatingSystem
      nTotalAllocations
    }
  }
}
```

### NOT Operator

```graphql
query {
  networkConfiguration(id: "some-uuid") {
    printServers(where: {
      NOT: {
        operatingSystem: { equals: "Windows Server" }
      }
    }) {
      hostname
      operatingSystem
    }
  }
}
```

### Complex Nested Logic

```graphql
query {
  networkConfiguration(id: "some-uuid") {
    printServers(where: {
      OR: [
        {
          # High-spec production servers
          AND: [
            { hostname: { contains: "prod" } }
            { nTotalAllocations: { gte: 100 } }
            { operatingSystem: { in: ["Windows Server", "Linux"] } }
          ]
        }
        {
          # Active development servers
          AND: [
            { hostname: { contains: "dev" } }
            { ipAddress: { isnull: false } }
            { NOT: { operatingSystem: { equals: "legacy" } } }
          ]
        }
      ]
    }) {
      hostname
      operatingSystem
      nTotalAllocations
      ipAddress
    }
  }
}
```

### Advanced Complex Example

```graphql
query {
  networkConfiguration(id: "some-uuid") {
    printServers(where: {
      AND: [
        {
          OR: [
            { operatingSystem: { equals: "Linux" } }
            { operatingSystem: { equals: "Windows Server" } }
          ]
        }
        {
          OR: [
            { nTotalAllocations: { gte: 50 } }
            { hostname: { contains: "critical" } }
          ]
        }
        {
          NOT: {
            AND: [
              { ipAddress: { isnull: true } }
              { operatingSystem: { equals: "legacy" } }
            ]
          }
        }
      ]
    }) {
      hostname
      operatingSystem
      nTotalAllocations
      ipAddress
    }
  }
}
```

## Field Operators Reference

### String Operators

| Operator | GraphQL Syntax | Description | Example |
|----------|----------------|-------------|---------|
| `equals` | `{ equals: "value" }` | Exact match | `hostname: { equals: "server-01" }` |
| `not` | `{ not: "value" }` | Not equal to | `hostname: { not: "localhost" }` |
| `in` | `{ in: ["val1", "val2"] }` | Matches any value in list | `operatingSystem: { in: ["Linux", "Windows"] }` |
| `notIn` | `{ notIn: ["val1", "val2"] }` | Does not match any value | `hostname: { notIn: ["test", "temp"] }` |
| `contains` | `{ contains: "substring" }` | Contains substring | `hostname: { contains: "prod" }` |
| `startsWith` | `{ startsWith: "prefix" }` | Starts with prefix | `hostname: { startsWith: "web-" }` |
| `endsWith` | `{ endsWith: "suffix" }` | Ends with suffix | `hostname: { endsWith: "-01" }` |
| `isnull` | `{ isnull: true/false }` | Is null or not null | `ipAddress: { isnull: false }` |

### Numeric Operators

| Operator | GraphQL Syntax | Description | Example |
|----------|----------------|-------------|---------|
| `equals` | `{ equals: 42 }` | Exact match | `nTotalAllocations: { equals: 100 }` |
| `not` | `{ not: 42 }` | Not equal to | `nTotalAllocations: { not: 0 }` |
| `gt` | `{ gt: 42 }` | Greater than | `nTotalAllocations: { gt: 50 }` |
| `gte` | `{ gte: 42 }` | Greater than or equal | `nTotalAllocations: { gte: 100 }` |
| `lt` | `{ lt: 42 }` | Less than | `nTotalAllocations: { lt: 200 }` |
| `lte` | `{ lte: 42 }` | Less than or equal | `nTotalAllocations: { lte: 150 }` |
| `in` | `{ in: [10, 20, 30] }` | Matches any value in list | `nTotalAllocations: { in: [50, 100, 150] }` |
| `notIn` | `{ notIn: [10, 20] }` | Does not match any value | `nTotalAllocations: { notIn: [0] }` |
| `isnull` | `{ isnull: true/false }` | Is null or not null | `nTotalAllocations: { isnull: false }` |

## Logical Operators

### AND Operator

**Behavior**: All conditions must be true
**Syntax**: `{ AND: [condition1, condition2, ...] }`
**Empty Array**: Returns all items (`[]` = match all)

```graphql
# All conditions must match
printServers(where: {
  AND: [
    { operatingSystem: { equals: "Linux" } }
    { nTotalAllocations: { gte: 50 } }
    { ipAddress: { isnull: false } }
  ]
})
```

**Implicit AND**: Multiple fields at the same level are automatically AND'ed:

```graphql
# These are equivalent
printServers(where: {
  operatingSystem: { equals: "Linux" }
  nTotalAllocations: { gte: 50 }
})

printServers(where: {
  AND: [
    { operatingSystem: { equals: "Linux" } }
    { nTotalAllocations: { gte: 50 } }
  ]
})
```

### OR Operator

**Behavior**: Any condition can be true
**Syntax**: `{ OR: [condition1, condition2, ...] }`
**Empty Array**: Returns no items (`[]` = match none)

```graphql
# Any condition can match
printServers(where: {
  OR: [
    { operatingSystem: { equals: "Linux" } }
    { nTotalAllocations: { gte: 200 } }
  ]
})
```

### NOT Operator

**Behavior**: Inverts the condition result
**Syntax**: `{ NOT: condition }`

```graphql
# Exclude Windows servers
printServers(where: {
  NOT: {
    operatingSystem: { equals: "Windows Server" }
  }
})

# Complex NOT with nested conditions
printServers(where: {
  NOT: {
    AND: [
      { operatingSystem: { equals: "legacy" } }
      { ipAddress: { isnull: true } }
    ]
  }
})
```

## Advanced Usage

### Python Resolver Implementation

```python
import fraiseql

from fraiseql.core.nested_field_resolver import create_nested_array_field_resolver_with_where
from fraiseql.sql.graphql_where_generator import create_graphql_where_input

# Create WhereInput type
PrintServerWhereInput = create_graphql_where_input(PrintServer)

# Create resolver with where filtering support
resolver = create_nested_array_field_resolver_with_where("print_servers", list[PrintServer])

# Use in GraphQL resolvers
@fraiseql.query
async def network_configuration_print_servers(
    parent: NetworkConfiguration,
    info: GraphQLResolveInfo,
    where: PrintServerWhereInput | None = None
) -> list[PrintServer]:
    return await resolver(parent, info, where=where)
```

### Custom Resolver Logic

```python
async def test_complex_filtering():
    # Create complex filter conditions
    windows_condition = PrintServerWhereInput()
    windows_condition.operating_system = {"equals": "Windows Server"}
    windows_condition.nTotalAllocations = {"gte": 100}

    linux_condition = PrintServerWhereInput()
    linux_condition.operating_system = {"equals": "Linux"}
    linux_condition.ipAddress = {"isnull": False}

    # Combine with OR
    where_filter = PrintServerWhereInput()
    where_filter.OR = [windows_condition, linux_condition]

    # Execute filtering
    result = await resolver(network_config, None, where=where_filter)

    # Process results
    for server in result:
        print(f"Found: {server.hostname} ({server.operating_system})")
```

## Performance Considerations

### Client-Side Filtering

Nested array filtering is performed **client-side** in memory, not at the database level:

```python
# Filtering happens after data is loaded
async def _apply_where_filter_to_array(items: list, where_filter: Any) -> list:
    """Apply where filtering to an array of items."""
    filtered_items = []
    for item in items:  # ← Iterates through each item in memory
        if await _item_matches_where_criteria(item, where_filter):
            filtered_items.append(item)
    return filtered_items
```

### Performance Characteristics

- **Best for**: Small to medium arrays (< 1000 items)
- **Response Time**: Sub-millisecond for simple conditions on small datasets
- **Complex Queries**: < 0.1 seconds for deeply nested conditions on moderate datasets
- **Memory Usage**: Minimal overhead, processes one item at a time

### Optimization Tips

1. **Use specific filters early**: More restrictive conditions first
2. **Combine with database filtering**: Filter at database level first, then use nested array filtering for refinement
3. **Consider materialized views**: For frequently accessed filtered data
4. **Monitor performance**: Use performance testing for complex nested conditions

## Troubleshooting

### Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| Filter not working | Field not registered | Use `@auto_nested_array_filters` or manual registration |
| Empty results | Wrong field names | Check generated WhereInput field names (camelCase) |
| Type errors | Incorrect operator | Use correct operators for field types |
| Complex query slow | Too many items | Consider database-level pre-filtering |

### Debug Tips

```python
# Check registered filters
from fraiseql.nested_array_filters import list_registered_filters
filters = list_registered_filters()
print("Registered filters:", filters)

# Verify WhereInput structure
PrintServerWhereInput = create_graphql_where_input(PrintServer)
where_input = PrintServerWhereInput()
print("Available fields:", dir(where_input))
```

## Migration Guide

### From Verbose Field Definitions

**Before (Verbose):**
```python
print_servers: list[PrintServer] = fraise_field(
    default_factory=list,
    supports_where_filtering=True,
    nested_where_type=PrintServer
)
```

**After (Clean):**
```python
@auto_nested_array_filters  # Just add this decorator
@fraise_type
class NetworkConfiguration:
    print_servers: list[PrintServer] = fraise_field(default_factory=list)
```

### Backward Compatibility

The new registration-based API is **fully backward compatible**:
- Existing verbose field definitions continue to work
- Can mix verbose and clean approaches in the same codebase
- Registry takes precedence over field metadata when both are present

## API Reference

### Registry Functions

```python
# Automatic registration
enable_nested_array_filtering(parent_type: Type) -> None

# Manual registration
register_nested_array_filter(parent_type: Type, field_name: str, element_type: Type) -> None

# Query functions
get_nested_array_filter(parent_type: Type, field_name: str) -> Type | None
is_nested_array_filterable(parent_type: Type, field_name: str) -> bool
list_registered_filters() -> dict[str, dict[str, str]]

# Utility
clear_registry() -> None  # For testing
```

### Decorators

```python
# Automatic detection for all list[FraiseQLType] fields
@auto_nested_array_filters
class MyType: ...

# Selective registration for specific fields
@nested_array_filterable("field1", "field2")
class MyType: ...
```

### Resolver Functions

```python
# Create enhanced resolver with where support
create_nested_array_field_resolver_with_where(
    field_name: str,
    field_type: Any,
    field_metadata: Any = None
) -> AsyncResolver

# Generate WhereInput types
create_graphql_where_input(cls: type, name: str | None = None) -> type
```



## Testing

Comprehensive test suite covering all logical operator scenarios:

```bash
# Run all nested array filtering tests
python -m pytest tests/test_nested_array* -v

# Run specific logical operator tests
python -m pytest tests/test_nested_array_logical_operators.py -v

# Run registry tests
python -m pytest tests/test_nested_array_registry.py -v
```

Test coverage includes:
- 40+ test cases covering all functionality
- Complex nested logical operator combinations
- Edge cases (empty arrays, null values)
- Performance testing
- Registry functionality
- Backward compatibility

---

**FraiseQL Nested Array Where Filtering** provides powerful, intuitive filtering capabilities with clean, registration-based configuration. No more verbose field definitions—just simple decorators and comprehensive logical operator support for sophisticated GraphQL queries.
