# Application API Reference

Complete reference for FraiseQL application factory functions and configuration.

## create_fraiseql_app

```python
def create_fraiseql_app(
    *,
    database_url: str | None = None,
    types: Sequence[type] = (),
    mutations: Sequence[Callable[..., Any]] = (),
    queries: Sequence[type] = (),
    config: FraiseQLConfig | None = None,
    auth: Auth0Config | AuthProvider | None = None,
    context_getter: Callable[[Request], Awaitable[dict[str, Any]]] | None = None,
    lifespan: Callable[[FastAPI], Any] | None = None,
    title: str | None = None,
    version: str | None = None,
    description: str | None = None,
    production: bool = False,
    dev_auth_username: str | None = None,
    dev_auth_password: str | None = None,
    app: FastAPI | None = None,
) -> FastAPI
```

Creates a FastAPI application with FraiseQL GraphQL endpoint.

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `database_url` | `str` | Yes* | PostgreSQL connection URL with JSONB support |
| `types` | `Sequence[type]` | No | FraiseQL types to register in schema |
| `mutations` | `Sequence[Callable]` | No | Mutation resolver functions |
| `queries` | `Sequence[type]` | No | Query types (if not using @query decorator) |
| `config` | `FraiseQLConfig` | No | Full configuration object (overrides other params) |
| `auth` | `Auth0Config \| AuthProvider` | No | Authentication configuration |
| `context_getter` | `Callable` | No | Async function to build GraphQL context |
| `lifespan` | `Callable` | No | Custom lifespan context manager |
| `title` | `str` | No | API title for documentation |
| `version` | `str` | No | API version string |
| `description` | `str` | No | API description |
| `production` | `bool` | No | Enable production optimizations |
| `dev_auth_username` | `str` | No | Development auth username |
| `dev_auth_password` | `str` | No | Development auth password |
| `app` | `FastAPI` | No | Existing FastAPI app to extend |

*Required unless provided via `config` parameter

### Returns

`FastAPI` - Configured FastAPI application instance

### Examples

#### Basic Application

```python
from fraiseql import create_fraiseql_app, fraise_type, query
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

#### With Configuration Object

```python
from fraiseql.fastapi import FraiseQLConfig, create_fraiseql_app

config = FraiseQLConfig(
    database_url="postgresql://user:pass@localhost/db",
    environment="production",
    database_pool_size=50,
    enable_turbo_router=True
)

app = create_fraiseql_app(
    config=config,
    types=[User, Post, Comment]
)
```

#### With Authentication

```python
from fraiseql.auth.auth0 import Auth0Config

auth_config = Auth0Config(
    domain="myapp.auth0.com",
    api_identifier="https://api.myapp.com",
    algorithms=["RS256"]
)

app = create_fraiseql_app(
    database_url="postgresql://localhost/db",
    types=[User],
    auth=auth_config
)
```

#### With Custom Context

```python
async def get_context(request: Request) -> dict:
    return {
        "db": request.state.db,
        "user": await get_current_user(request),
        "request_id": request.headers.get("X-Request-ID"),
        "ip_address": request.client.host
    }

app = create_fraiseql_app(
    database_url="postgresql://localhost/db",
    context_getter=get_context
)
```

## create_production_app

```python
def create_production_app(
    config: FraiseQLConfig,
    types: Sequence[type] = (),
    mutations: Sequence[Callable[..., Any]] = (),
) -> FastAPI
```

Creates a production-optimized FastAPI application with FraiseQL.

Automatically enables:
- TurboRouter for registered queries
- JSON passthrough optimization
- Connection pooling with optimal settings
- Disabled introspection and playground
- Security headers and CORS
- Metrics and monitoring

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `config` | `FraiseQLConfig` | Yes | Complete configuration object |
| `types` | `Sequence[type]` | No | FraiseQL types to register |
| `mutations` | `Sequence[Callable]` | No | Mutation functions |

### Example

```python
config = FraiseQLConfig(
    database_url=os.getenv("DATABASE_URL"),
    environment="production",
    auth_provider="auth0",
    auth0_domain="myapp.auth0.com"
)

