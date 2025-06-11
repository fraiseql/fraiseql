# Architecture Overview

FraiseQL's architecture is designed around a simple principle: let PostgreSQL do what it does best while providing a seamless GraphQL experience.

## High-Level Architecture

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│  GraphQL Client │────▶│   FraiseQL API   │────▶│   PostgreSQL    │
└─────────────────┘     └──────────────────┘     └─────────────────┘
                               │                           │
                               ▼                           ▼
                        ┌──────────────┐           ┌──────────────┐
                        │ Query Builder│           │ JSON Views   │
                        └──────────────┘           └──────────────┘
```

## Component Overview

### 1. GraphQL Layer

The GraphQL layer handles:
- Schema generation from Python types
- Query parsing and validation
- Field resolution coordination

```python
@fraiseql.type
class User:
    id: int
    name: str
    email: str
```

### 2. Query Translation

FraiseQL translates GraphQL queries into efficient SQL:

**GraphQL Query:**
```graphql
query {
  users {
    id
    name
  }
}
```

**Generated SQL:**
```sql
SELECT
    id,
    data->>'id' as id,
    data->>'name' as name
FROM users_view
```

### 3. Database Views

The heart of FraiseQL's efficiency - database views that return JSON:

```sql
CREATE VIEW users_view AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', data->>'name',
        'email', data->>'email',
        'posts', (
            SELECT jsonb_agg(p.data)
            FROM posts_view p
            WHERE p.author_id = users.id
        )
    ) as data
FROM users;
```

### 4. Field Selection

FraiseQL's query builder extracts only requested fields:

```python
# Only selects id and name from the JSON
SELECT data->>'id', data->>'name' FROM users_view
```

## Request Flow

1. **Client sends GraphQL query** to the FraiseQL endpoint
2. **Schema validates** the query structure
3. **Query translator** converts GraphQL to SQL
4. **Field selector** identifies which JSON fields to extract
5. **SQL executes** against the appropriate view
6. **Results transform** from JSONB to GraphQL response

## Key Design Decisions

### Why Views?

Database views provide several advantages:
- **Single source of truth** for data shape
- **Composition** through view references
- **Performance** via materialized views when needed
- **Security** through view-level permissions

### Why JSONB?

PostgreSQL's JSONB type enables:
- **Flexible schemas** without constant migrations
- **Efficient indexing** with GIN indexes
- **Native operations** for field extraction
- **Nested data** without complex joins

### Why One View Per Resolver?

This constraint ensures:
- **Predictable performance** - one query per resolver
- **No N+1 problems** - relationships handled in views
- **Clear boundaries** - easy to reason about data flow
- **Optimization opportunities** - views can be materialized

## Production Optimizations

FraiseQL provides two execution modes:

### Development Mode
- Full GraphQL validation
- Helpful error messages
- Schema introspection
- Hot reloading

### Production Mode
- Bypasses GraphQL validation
- Direct SQL execution
- No introspection
- Minimal overhead

## Integration Points

### FastAPI Integration

FraiseQL provides seamless FastAPI integration:

```python
app = fraiseql.create_fraiseql_app(
    database_url="postgresql://...",
    types=[User, Post],
    production=True
)
```

### Authentication

Pluggable authentication with built-in Auth0 support:

```python
app = fraiseql.create_fraiseql_app(
    database_url="postgresql://...",
    types=[User, Post],
    auth_provider=Auth0Provider(
        domain="your-domain.auth0.com",
        api_identifier="your-api"
    )
)
```

### Database Connection

FraiseQL uses psycopg3 with connection pooling:

```python
# Automatic connection management
async with CQRSRepository(db) as repo:
    users = await repo.get_many(User)
```

## Performance Characteristics

- **Query Complexity**: O(1) database queries per resolver
- **Field Selection**: O(n) where n is number of fields
- **Memory Usage**: Streaming results for large datasets
- **Connection Pooling**: Configurable pool size

## Next Steps

- Dive into the [Type System](./type-system.md)
- Learn about [Database Views](./database-views.md)
- Understand [Query Translation](./query-translation.md)
