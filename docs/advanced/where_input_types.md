# Where Input Types & Advanced Filtering

FraiseQL provides automatic generation of GraphQL Where input types that enable powerful, type-safe filtering across your API. This feature transforms simple type definitions into comprehensive filtering interfaces.

## Overview

Where input types are automatically generated GraphQL input types that provide operator-based filtering for any `@fraise_type` decorated class. They support:

- **Type-safe filtering** - Generated from your type definitions
- **Rich operators** - Equality, comparison, string matching, arrays, etc.
- **Logical composition** - AND, OR, NOT operations
- **Nested filtering** - Filter on related object properties
- **Automatic SQL generation** - Converts GraphQL filters to SQL WHERE clauses

## Basic Usage

### 1. Define Your Type

```python
import fraiseql

@fraiseql.type(sql_source="users")
class User:
    id: UUID
    name: str
    email: str
    age: int
    is_active: bool
    tags: list[str]
    created_at: datetime
```

### 2. Generate Where Input Type

```python
from fraiseql.sql import create_graphql_where_input

# Automatically generate UserWhereInput type
UserWhereInput = create_graphql_where_input(User)
```

### 3. Use in Queries

```python
@fraiseql.query
async def users(info, where: UserWhereInput | None = None) -> list[User]:
    db = info.context["db"]
    return await db.find("users", where=where)
```

## Filter Operators by Field Type

> **💡 Advanced Operators**: FraiseQL provides comprehensive PostgreSQL operator support including arrays, full-text search, JSONB, and regex. See:
> - **[Filter Operators Reference](./filter-operators.md)** - Complete operator documentation with examples
> - **[Advanced Filtering Examples](../examples/advanced-filtering.md)** - Real-world use cases

### String Fields

```graphql
query {
  users(where: {
    name: { eq: "John" }
    email: { contains: "@company.com" }
    name: { startswith: "J" }
    name: { endswith: "son" }
    email: { in: ["john@example.com", "jane@example.com"] }
    name: { isnull: false }
  }) {
    id name email
  }
}
```

**Available operators:**
- `eq`, `neq` - equals, not equals
- `contains`, `startswith`, `endswith` - string pattern matching
- `in`, `nin` - list membership
- `isnull` - null checking

### Numeric Fields (int, float, Decimal)

```graphql
query {
  users(where: {
    age: { gt: 18, lte: 65 }
    age: { in: [25, 30, 35] }
    score: { gte: 85.5, lt: 100 }
  }) {
    id name age
  }
}
```

**Available operators:**
- `eq`, `neq` - equals, not equals
- `gt`, `gte`, `lt`, `lte` - comparisons
- `in`, `nin` - list membership
- `isnull` - null checking

### Boolean Fields

```graphql
query {
  users(where: {
    is_active: { eq: true }
    is_active: { neq: false }
  }) {
    id name is_active
  }
}
```

**Available operators:**
- `eq`, `neq` - equals, not equals
- `isnull` - null checking

### Date/DateTime Fields

```graphql
query {
  users(where: {
    created_at: { gt: "2023-01-01", lte: "2023-12-31" }
    created_at: { in: ["2023-01-01", "2023-06-01"] }
  }) {
    id name created_at
  }
}
```

**Available operators:**
- `eq`, `neq` - equals, not equals
- `gt`, `gte`, `lt`, `lte` - comparisons
- `in`, `nin` - list membership
- `isnull` - null checking

### Array/List Fields

```graphql
query {
  users(where: {
    tags: { contains: "admin" }  # Array contains this value
    tags: { in: ["developer", "manager"] }  # Array intersects with this list
  }) {
    id name tags
  }
}
```

**Basic operators:**
- `contains` - array contains this value
- `in` - array intersects with provided list
- `isnull` - null checking

