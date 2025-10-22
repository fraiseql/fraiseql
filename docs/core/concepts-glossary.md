# Concepts & Glossary

Key concepts and terminology in FraiseQL.

## Core Concepts

### CQRS (Command Query Responsibility Segregation)

Separating read and write operations for optimal performance:

- **Commands (Writes)**: Mutations that modify data
- **Queries (Reads)**: Queries that fetch data from optimized views

**Benefits**:
- Optimized read paths with PostgreSQL views
- ACID transactions for writes
- Independent scaling of reads and writes

### Repository Pattern

Abstraction layer for database operations:

```python
from fraiseql.db import FraiseQLRepository

repo = FraiseQLRepository(pool)
users = await repo.find("users_view", is_active=True)
```

### Hybrid Tables

Tables with separate write and read paths:
- Writes go to normalized tables
- Reads come from denormalized views

See [Hybrid Tables Example](../../examples/hybrid_tables.py)

### DataLoader Pattern

Automatic batching to prevent N+1 queries:

```python
from fraiseql import dataloader

@fraiseql.field
@dataloader
async def posts(user: User, info: Info) -> List[Post]:
    return await info.context.repo.find("posts_view", user_id=user.id)
```

## GraphQL Concepts

### Type

Define your data models:

```python
@fraiseql.type
class User:
    id: UUID
    name: str
    email: str
```

### Query

Read operations:

```python
@fraiseql.query
def get_users(info: Info) -> List[User]:
    return info.context.repo.find("users_view")
```

### Mutation

Write operations:

```python
@fraiseql.mutation
async def create_user(info: Info, name: str, email: str) -> User:
    return await info.context.repo.insert("users", {"name": name, "email": email})
```

### Connection

Paginated results:

```python
@fraiseql.connection
def users(info: Info, first: int = 100) -> Connection[User]:
    return info.context.repo.find("users_view", limit=first)
```

### Field

Computed or related fields:

```python
@fraiseql.field
async def posts(user: User, info: Info) -> List[Post]:
    return info.context.repo.find("posts_view", user_id=user.id)
```

## Database Concepts

### View

Read-optimized database views:

```sql
CREATE OR REPLACE VIEW users_view AS
SELECT id, name, email, created_at
FROM users
WHERE deleted_at IS NULL;
```

### Materialized View

Pre-computed aggregations:

```sql
CREATE MATERIALIZED VIEW user_stats AS
SELECT
    user_id,
    COUNT(*) as post_count,
    MAX(created_at) as last_post_at
FROM posts
GROUP BY user_id;
```

### Index

Performance optimization:

```sql
CREATE INDEX idx_users_email ON users(email);
```

## Performance Concepts

### Query Complexity

Limiting query depth and breadth:

```python
from fraiseql import ComplexityConfig

config = ComplexityConfig(
    max_complexity=1000,
    max_depth=10
)
```

### APQ (Automatic Persisted Queries)

Caching GraphQL queries by hash to reduce bandwidth.

### Rust JSON Pipeline

High-performance JSON processing using Rust for 10-100x speed improvement.

## Security Concepts

### Field-Level Authorization

Control access at the field level:

```python
@fraiseql.field
@requires_permission("read:sensitive")
def sensitive_field(user: User, info: Info) -> str:
    return user.sensitive_data
```

### Rate Limiting

Prevent abuse:

```python
from fraiseql.auth import RateLimitConfig

rate_limit = RateLimitConfig(
    requests_per_minute=100
)
```

### Introspection Control

Disable schema introspection in production:

```python
config = FraiseQLConfig(
    introspection_enabled=False
)
```

## Related

- [Core Documentation](README.md)
- [Examples](../../examples/)
- [API Reference](../api-reference/)
