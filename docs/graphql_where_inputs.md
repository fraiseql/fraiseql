# GraphQL Where Input Types

FraiseQL provides automatic generation of GraphQL-compatible where input types that eliminate the need for manual conversion between GraphQL inputs and SQL where conditions.

## Overview

The `create_graphql_where_input()` function generates GraphQL input types with operator-based filtering that are automatically converted to SQL where conditions when used with `FraiseQLRepository`.

## Basic Usage

```python
from fraiseql import fraise_type, query
from fraiseql.sql import create_graphql_where_input, StringFilter, IntFilter

@fraise_type
class User:
    id: UUID
    name: str
    age: int
    is_active: bool

# Generate GraphQL where input type
UserWhereInput = create_graphql_where_input(User)

# Use in a query resolver
@query
async def users(info, where: UserWhereInput | None = None) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view", where=where)
```

## Operator Filter Types

FraiseQL provides operator filter types for all common data types:

### StringFilter
- `eq`: Exact match
- `neq`: Not equal
- `contains`: Contains substring
- `startswith`: Starts with
- `endswith`: Ends with
- `in`: In list of values
- `nin`: Not in list
- `isnull`: Is null check

### Numeric Filters (IntFilter, FloatFilter, DecimalFilter)
- `eq`, `neq`: Equality
- `gt`, `gte`: Greater than (or equal)
- `lt`, `lte`: Less than (or equal)
- `in`, `nin`: List membership
- `isnull`: Null check

### Other Filter Types
- `BooleanFilter`: `eq`, `neq`, `isnull`
- `UUIDFilter`: `eq`, `neq`, `in`, `nin`, `isnull`
- `DateFilter`, `DateTimeFilter`: All numeric operators plus list operations

## GraphQL Schema Example

The generated input types work seamlessly with GraphQL:

```graphql
query GetActiveUsers {
  users(where: {
    is_active: { eq: true }
    age: { gte: 18, lt: 65 }
    name: { contains: "John" }
  }) {
    id
    name
    age
  }
}
```

## Complex Filtering

Multiple operators can be combined on the same field:

```python
where = UserWhereInput(
    name=StringFilter(
        startswith="A",
        contains="dam",
        isnull=False
    ),
    age=IntFilter(
        gte=21,
        lte=65,
        neq=30
    )
)
```

## Automatic Conversion

When you pass a GraphQL where input to `FraiseQLRepository.find()` or `find_one()`, it's automatically converted to the SQL where type:

```python
# This GraphQL where input...
where_input = UserWhereInput(
    name=StringFilter(contains="John"),
    is_active=BooleanFilter(eq=True)
)

# ...is automatically converted to this SQL where type:
# UserWhere(name={"contains": "John"}, is_active={"eq": True})
```

## Custom Type Names

You can specify a custom name for the generated input type:

```python
FilterInput = create_graphql_where_input(User, name="UserFilterInput")
```

## Benefits

1. **No Manual Conversion**: Eliminate 300+ lines of boilerplate conversion code
2. **Type Safety**: Full type checking in Python and GraphQL
3. **Consistency**: Same operator names across all field types
4. **Extensibility**: Easy to add new operators or field types
5. **Performance**: Conversion happens once at the repository layer

## Migration from Manual Conversion

Before (manual conversion):
```python
# 40+ lines of manual mapping per resolver
where_conditions = {}
if where and where.get("name"):
    name_filter = where["name"]
    if name_filter.get("eq"):
        where_conditions["name"] = {"eq": name_filter["eq"]}
    if name_filter.get("contains"):
        where_conditions["name"]["contains"] = name_filter["contains"]
    # ... repeat for every operator
```

After (automatic):
```python
# Just use the generated type directly
@query
async def users(info, where: UserWhereInput | None = None) -> list[User]:
    return await info.context["db"].find("user_view", where=where)
```
