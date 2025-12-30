# ID Type

FraiseQL uses `ID` for all entity identifiers.

## Quick Start

```python
from fraiseql.types import ID

@fraiseql.type
class User:
    id: ID
    name: str
```

## PostgreSQL Mapping

`ID` maps to `UUID` in PostgreSQL:

```sql
CREATE TABLE tb_user (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL
);
```

## GraphQL Schema

```graphql
type User {
  id: ID!
  name: String!
}
```

## Why UUID?

FraiseQL makes the opinionated choice to use UUIDs for all IDs because:

- **Security**: No enumeration attacks
- **Distribution**: Generate IDs anywhere (client, server, offline)
- **Scalability**: No coordination needed
- **Modern**: Industry standard (Stripe, GitHub, Hasura)

## Best Practices

```python
from fraiseql.types import ID

@fraiseql.type
class User:
    id: ID  # Primary key

@fraiseql.type
class Post:
    id: ID          # Primary key
    author_id: ID   # Foreign key reference
```

## Implementation Details

### Type Hierarchy

- **Python**: `IDField` (type marker that inherits from `str` and `ScalarMarker`)
- **GraphQL**: `IDScalar` (GraphQLScalarType)
- **PostgreSQL**: `UUID` (native PostgreSQL type)

### Serialization

```python
# ID serializes UUIDs to strings
uuid_obj = uuid.uuid4()
id_string = IDScalar.serialize(uuid_obj)  # "550e8400-e29b-41d4-a716-446655440000"

# ID parses strings back to UUIDs
parsed_uuid = IDScalar.parse_value(id_string)  # uuid.UUID object
```

### GraphQL Introspection

When you query the schema, ID appears as a scalar:

```graphql
query {
  __type(name: "User") {
    fields {
      name
      type {
        name  # Returns "ID" for id field
        kind  # Returns "SCALAR"
      }
    }
  }
}
```

## Migration from UUID

If you have existing code using `UUID`, migration is straightforward:

**Before:**
```python
from uuid import UUID

@fraiseql.type
class User:
    id: UUID
```

**After:**
```python
from fraiseql.types import ID

@fraiseql.type
class User:
    id: ID
```

### Database Schema

No database changes needed - both map to PostgreSQL `UUID`:

```sql
-- This schema works with both UUID and ID
CREATE TABLE tb_user (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL
);
```

## FAQ

### Why ID instead of UUID?

`ID` is the GraphQL standard scalar for identifiers. Using `ID` makes FraiseQL schemas more idiomatic and future-proof.

### Can I still use UUID?

Yes! For backward compatibility, you can continue using `UUID`. However, `ID` is recommended for new code.

### What about integer IDs?

FraiseQL is opinionated about using UUIDs. If you need integer IDs, you can use `int` fields, but you'll lose the benefits of UUID-based identifiers.

### Does ID support other identifier types?

Currently, `ID` is backed by UUID. Support for other identifier types (ULID, KSUID, etc.) may be added in the future.

## Related

- [Scalars](scalars.md)
- [Trinity Pattern](../patterns/trinity.md)
- [Database Integration](database-integration.md)