app = create_production_app(
    config=config,
    types=[User, Post, Comment],
    mutations=[create_post, update_post]
)
```

## FraiseQLConfig

Complete configuration class for FraiseQL applications.

### Database Configuration

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `database_url` | `str` | Required | PostgreSQL URL (supports Unix sockets) |
| `database_pool_size` | `int` | 20 | Maximum connections in pool |
| `database_max_overflow` | `int` | 10 | Additional overflow connections |
| `database_pool_timeout` | `int` | 30 | Seconds to wait for connection |
| `database_echo` | `bool` | False | Enable SQL query logging |

### Application Settings

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `app_name` | `str` | "FraiseQL API" | Application name |
| `app_version` | `str` | "1.0.0" | Version string |
| `environment` | `str` | "development" | Environment mode |

### GraphQL Settings

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enable_introspection` | `bool` | True* | Allow schema introspection |
| `enable_playground` | `bool` | True* | Enable GraphQL IDE |
| `playground_tool` | `str` | "graphiql" | IDE to use (graphiql/apollo-sandbox) |
| `max_query_depth` | `int \| None` | None | Maximum query nesting |
| `query_timeout` | `int` | 30 | Query timeout in seconds |
| `auto_camel_case` | `bool` | True | Convert snake_case to camelCase |

*Disabled in production unless explicitly enabled

### Performance Settings

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enable_turbo_router` | `bool` | True | Enable TurboRouter optimization |
| `turbo_router_cache_size` | `int` | 1000 | Max cached queries |
| `turbo_max_complexity` | `int` | 100 | Max complexity for caching |
| `turbo_max_total_weight` | `float` | 2000.0 | Max total cache weight |
| `enable_query_caching` | `bool` | True | Enable query result caching |
| `cache_ttl` | `int` | 300 | Cache TTL in seconds |

### JSON Passthrough Settings

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `json_passthrough_enabled` | `bool` | True | Enable JSON optimization |
| `json_passthrough_in_production` | `bool` | True | Auto-enable in production |
| `json_passthrough_cache_nested` | `bool` | True | Cache nested objects |
| `passthrough_complexity_limit` | `int` | 50 | Max complexity for passthrough |
| `passthrough_max_depth` | `int` | 3 | Max depth for passthrough |

### JSONB Extraction Settings

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `jsonb_extraction_enabled` | `bool` | True | Enable JSONB extraction |
| `jsonb_default_columns` | `list[str]` | ["data", "json_data", "jsonb_data"] | Column names to check |
| `jsonb_auto_detect` | `bool` | True | Auto-detect JSONB columns |
| `jsonb_field_limit_threshold` | `int` | 20 | Field count threshold |

### Authentication Settings

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `auth_enabled` | `bool` | True | Enable authentication |
| `auth_provider` | `str` | "none" | Provider (auth0/custom/none) |
| `auth0_domain` | `str \| None` | None | Auth0 domain |
| `auth0_api_identifier` | `str \| None` | None | Auth0 API identifier |
| `auth0_algorithms` | `list[str]` | ["RS256"] | JWT algorithms |
| `dev_auth_username` | `str \| None` | "admin" | Dev auth username |
| `dev_auth_password` | `str \| None` | None | Dev auth password |

### Security Settings

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `rate_limit_enabled` | `bool` | True | Enable rate limiting |
| `rate_limit_requests_per_minute` | `int` | 60 | Requests per minute |
| `rate_limit_requests_per_hour` | `int` | 1000 | Requests per hour |
| `rate_limit_burst_size` | `int` | 10 | Burst allowance |
| `complexity_enabled` | `bool` | True | Enable complexity analysis |
| `complexity_max_score` | `int` | 1000 | Maximum complexity score |
| `complexity_max_depth` | `int` | 10 | Maximum query depth |

### CORS Settings

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `cors_enabled` | `bool` | False | Enable CORS middleware |
| `cors_origins` | `list[str]` | [] | Allowed origins (empty by default) |
| `cors_methods` | `list[str]` | ["GET", "POST"] | Allowed methods |
| `cors_headers` | `list[str]` | ["*"] | Allowed headers |

!!! warning "CORS Configuration Required"
    CORS is disabled by default to prevent conflicts with reverse proxies like Nginx, Apache, or Cloudflare that handle CORS at the infrastructure level. You must explicitly enable and configure CORS if your application needs it:

    ```python
    config = FraiseQLConfig(
        cors_enabled=True,
        cors_origins=["https://yourdomain.com", "https://app.yourdomain.com"]
    )
    ```

### Environment Variables

All settings can be configured via environment variables with the `FRAISEQL_` prefix:

```bash
FRAISEQL_DATABASE_URL=postgresql://user:pass@localhost/db
FRAISEQL_ENVIRONMENT=production
FRAISEQL_DATABASE_POOL_SIZE=100
FRAISEQL_ENABLE_TURBO_ROUTER=true
FRAISEQL_AUTH_PROVIDER=auth0
FRAISEQL_AUTH0_DOMAIN=myapp.auth0.com
```

### Configuration File

Settings can also be loaded from a `.env` file:

```env
# .env
FRAISEQL_DATABASE_URL=postgresql://localhost/mydb
FRAISEQL_ENVIRONMENT=development
FRAISEQL_ENABLE_PLAYGROUND=true
FRAISEQL_DATABASE_POOL_SIZE=20
```

## create_db_pool

```python
async def create_db_pool(
    database_url: str,
    **pool_kwargs: Any
) -> psycopg_pool.AsyncConnectionPool
```

Creates an async database connection pool with FraiseQL's custom type handling.

### Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `database_url` | `str` | PostgreSQL connection URL |
| `**pool_kwargs` | `Any` | Additional psycopg pool parameters |

### Pool Configuration Options

- `min_size`: Minimum number of connections (default: 4)
- `max_size`: Maximum number of connections (default: 20)
- `timeout`: Connection acquisition timeout (default: 30)
- `max_waiting`: Maximum waiting requests (default: 0 - unlimited)
- `max_lifetime`: Maximum connection lifetime in seconds
- `max_idle`: Maximum idle time before closing connection

### Type Handling

The pool automatically configures PostgreSQL type adapters to:
- Keep dates as ISO strings instead of Python objects
- Preserve exact PostgreSQL formatting
- Optimize for JSON serialization

### Example

```python
pool = await create_db_pool(
    "postgresql://user:pass@localhost/db",
    min_size=10,
    max_size=50,
    timeout=10,
    max_lifetime=3600
)

