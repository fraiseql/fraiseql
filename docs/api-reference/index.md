# API Reference

Complete technical reference for FraiseQL v0.1.0 components, functions, and configuration options.

## Core Components

### Application Factory

- [`create_fraiseql_app()`](./application.md#create_fraiseql_app) - Create FastAPI application with FraiseQL endpoint
- [`FraiseQLConfig`](./application.md#fraiseqlconfig) - Complete configuration class
- [`create_production_app()`](./application.md#create_production_app) - Production-optimized application factory

### Decorators

FraiseQL provides powerful decorators for GraphQL schema definition:

- [`@query`](./decorators.md#query) - Define GraphQL queries
- [`@mutation`](./decorators.md#mutation) - Define GraphQL mutations
- [`@subscription`](./decorators.md#subscription) - Define real-time subscriptions
- [`@field`](./decorators.md#field) - Define computed fields
- [`@dataloader_field`](./decorators.md#dataloader_field) - N+1 query prevention

### Type System

- [`@fraise_type`](./decorators.md#fraise_type) - Define GraphQL object types
- [`@fraise_input`](./decorators.md#fraise_input) - Define input types
- [`@fraise_enum`](./decorators.md#fraise_enum) - Define enum types
- [`@fraise_interface`](./decorators.md#fraise_interface) - Define interface types

### Repository & Execution

- [`CQRSRepository`](./repository.md) - Database access layer
- [`CQRSExecutor`](./repository.md#cqrsexecutor) - Query execution engine
- [`TurboRouter`](./turbo-router.md) - High-performance direct SQL router

## Configuration Reference

### Environment Variables

All configuration can be set via environment variables prefixed with `FRAISEQL_`:

```bash
FRAISEQL_DATABASE_URL=postgresql://user:pass@localhost/db
FRAISEQL_ENVIRONMENT=production
FRAISEQL_ENABLE_TURBO_ROUTER=true
```

### Configuration Classes

```python
from fraiseql.fastapi import FraiseQLConfig

config = FraiseQLConfig(
    database_url="postgresql://...",
    environment="production",
    enable_turbo_router=True,
    database_pool_size=50
)
```

See [FraiseQLConfig](./application.md#fraiseqlconfig) for all options.

## Performance APIs

### TurboRouter

Direct SQL execution for registered queries:

```python
from fraiseql.fastapi import TurboRegistry, TurboQuery

registry = TurboRegistry(max_size=1000)
query = TurboQuery(
    graphql_query="...",
    sql_template="SELECT * FROM users WHERE id = %(id)s",
    param_mapping={"id": "id"}
)
registry.register(query)
```

### DataLoader Integration

Automatic batching for N+1 prevention:

```python
@fraise_type
class User:
    id: UUID

    @dataloader_field
    async def posts(self, info) -> list[Post]:
        # Automatically batched
        return await load_posts_for_user(self.id)
```

## Database APIs

### Connection Pool Management

```python
from fraiseql.fastapi.app import create_db_pool

pool = await create_db_pool(
    database_url,
    min_size=10,
    max_size=50,
    timeout=30
)
```

### Query Building

```python
from fraiseql.sql import WhereClause, SQLGenerator

where = WhereClause(filters={"name__icontains": "john"})
sql = SQLGenerator.generate_select(
    table="users",
    where=where,
    limit=10
)
```

## Authentication & Security

### Auth Decorators

```python
from fraiseql.auth import requires_auth, requires_role

@query
@requires_auth
async def get_profile(info) -> User:
    user = info.context["user"]
    return await fetch_user(user.id)

@mutation
@requires_role("admin")
async def delete_user(info, id: UUID) -> bool:
    return await delete_from_db(id)
```

### Field Authorization

```python
@fraise_type
class User:
    id: UUID
    email: str  # Public

    @field
    @requires_auth
    def ssn(self) -> str:
        # Only visible to authenticated users
        return self._ssn
```

## Extension Points

### Custom Scalars

```python
from fraiseql.types.scalars import create_scalar

IPv4 = create_scalar(
    "IPv4",
    serialize=str,
    parse_value=ipaddress.IPv4Address,
    parse_literal=lambda ast: ipaddress.IPv4Address(ast.value)
)
```

### Context Injection

```python
async def get_context(request: Request) -> dict:
    return {
        "db": request.state.db,
        "user": request.state.user,
        "request_id": request.headers.get("X-Request-ID")
    }

app = create_fraiseql_app(
    context_getter=get_context
)
```

## CLI Reference

See [CLI Commands](./cli/index.md) for command-line interface documentation.

## Migration Guides

- [General Migration](../migration/index.md) - Migrate from existing GraphQL frameworks
- [ORM-based Frameworks](../migration/index.md#from-orm-based-frameworks) - From Strawberry, Graphene, etc.
- [Schema-first Frameworks](../migration/index.md#from-schema-first-frameworks) - From PostGraphile, Hasura, etc.

## Complete Examples

### Minimal Application

```python
from fraiseql import create_fraiseql_app, query, fraise_type
from uuid import UUID

@fraise_type
class User:
    id: UUID
    name: str
    email: str

@query
async def get_user(info, id: UUID) -> User:
    db = info.context["db"]
    return await db.find_one("users", {"id": id})

app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User]
)
```

### Production Application

```python
from fraiseql import create_production_app
from fraiseql.fastapi import FraiseQLConfig

config = FraiseQLConfig(
    database_url=os.getenv("DATABASE_URL"),
    environment="production",
    enable_turbo_router=True,
    database_pool_size=100,
    enable_auth=True,
    auth_provider="auth0",
    auth0_domain="myapp.auth0.com"
)

app = create_production_app(
    config=config,
    types=[User, Post, Comment],
    mutations=[create_post, update_post, delete_post]
)
```

## API Stability

All components in FraiseQL v0.1.0 are considered stable for production use. The framework follows semantic versioning, and breaking changes will only be introduced in major version updates.

| Component | Status | Version |
|-----------|--------|---------|
| Core Decorators | Stable | v0.1.0 |
| Type System | Stable | v0.1.0 |
| FastAPI Integration | Stable | v0.1.0 |
| TurboRouter | Stable | v0.1.0 |
| DataLoader | Stable | v0.1.0 |
| Auth Integration | Stable | v0.1.0 |
| WebSocket Subscriptions | Stable | v0.1.0 |
| CQRS Repository | Stable | v0.1.0 |

## Next Steps

- [Application Configuration](./application.md) - Detailed configuration reference
- [Decorators Reference](./decorators.md) - Complete decorator documentation
- [Performance Guide](../advanced/performance.md) - Optimization strategies
- [Security Best Practices](../advanced/security.md) - Production security
