# Concepts & Glossary

Key concepts and terminology in FraiseQL.

## Core Concepts

### CQRS (Command Query Responsibility Segregation)

Separating read and write operations for optimal performance:

**Traditional vs FraiseQL:**
```
Traditional Approach:                    FraiseQL Approach:
┌─────────────────┐                     ┌─────────────────────────────────────┐
│   GraphQL       │                     │         GraphQL API                 │
│   API           │                     ├──────────────────┬──────────────────┤
├─────────┬───────┤                     │   QUERIES        │   MUTATIONS      │
│ Query   │ Mut. │                     │   (Reads)        │   (Writes)       │
├─────────┼───────┤                     ├──────────────────┼──────────────────┤
│ ORM     │ ORM   │                     │  v_* views       │  fn_* functions  │
│ Read    │ Write │                     │  tv_* tables     │  tb_* tables     │
└─────────┴───────┘                     └──────────────────┴──────────────────┘
Same code path                          Separate optimized paths
```

- **Commands (Writes)**: Mutations that modify data
- **Queries (Reads)**: Queries that fetch data from optimized views

**Benefits**:
- Optimized read paths with PostgreSQL views
- ACID transactions for writes
- Independent scaling of reads and writes

### Repository Pattern

Abstraction layer for database operations:

**JSONB View Pattern:**
```
┌─────────────┐      ┌──────────────┐      ┌─────────────┐
│  tb_user    │  →   │   v_user     │  →   │  GraphQL    │
│ (table)     │      │  (view)      │      │  Response   │
│             │      │              │      │             │
│ id: 1       │      │ SELECT       │      │ {           │
│ name: Alice │      │ jsonb_build_ │      │   "id": 1   │
│ email: a@b  │      │   object     │      │   "name":.. │
└─────────────┘      └──────────────┘      └─────────────┘
```

```python
from fraiseql.db import FraiseQLRepository

repo = FraiseQLRepository(pool)
users = await repo.find("users_view", is_active=True)
```

### Hybrid Tables

Tables with separate write and read paths:
- Writes go to normalized tables
- Reads come from denormalized views

See [Hybrid Tables Example](../../examples/hybrid_tables/)

### DataLoader Pattern

Automatic batching to prevent N+1 queries:

```python
from fraiseql import field

@field
def posts(user: User) -> List[Post]:
    """Get posts for user."""
    pass  # Implementation handled by framework
```

## GraphQL Concepts

### Type

Define your data models:

```python
from fraiseql import type

@type(sql_source="v_user")
class User:
    id: UUID
    name: str
    email: str
```

### Query

Read operations:

```python
from fraiseql import query
from typing import List

@query
def get_users() -> List[User]:
    """Get all users."""
    pass  # Implementation handled by framework
```

### Mutation

Write operations:

```python
from fraiseql import mutation

@mutation
def create_user(name: str, email: str) -> User:
    """Create a new user."""
    pass  # Implementation handled by framework
```

### Connection

Paginated results:

```python
from fraiseql import connection
from typing import List

@connection(node_type=User)
def users(first: int = 100) -> Connection[User]:
    """Get paginated users."""
    pass  # Implementation handled by framework
```

### Field

Computed or related fields:

```python
from fraiseql import field
from typing import List

@field
def posts(user: User) -> List[Post]:
    """Get posts for user."""
    pass  # Implementation handled by framework
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
from fraiseql import field

@field
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
- [API Reference](../reference/)