async with pool.connection() as conn:
    async with conn.cursor() as cur:
        await cur.execute("SELECT * FROM users")
        users = await cur.fetchall()
```

## Performance Characteristics

### Connection Pool Sizing

| Application Size | Pool Size | Max Overflow | Timeout |
|-----------------|-----------|--------------|---------|
| Small (< 100 RPS) | 10-20 | 5-10 | 30s |
| Medium (100-1000 RPS) | 20-50 | 10-20 | 20s |
| Large (> 1000 RPS) | 50-100 | 20-40 | 10s |

### Memory Usage

| Component | Memory per Instance | Notes |
|-----------|-------------------|--------|
| Connection | ~2-4 MB | PostgreSQL connection overhead |
| Query Cache Entry | ~1-10 KB | Depends on query complexity |
| TurboRouter Entry | ~500 bytes | SQL template + metadata |
| DataLoader Batch | ~100 bytes/key | Temporary during request |

### Startup Performance

| Configuration | Startup Time | First Query |
|--------------|--------------|-------------|
| Development | ~500ms | ~100ms |
| Production (cold) | ~1s | ~50ms |
| Production (warm) | ~200ms | ~5ms |
| TurboRouter | ~200ms | ~1ms |

## Thread Safety

All FraiseQL components are designed to be thread-safe:

- Connection pools use async locks
- TurboRegistry uses OrderedDict with proper locking
- Schema building is done once at startup
- Context is request-scoped

## Error Handling

FraiseQL provides structured error responses:

```python
{
    "errors": [{
        "message": "User not found",
        "extensions": {
            "code": "NOT_FOUND",
            "field": "user",
            "id": "123e4567-e89b-12d3-a456-426614174000"
        }
    }]
}
```

Custom error handling:

```python
from graphql import GraphQLError

@query
async def get_user(info, id: UUID) -> User:
    user = await db.find_one("users", {"id": id})
    if not user:
        raise GraphQLError(
            "User not found",
            extensions={"code": "NOT_FOUND", "id": str(id)}
        )
    return user
```
