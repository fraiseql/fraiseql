# Query Translation

FraiseQL's query translation engine converts GraphQL queries into efficient SQL that extracts only the requested fields from your database views.

## Case Convention

FraiseQL automatically handles case conversion between Python's snake_case and GraphQL's camelCase conventions:

1. **Write everything in snake_case** (Python and SQL)
2. **GraphQL API automatically uses camelCase**
3. **No manual conversion needed**

### How It Works

1. **Python models**: Use natural Python snake_case
   ```python
   @fraiseql.type
   class User:
       first_name: str
       last_name: str
       is_active: bool
       created_at: datetime
       avatar_url: Optional[str]
   ```

2. **Database views**: Use snake_case everywhere
   ```sql
   CREATE VIEW v_users AS
   SELECT jsonb_build_object(
       'id', id,
       'first_name', first_name,
       'last_name', last_name,
       'is_active', is_active,
       'created_at', created_at,
       'avatar_url', avatar_url
   ) as data
   FROM tb_users;
   ```

3. **GraphQL API**: Automatically exposes camelCase
   ```graphql
   query {
     users {
       firstName    # Auto-converted from first_name
       lastName     # Auto-converted from last_name
       isActive     # Auto-converted from is_active
       createdAt    # Auto-converted from created_at
       avatarUrl    # Auto-converted from avatar_url
     }
   }
   ```

4. **Generated SQL**: FraiseQL handles the conversion
   ```sql
   SELECT
       data->>'first_name' AS firstName,
       data->>'last_name' AS lastName,
       data->>'is_active' AS isActive,
       data->>'created_at' AS createdAt,
       data->>'avatar_url' AS avatarUrl
   FROM v_users
   ```

### Benefits

- **Consistency**: Use snake_case throughout your Python code
- **No Manual Conversion**: No need to convert cases in view definitions
- **GraphQL Best Practices**: API follows GraphQL camelCase conventions
- **Zero Runtime Overhead**: Conversion happens at query build time

## Translation Process

### 1. GraphQL Query

```graphql
query GetUser {
  user(id: 1) {
    id
    firstName
    lastName
    isActive
    posts {
      title
      isPublished
      createdAt
    }
  }
}
```

### 2. Field Path Extraction

FraiseQL extracts the requested fields and converts them:
- GraphQL field `firstName` → SQL path `first_name`
- GraphQL field `isActive` → SQL path `is_active`
- GraphQL field `createdAt` → SQL path `created_at`

### 3. Generated SQL

```sql
SELECT jsonb_build_object(
    'id', data->>'id',
    'firstName', data->>'first_name',
    'lastName', data->>'last_name',
    'isActive', data->>'is_active',
    'posts', data->'posts'
) AS result
FROM v_users
WHERE id = $1
```

## Field Selection

### Basic Fields

GraphQL camelCase fields automatically map to snake_case JSON paths:

```graphql
{
  user {
    firstName      # Maps to 'first_name'
    emailAddress   # Maps to 'email_address'
    phoneNumber    # Maps to 'phone_number'
  }
}
```

```sql
SELECT
    data->>'first_name' AS firstName,
    data->>'email_address' AS emailAddress,
    data->>'phone_number' AS phoneNumber
FROM v_users
```

### Nested Objects

Nested selections work the same way:

```graphql
{
  user {
    profile {
      dateOfBirth     # Maps to 'date_of_birth'
      preferredName   # Maps to 'preferred_name'
    }
  }
}
```

The view returns snake_case JSON, FraiseQL extracts the requested fields.

## Filter Translation

### Simple Filters

GraphQL camelCase arguments work with snake_case data:

```graphql
{
  users(isActive: true) {
    firstName
  }
}
```

```sql
SELECT data->>'first_name' AS firstName
FROM v_users
WHERE (data->>'is_active') = $1  -- Parameterized for security
```

### Complex Filters with WHERE Types

FraiseQL provides type-safe WHERE clause generation with complete SQL injection protection:

```python
from fraiseql.sql.where_generator import safe_create_where_type

@fraiseql.type
class User:
    id: int
    first_name: str
    email: str
    age: int
    is_active: bool
    created_at: datetime

# Generate a type-safe WHERE filter
UserWhere = safe_create_where_type(User)

# Use in queries with automatic parameterization
where = UserWhere(
    first_name={"eq": "John"},
    age={"gte": 18, "lt": 65},
    is_active={"eq": True},
    email={"in": ["john@example.com", "j.doe@example.com"]}
)
```

### Supported Filter Operators

All operators use parameterized queries for security:

| Operator | Description | Example |
|----------|-------------|---------|
| `eq` | Equals | `{"eq": "John"}` |
| `neq` | Not equals | `{"neq": "admin"}` |
| `gt` | Greater than | `{"gt": 21}` |
| `gte` | Greater than or equal | `{"gte": 18}` |
| `lt` | Less than | `{"lt": 65}` |
| `lte` | Less than or equal | `{"lte": 100}` |
| `in` | In list | `{"in": ["A", "B", "C"]}` |
| `notin` | Not in list | `{"notin": [1, 2, 3]}` |
| `contains` | JSONB contains | `{"contains": {"role": "admin"}}` |
| `overlaps` | JSONB overlaps | `{"overlaps": ["tag1", "tag2"]}` |
| `matches` | Regex match | `{"matches": "^[A-Z]"}` |
| `startswith` | String starts with | `{"startswith": "John"}` |
| `isnull` | Is null/not null | `{"isnull": True}` or `{"isnull": False}` |