**Advanced array operators** ([full documentation](./filter-operators.md#array-operators)):
- `eq`, `neq` - Array equality/inequality
- `overlaps` - Arrays share elements (automatically optimized for native/JSONB arrays)
- `contained_by` - Array is subset of provided values
- `len_eq`, `len_gt`, `len_gte`, `len_lt`, `len_lte` - Length comparisons
- `any_eq`, `all_eq` - Element-level matching

### UUID Fields

```graphql
query {
  users(where: {
    id: { eq: "550e8400-e29b-41d4-a716-446655440000" }
    id: { in: ["uuid1", "uuid2", "uuid3"] }
  }) {
    id name
  }
}
```

**Available operators:**
- `eq`, `neq` - equals, not equals
- `in`, `nin` - list membership
- `isnull` - null checking

## Logical Operators

### AND - All conditions must be true

```graphql
query {
  users(where: {
    AND: [
      { age: { gte: 18 } },
      { is_active: { eq: true } },
      { name: { contains: "Smith" } }
    ]
  }) {
    id name age is_active
  }
}
```

### OR - Any condition must be true

```graphql
query {
  users(where: {
    OR: [
      { role: { eq: "admin" } },
      { department: { eq: "engineering" } },
      { tags: { contains: "manager" } }
    ]
  }) {
    id name role department
  }
}
```

### NOT - Negate a condition

```graphql
query {
  users(where: {
    NOT: { is_active: { eq: false } }
  }) {
    id name is_active
  }
}
```

### Complex Nested Logic

```graphql
query {
  users(where: {
    AND: [
      { age: { gte: 21 } },
      {
        OR: [
          { department: { eq: "engineering" } },
          { role: { eq: "admin" } }
        ]
      },
      {
        NOT: { tags: { contains: "inactive" } }
      }
    ]
  }) {
    id name age department role tags
  }
}
```

## Nested Object Filtering

When your types have relationships, you can filter on nested object properties:

```python
@fraiseql.type(sql_source="posts")
class Post:
    id: UUID
    title: str
    author_id: UUID
    author: User  # Nested relationship

# Generate Where input for nested filtering
PostWhereInput = create_graphql_where_input(Post)
```

```graphql
query {
  posts(where: {
    author: {
      name: { contains: "John" }
      department: { eq: "engineering" }
    }
    title: { contains: "GraphQL" }
  }) {
    id title
    author {
      name department
    }
  }
}
```

## Advanced Filtering Examples

### Filtering on Array Elements

```graphql
# Find users with specific tags
query {
  users(where: {
    tags: { contains: "developer" }
  }) {
    id name tags
  }
}

# Find users with any of these tags
query {
  users(where: {
    OR: [
      { tags: { contains: "admin" } },
      { tags: { contains: "manager" } }
    ]
  }) {
    id name tags
  }
}
```

### Date Range Filtering

```graphql
query {
  posts(where: {
    created_at: {
      gte: "2023-01-01"
      lt: "2024-01-01"
    }
  }) {
    id title created_at
  }
}
```

### Complex Business Logic

```graphql
query {
  users(where: {
    AND: [
      { is_active: { eq: true } },
      { age: { gte: 18, lte: 65 } },
      {
        OR: [
          { department: { eq: "engineering" } },
          { role: { in: ["admin", "manager"] } }
        ]
      },
      {
        NOT: { tags: { contains: "suspended" } }
      }
    ]
  }) {
    id name age department role tags
  }
}
```

## Programmatic Usage

You can also create Where filters programmatically in your resolvers:

```python
@fraiseql.query
async def active_users_in_department(info, department: str) -> list[User]:
    db = info.context["db"]

    # Create filter programmatically
    where_filter = UserWhereInput(
        is_active={"eq": True},
        department={"eq": department}
    )

    return await db.find("users", where=where_filter)

@fraiseql.query
async def users_by_age_range(info, min_age: int, max_age: int) -> list[User]:
    db = info.context["db"]

    # Complex programmatic filter
    where_filter = UserWhereInput(
        AND=[
            UserWhereInput(age={"gte": min_age}),
            UserWhereInput(age={"lte": max_age}),
            UserWhereInput(is_active={"eq": True})
        ]
    )

    return await db.find("users", where=where_filter)
```

## Field-Level Filtering

Where input types can also be used for field resolvers to filter nested collections:

```python
@fraiseql.field
async def posts(user: User, info, where: PostWhereInput | None = None) -> list[Post]:
    """Get posts for a user with optional filtering."""
    db = info.context["db"]

    # Combine user filter with relationship constraint
    author_filter = PostWhereInput(author_id={"eq": user.id})
    if where:
        combined_where = PostWhereInput(AND=[author_filter, where])
    else:
        combined_where = author_filter

    return await db.find("posts", where=combined_where)
```

## Performance Considerations

- **Database indexes** - Ensure your database has appropriate indexes for filtered columns
- **Query optimization** - Where filters are converted to efficient SQL WHERE clauses
- **Pagination** - Combine with limit/offset for large result sets
- **Caching** - Consider caching for frequently filtered data

## Best Practices

1. **Use descriptive field names** - Make your filters self-documenting
2. **Validate input ranges** - Add constraints for performance
3. **Index filtered columns** - Database performance depends on proper indexing
4. **Combine with pagination** - Always paginate large result sets
5. **Test complex filters** - Verify SQL generation for complex AND/OR/NOT combinations

## Troubleshooting

### Common Issues

**"Field 'X' doesn't exist on WhereInput type"**
- Ensure the field exists on your base type
- Check for typos in field names

**"Operator 'X' not supported for field type"**
- Different field types support different operators
- Check the operator compatibility table above

**"Circular reference in Where input generation"**
- Avoid circular relationships in your type definitions
- Use forward references or restructure your types

**Performance issues with complex filters**
- Simplify your filter logic
- Add database indexes on filtered columns
- Consider pre-computed views for complex queries

## Migration from Manual Filtering

If you're migrating from manual query implementations:

```python
# Before: Manual filtering
@fraiseql.query
async def users_by_status(info, status: str) -> list[User]:
    db = info.context["db"]
    query = "SELECT * FROM users WHERE status = %s"
    result = await db.run(DatabaseQuery(query, [status]))
    return [User(**row) for row in result]

# After: Where input filtering
@fraiseql.query
async def users(info, where: UserWhereInput | None = None) -> list[User]:
    db = info.context["db"]
    return await db.find("users", where=where)

# Usage remains the same, but now supports complex filtering
query {
  users(where: { status: { eq: "active" } }) { id name status }
}
```

This approach provides much more flexibility while maintaining the same simple API surface.

## Advanced Filtering Capabilities

Beyond basic operators, FraiseQL provides comprehensive PostgreSQL operator support:

### Full-Text Search

Search text content with PostgreSQL's powerful full-text search:

```graphql
query {
  posts(where: {
    searchVector: {
      websearch_query: "python OR graphql",
      rank_gt: 0.1  # Filter by relevance score
    }
  }) {
    id
    title
  }
}
```

**Available operators**: `matches`, `plain_query`, `phrase_query`, `websearch_query`, `rank_gt`, `rank_gte`, `rank_lt`, `rank_lte`, `rank_cd_*`

**[See full documentation →](./filter-operators.md#full-text-search-operators)**

### JSONB Operators

Query JSON structure and content:

```graphql
query {
  products(where: {
    attributes: {
      has_key: "ram",
      contains: {brand: "Apple"}
    }
  }) {
    id
    name
  }
}
```

**Available operators**: `has_key`, `has_any_keys`, `has_all_keys`, `contains`, `contained_by`, `path_exists`, `path_match`, `get_path`, `get_path_text`

**[See full documentation →](./filter-operators.md#jsonb-operators)**

### Text Regex

Pattern matching with POSIX regular expressions:

```graphql
query {
  products(where: {
    sku: { matches: "^PROD-[0-9]{4}$" }
  }) {
    id
    sku
  }
}
```

**Available operators**: `matches`, `imatches`, `not_matches`

**[See full documentation →](./filter-operators.md#text-regex-operators)**

## Next Steps

- **[Filter Operators Reference](./filter-operators.md)** - Complete operator documentation
- **[Advanced Filtering Examples](../examples/advanced-filtering.md)** - Real-world use cases
- **[Nested Array Filtering](./nested-array-filtering.md)** - Complex array queries
