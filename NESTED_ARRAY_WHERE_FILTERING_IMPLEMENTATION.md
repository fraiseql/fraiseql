# Nested Array Where Filtering Implementation - FraiseQL v0.7.10+

## Overview

This document describes the implementation of nested array where filtering in FraiseQL, a feature that builds on the v0.7.10 nested array resolution capabilities. The feature enables GraphQL queries to filter nested array elements based on their properties using WhereInput types.

## Implementation Strategy

The implementation followed a **Phased Micro TDD** approach with **RED-GREEN-REFACTOR** cycles:

### Phase 1: Core Infrastructure ✅
- Extended `fraise_field` to support where filtering parameters
- Added `supports_where_filtering`, `nested_where_type`, and `where_input_type` attributes
- Fixed `_convert_filter_to_dict` to handle plain Python dictionaries

### Phase 2: Enhanced Field Resolver ✅
- Created `create_nested_array_field_resolver_with_where` function
- Implemented filtering logic with support for all standard operators
- Added proper error handling and edge cases

### Phase 3: GraphQL Integration ✅
- Integrated enhanced resolver into FraiseQL's type system
- Added automatic WhereInput type generation
- Created GraphQL field arguments for where parameters

## Key Components

### 1. Enhanced FraiseQLField (`src/fraiseql/fields.py`)

```python
class FraiseQLField:
    # New attributes for nested array where filtering support
    where_input_type: type | None = None
    supports_where_filtering: bool = False
    nested_where_type: type | None = None
```

### 2. Enhanced Field Resolver (`src/fraiseql/core/nested_field_resolver.py`)

```python
def create_nested_array_field_resolver_with_where(
    field_name: str, field_type: Any, field_metadata: Any = None
):
    """Create a field resolver for nested arrays that supports where parameter filtering."""
```

### 3. GraphQL Type Integration (`src/fraiseql/core/graphql_type.py`)

Added logic to detect when fields have `supports_where_filtering=True` and create enhanced resolvers with GraphQL arguments.

### 4. Fixed Where Filter Conversion (`src/fraiseql/sql/graphql_where_generator.py`)

```python
def _convert_filter_to_dict(filter_obj: Any) -> dict[str, Any]:
    # Check if this is already a plain dict - return it directly
    if isinstance(filter_obj, dict):
        return filter_obj
```

## Usage Examples

### Basic Usage

```python
@fraise_type
class PrintServer:
    id: uuid.UUID
    hostname: str
    operating_system: str
    n_total_allocations: int = 0

@fraise_type(sql_source="tv_network_configuration", jsonb_column="data")
class NetworkConfiguration:
    id: uuid.UUID
    name: str
    # Enable where filtering on nested array
    print_servers: List[PrintServer] = fraise_field(
        default_factory=list,
        supports_where_filtering=True,
        nested_where_type=PrintServer
    )
```

### GraphQL Query Example

```graphql
query GetNetworkConfig($id: UUID!) {
  networkConfiguration(id: $id) {
    name
    printServers(where: {
      hostname: { contains: "prod" }
      operatingSystem: { in: ["Windows Server", "Linux"] }
      nTotalAllocations: { gte: 100 }
    }) {
      hostname
      operatingSystem
      nTotalAllocations
    }
  }
}
```

### Programmatic Filtering

```python
# Create where filter
PrintServerWhereInput = create_graphql_where_input(PrintServer)
where_filter = PrintServerWhereInput()
where_filter.hostname = {'contains': 'prod'}
where_filter.operating_system = {'in_': ['Windows Server', 'Linux']}

# Use with resolver
resolver = create_nested_array_field_resolver_with_where('print_servers', List[PrintServer])
filtered_results = await resolver(network_config, None, where=where_filter)
```

## Supported Filter Operations

The implementation supports all standard filter operations:

- **String Operations**: `eq`, `neq`, `contains`, `startswith`, `endswith`, `in_`, `nin`, `isnull`
- **Numeric Operations**: `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `in_`, `nin`, `isnull`
- **Boolean Operations**: `eq`, `neq`, `isnull`
- **UUID Operations**: `eq`, `neq`, `in_`, `nin`, `isnull`
- **Date/DateTime Operations**: `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `in_`, `nin`, `isnull`

## Test Coverage

Comprehensive test suite covering:

- ✅ **Unit Tests**: Individual component functionality
- ✅ **Integration Tests**: FraiseQL type system integration
- ✅ **End-to-End Tests**: Complete filtering scenarios
- ✅ **Edge Cases**: Empty arrays, null values, complex filters

### Test Files
- `tests/test_nested_array_where_filtering.py` - Basic functionality tests
- `tests/test_nested_array_where_integration.py` - Integration tests
- `tests/test_nested_array_resolver_where.py` - Resolver tests
- `tests/test_integration_where_field.py` - GraphQL integration tests

## Performance Characteristics

- **In-Memory Filtering**: Current implementation filters at the application level
- **Database Integration**: Future enhancement for JSONB path queries at PostgreSQL level
- **Caching**: Compatible with existing FraiseQL caching mechanisms
- **Scalability**: Efficient for typical nested array sizes

## Breaking Changes

**None** - The implementation is fully backward compatible:
- Existing fields without where filtering continue to work unchanged
- New parameters are optional with sensible defaults
- GraphQL schema remains compatible for non-filtered fields

## Future Enhancements

### Phase 4: Database-Level Filtering (Pending)
```sql
-- Future PostgreSQL JSONB path filtering
SELECT data->'printServers'
FROM tv_network_configuration
WHERE jsonb_path_exists(
  data->'printServers',
  '$[*] ? (@.operatingSystem == "Windows" && @.nTotalAllocations >= 100)'
)
```

### Phase 5: Advanced Features (Pending)
- Nested object filtering (not just arrays)
- Complex AND/OR logic support
- Performance optimization with database pushdown
- GraphQL subscription support for filtered arrays

## Files Modified

1. `src/fraiseql/fields.py` - Extended FraiseQLField class and fraise_field function
2. `src/fraiseql/core/nested_field_resolver.py` - Added enhanced resolver with filtering
3. `src/fraiseql/core/graphql_type.py` - Integrated enhanced resolver into type system
4. `src/fraiseql/sql/graphql_where_generator.py` - Fixed filter conversion for plain dicts

## Example Application

See `example_nested_array_where_filtering.py` for a complete working example demonstrating all functionality.

## Conclusion

The nested array where filtering feature successfully extends FraiseQL's capabilities while maintaining backward compatibility and following established patterns. The implementation provides a solid foundation for future enhancements and delivers significant value for GraphQL API developers working with nested data structures.

**Status**: ✅ **Feature Complete and Tested**
**Branch**: `feature/nested-array-where-filtering`
**FraiseQL Version**: v0.7.10+