### Security: Parameterized Queries

**All WHERE clauses use parameterized queries**, preventing SQL injection:

```python
# Even with malicious input, queries are safe
where = UserWhere(
    name={"eq": "'; DROP TABLE users; --"},  # SQL injection attempt
    email={"in": ["admin@example.com", "' OR '1'='1"]}
)

# Generated SQL uses proper parameterization:
# (data ->> 'name') = $1 AND (data ->> 'email') IN ($2, $3)
# Parameters: ["'; DROP TABLE users; --", "admin@example.com", "' OR '1'='1"]
```

The psycopg library handles all escaping and parameterization automatically.

### Complex Filter Example

```python
@fraiseql.input
class UserFilter:
    first_name_contains: Optional[str]
    created_after: Optional[datetime]
    is_active: Optional[bool]
    age_range: Optional[dict]  # {"gte": 18, "lte": 65}
```

In GraphQL, these become camelCase:

```graphql
{
  users(where: {
    firstNameContains: "John",
    createdAfter: "2024-01-01",
    isActive: true,
    ageRange: { gte: 18, lte: 65 }
  }) {
    id
    firstName
    lastName
  }
}
```

Generated SQL (with proper parameterization):
```sql
SELECT
    data->>'id' AS id,
    data->>'first_name' AS firstName,
    data->>'last_name' AS lastName
FROM v_users
WHERE
    data->>'first_name' LIKE $1  -- '%John%'
    AND (data->>'created_at')::timestamp > $2::timestamp
    AND (data->>'is_active') = $3  -- 'true' for JSONB text comparison
    AND (data->>'age')::int >= $4
    AND (data->>'age')::int <= $5
```

## Sorting

### Order By Fields

Sorting uses the same automatic conversion:

```graphql
{
  posts(orderBy: createdAt_DESC) {
    title
    publishedAt
  }
}
```

```sql
SELECT
    data->>'title' AS title,
    data->>'published_at' AS publishedAt
FROM v_posts
ORDER BY (data->>'created_at')::timestamp DESC
```

### Multiple Sort Fields

```graphql
{
  posts(orderBy: [isPublished_DESC, createdAt_DESC]) {
    title
  }
}
```

```sql
SELECT data->>'title' AS title
FROM v_posts
ORDER BY
    (data->>'is_published')::boolean DESC,
    (data->>'created_at')::timestamp DESC
```

## Special Cases

### Acronyms and Numbers

FraiseQL handles special cases intelligently:

- `api_key` → `apiKey` (not `aPIKey`)
- `oauth2_token` → `oauth2Token`
- `page_2_content` → `page2Content`

### Already CamelCase Fields

If a field is already in camelCase (not recommended), it won't be double-converted:

```python
@fraiseql.type
class LegacyType:
    userId: int  # Not recommended, but works
    user_name: str  # Preferred approach
```

## Production Mode Optimization

In production mode, FraiseQL:
1. Caches the field name conversions
2. Pre-compiles common queries
3. Bypasses GraphQL validation for known queries
4. Sends optimized SQL directly to PostgreSQL

The automatic case conversion has zero runtime overhead in production.

## Best Practices

1. **Always use snake_case** in Python and SQL
2. **Let FraiseQL handle conversion** to camelCase
3. **Be consistent** - don't mix naming conventions
4. **Document any exceptions** if you must use camelCase in Python

### Example: Complete User Type

```python
# Python model - all snake_case
@fraiseql.type
class User:
    id: UUID
    email: str
    first_name: str
    last_name: str
    date_of_birth: Optional[date]
    is_active: bool = True
    email_verified: bool = False
    created_at: datetime
    updated_at: datetime
    last_login_at: Optional[datetime]
```

```sql
-- Database view - all snake_case
CREATE VIEW v_users AS
SELECT jsonb_build_object(
    'id', id,
    'email', email,
    'first_name', first_name,
    'last_name', last_name,
    'date_of_birth', date_of_birth,
    'is_active', is_active,
    'email_verified', email_verified,
    'created_at', created_at,
    'updated_at', updated_at,
    'last_login_at', last_login_at
) as data
FROM tb_users;
```

```graphql
# GraphQL API - automatic camelCase
query {
  users {
    id
    email
    firstName
    lastName
    dateOfBirth
    isActive
    emailVerified
    createdAt
    updatedAt
    lastLoginAt
  }
}
```

## Debugging

To see the actual SQL being generated:

```python
import logging
logging.getLogger('fraiseql.sql').setLevel(logging.DEBUG)
```

This will show how field names are being converted:
```
DEBUG: Converting GraphQL field 'firstName' to SQL path 'first_name'
DEBUG: Generated SQL: SELECT data->>'first_name' AS firstName FROM v_users
```

## Next Steps

- Explore [Database Views](./database-views.md) best practices
- Learn about [Performance Optimization](../advanced/performance.md)
- Read the [API Reference](../api-reference/index.md)
