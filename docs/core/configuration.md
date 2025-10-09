# Configuration

FraiseQLConfig class for comprehensive application configuration.

## Overview

```python
from fraiseql import FraiseQLConfig, create_fraiseql_app

config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    environment="production",
    enable_playground=False
)

app = create_fraiseql_app(types=[User, Post], config=config)
```

## Core Settings

### Database

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| database_url | PostgresUrl | Required | PostgreSQL connection URL (supports Unix sockets) |
| database_pool_size | int | 20 | Maximum number of connections in pool |
| database_max_overflow | int | 10 | Extra connections allowed beyond pool_size |
| database_pool_timeout | int | 30 | Connection timeout in seconds |
| database_echo | bool | False | Enable SQL query logging (development only) |

**Examples**:
```python
# Standard PostgreSQL URL
config = FraiseQLConfig(
    database_url="postgresql://user:pass@localhost:5432/mydb"
)

# Unix socket connection
config = FraiseQLConfig(
    database_url="postgresql://user@/var/run/postgresql:5432/mydb"
)

# With connection pool tuning
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    database_pool_size=50,
    database_max_overflow=20,
    database_pool_timeout=60
)
```

### Application

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| app_name | str | "FraiseQL API" | Application name displayed in API documentation |
| app_version | str | "1.0.0" | Application version string |
| environment | Literal | "development" | Environment mode (development/production/testing) |

**Examples**:
```python
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    app_name="My GraphQL API",
    app_version="2.1.0",
    environment="production"
)
```

## GraphQL Settings

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| introspection_policy | IntrospectionPolicy | PUBLIC | Schema introspection access control |
| enable_playground | bool | True | Enable GraphQL playground IDE |
| playground_tool | Literal | "graphiql" | GraphQL IDE to use (graphiql/apollo-sandbox) |
| max_query_depth | int \| None | None | Maximum allowed query depth (None = unlimited) |
| query_timeout | int | 30 | Maximum query execution time in seconds |
| auto_camel_case | bool | True | Auto-convert snake_case fields to camelCase |

**Introspection Policies**:

| Policy | Description |
|--------|-------------|
| IntrospectionPolicy.DISABLED | No introspection for anyone |
| IntrospectionPolicy.PUBLIC | Introspection allowed for everyone (default) |
| IntrospectionPolicy.AUTHENTICATED | Introspection only for authenticated users |

**Examples**:
```python
from fraiseql.fastapi.config import IntrospectionPolicy

# Production configuration (introspection disabled)
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    environment="production",
    introspection_policy=IntrospectionPolicy.DISABLED,
    enable_playground=False,
    max_query_depth=10,
    query_timeout=15
)

# Development configuration
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    environment="development",
    introspection_policy=IntrospectionPolicy.PUBLIC,
    enable_playground=True,
    playground_tool="graphiql",
    database_echo=True  # Log all SQL queries
)
```

## Performance Settings

### Query Caching

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| enable_query_caching | bool | True | Enable query result caching |
| cache_ttl | int | 300 | Cache time-to-live in seconds |

### TurboRouter

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| enable_turbo_router | bool | True | Enable TurboRouter for registered queries |
| turbo_router_cache_size | int | 1000 | Maximum number of queries to cache |
| turbo_router_auto_register | bool | False | Auto-register queries at startup |
| turbo_max_complexity | int | 100 | Max complexity score for turbo caching |
| turbo_max_total_weight | float | 2000.0 | Max total weight of cached queries |
| turbo_enable_adaptive_caching | bool | True | Enable complexity-based admission |

**Examples**:
```python
# High-performance configuration
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    enable_query_caching=True,
    cache_ttl=600,  # 10 minutes
    enable_turbo_router=True,
    turbo_router_cache_size=5000,
    turbo_max_complexity=200
)
```

### JSON Passthrough

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| json_passthrough_enabled | bool | True | Enable JSON passthrough optimization |
| json_passthrough_in_production | bool | True | Auto-enable in production mode |
| json_passthrough_cache_nested | bool | True | Cache wrapped nested objects |
| passthrough_complexity_limit | int | 50 | Max complexity for passthrough mode |
| passthrough_max_depth | int | 3 | Max query depth for passthrough |
| passthrough_auto_detect_views | bool | True | Auto-detect database views |
| passthrough_cache_view_metadata | bool | True | Cache view metadata |
| passthrough_view_metadata_ttl | int | 3600 | Metadata cache TTL in seconds |

### JSONB Extraction

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| jsonb_extraction_enabled | bool | True | Enable automatic JSONB column extraction |
| jsonb_default_columns | list[str] | ["data", "json_data", "jsonb_data"] | Default JSONB column names to search |
| jsonb_auto_detect | bool | True | Auto-detect JSONB columns by content analysis |
| jsonb_field_limit_threshold | int | 20 | Field count threshold for full data column |

**Examples**:
```python
# JSONB-optimized configuration
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    jsonb_extraction_enabled=True,
    jsonb_default_columns=["data", "metadata", "json_data"],
    jsonb_auto_detect=True,
    jsonb_field_limit_threshold=30
)
```

### CamelForge

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| camelforge_enabled | bool | False | Enable database-native camelCase transformation |
| camelforge_function | str | "turbo.fn_camelforge" | PostgreSQL function name for CamelForge |
| camelforge_field_threshold | int | 20 | Field count threshold for CamelForge |

**Examples**:
```python
# Enable CamelForge for large objects
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    camelforge_enabled=True,
    camelforge_function="turbo.fn_camelforge",
    camelforge_field_threshold=25
)
```

## Authentication Settings

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| auth_enabled | bool | True | Enable authentication system |
| auth_provider | Literal | "none" | Auth provider (auth0/custom/none) |
| auth0_domain | str \| None | None | Auth0 tenant domain |
| auth0_api_identifier | str \| None | None | Auth0 API identifier |
| auth0_algorithms | list[str] | ["RS256"] | Auth0 JWT algorithms |
| dev_auth_username | str \| None | "admin" | Development mode username |
| dev_auth_password | str \| None | None | Development mode password |

**Examples**:
```python
# Auth0 configuration
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    auth_enabled=True,
    auth_provider="auth0",
    auth0_domain="myapp.auth0.com",
    auth0_api_identifier="https://api.myapp.com",
    auth0_algorithms=["RS256"]
)

# Development authentication
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    environment="development",
    auth_provider="custom",
    dev_auth_username="admin",
    dev_auth_password="secret"
)
```

## CORS Settings

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| cors_enabled | bool | False | Enable CORS (disabled by default) |
| cors_origins | list[str] | [] | Allowed CORS origins |
| cors_methods | list[str] | ["GET", "POST"] | Allowed HTTP methods |
| cors_headers | list[str] | ["Content-Type", "Authorization"] | Allowed headers |

**Examples**:
```python
# Production CORS (specific origins)
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    cors_enabled=True,
    cors_origins=[
        "https://app.example.com",
        "https://admin.example.com"
    ],
    cors_methods=["GET", "POST", "OPTIONS"],
    cors_headers=["Content-Type", "Authorization", "X-Request-ID"]
)

# Development CORS (permissive)
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    environment="development",
    cors_enabled=True,
    cors_origins=["http://localhost:3000", "http://localhost:8080"]
)
```

## Rate Limiting Settings

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| rate_limit_enabled | bool | True | Enable rate limiting |
| rate_limit_requests_per_minute | int | 60 | Max requests per minute |
| rate_limit_requests_per_hour | int | 1000 | Max requests per hour |
| rate_limit_burst_size | int | 10 | Burst size for rate limiting |
| rate_limit_window_type | str | "sliding" | Window type (sliding/fixed) |
| rate_limit_whitelist | list[str] | [] | IP addresses to whitelist |
| rate_limit_blacklist | list[str] | [] | IP addresses to blacklist |

**Examples**:
```python
# Strict rate limiting
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    rate_limit_enabled=True,
    rate_limit_requests_per_minute=30,
    rate_limit_requests_per_hour=500,
    rate_limit_burst_size=5,
    rate_limit_whitelist=["10.0.0.1", "10.0.0.2"]
)
```

## Complexity Settings

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| complexity_enabled | bool | True | Enable query complexity analysis |
| complexity_max_score | int | 1000 | Maximum allowed complexity score |
| complexity_max_depth | int | 10 | Maximum query depth |
| complexity_default_list_size | int | 10 | Default list size for complexity calculation |
| complexity_include_in_response | bool | False | Include complexity score in response |
| complexity_field_multipliers | dict[str, int] | {} | Custom field complexity multipliers |

**Examples**:
```python
# Complexity limits
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    complexity_enabled=True,
    complexity_max_score=500,
    complexity_max_depth=8,
    complexity_default_list_size=20,
    complexity_field_multipliers={
        "users": 2,  # Users query costs 2x
        "posts": 1,  # Standard cost
        "comments": 3  # Comments query costs 3x
    }
)
```

## APQ (Automatic Persisted Queries) Settings

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| apq_storage_backend | Literal | "memory" | Storage backend (memory/postgresql/redis/custom) |
| apq_cache_responses | bool | False | Enable JSON response caching for APQ queries |
| apq_response_cache_ttl | int | 600 | Cache TTL for APQ responses in seconds |
| apq_backend_config | dict[str, Any] | {} | Backend-specific configuration options |

**Examples**:
```python
# APQ with PostgreSQL backend
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    apq_storage_backend="postgresql",
    apq_cache_responses=True,
    apq_response_cache_ttl=900  # 15 minutes
)

# APQ with Redis backend
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    apq_storage_backend="redis",
    apq_backend_config={
        "redis_url": "redis://localhost:6379/0",
        "key_prefix": "apq:"
    }
)
```

## Token Revocation Settings

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| revocation_enabled | bool | True | Enable token revocation |
| revocation_check_enabled | bool | True | Check revocation status on requests |
| revocation_ttl | int | 86400 | Token revocation TTL (24 hours) |
| revocation_cleanup_interval | int | 3600 | Cleanup interval (1 hour) |
| revocation_store_type | str | "memory" | Storage type (memory/redis) |

## Execution Mode Settings

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| execution_mode_priority | list[str] | ["turbo", "passthrough", "normal"] | Execution mode priority order |
| unified_executor_enabled | bool | True | Enable unified executor |
| include_execution_metadata | bool | False | Include mode and timing in response |
| execution_timeout_ms | int | 30000 | Execution timeout in milliseconds |
| enable_mode_hints | bool | True | Enable mode hints in queries |
| mode_hint_pattern | str | r"#\s*@mode:\s*(\w+)" | Regex pattern for mode hints |

**Examples**:
```python
# Custom execution priority
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    execution_mode_priority=["passthrough", "turbo", "normal"],
    unified_executor_enabled=True,
    include_execution_metadata=True,  # Add timing info to responses
    execution_timeout_ms=15000  # 15 second timeout
)
```

## Schema Settings

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| default_mutation_schema | str | "public" | Default schema for mutations |
| default_query_schema | str | "public" | Default schema for queries |

**Examples**:
```python
# Custom schema configuration
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    default_mutation_schema="app",
    default_query_schema="api"
)
```

## Entity Routing

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| entity_routing | EntityRoutingConfig \| dict \| None | None | Entity-aware query routing configuration |

**Examples**:
```python
from fraiseql.routing.config import EntityRoutingConfig

# Entity routing configuration
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    entity_routing=EntityRoutingConfig(
        enabled=True,
        default_schema="public",
        entity_mapping={
            "User": "users_schema",
            "Post": "content_schema"
        }
    )
)

# Or using dict
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    entity_routing={
        "enabled": True,
        "default_schema": "public",
        "entity_mapping": {
            "User": "users_schema"
        }
    }
)
```

## Environment Variables

All configuration options can be set via environment variables with the `FRAISEQL_` prefix:

```bash
# Database
export FRAISEQL_DATABASE_URL="postgresql://localhost/mydb"
export FRAISEQL_DATABASE_POOL_SIZE=50

# Application
export FRAISEQL_APP_NAME="My API"
export FRAISEQL_ENVIRONMENT="production"

# GraphQL
export FRAISEQL_INTROSPECTION_POLICY="disabled"
export FRAISEQL_ENABLE_PLAYGROUND="false"
export FRAISEQL_MAX_QUERY_DEPTH=10

# Auth
export FRAISEQL_AUTH_PROVIDER="auth0"
export FRAISEQL_AUTH0_DOMAIN="myapp.auth0.com"
export FRAISEQL_AUTH0_API_IDENTIFIER="https://api.myapp.com"
```

## .env File Support

Configuration can also be loaded from .env files:

```bash
# .env file
FRAISEQL_DATABASE_URL=postgresql://localhost/mydb
FRAISEQL_ENVIRONMENT=production
FRAISEQL_INTROSPECTION_POLICY=disabled
FRAISEQL_ENABLE_PLAYGROUND=false
```

```python
# Automatically loads from .env
config = FraiseQLConfig()
```

## Complete Example

```python
from fraiseql import FraiseQLConfig, create_fraiseql_app
from fraiseql.fastapi.config import IntrospectionPolicy

# Production-ready configuration
config = FraiseQLConfig(
    # Database
    database_url="postgresql://user:pass@db.example.com:5432/prod",
    database_pool_size=50,
    database_max_overflow=20,
    database_pool_timeout=60,

    # Application
    app_name="Production API",
    app_version="2.0.0",
    environment="production",

    # GraphQL
    introspection_policy=IntrospectionPolicy.DISABLED,
    enable_playground=False,
    max_query_depth=10,
    query_timeout=15,
    auto_camel_case=True,

    # Performance
    enable_query_caching=True,
    cache_ttl=600,
    enable_turbo_router=True,
    turbo_router_cache_size=5000,
    jsonb_extraction_enabled=True,

    # Auth
    auth_enabled=True,
    auth_provider="auth0",
    auth0_domain="myapp.auth0.com",
    auth0_api_identifier="https://api.myapp.com",

    # CORS
    cors_enabled=True,
    cors_origins=["https://app.example.com"],
    cors_methods=["GET", "POST"],

    # Rate Limiting
    rate_limit_enabled=True,
    rate_limit_requests_per_minute=30,
    rate_limit_requests_per_hour=500,

    # Complexity
    complexity_enabled=True,
    complexity_max_score=500,
    complexity_max_depth=8,

    # APQ
    apq_storage_backend="redis",
    apq_cache_responses=True,
    apq_response_cache_ttl=900
)

app = create_fraiseql_app(types=[User, Post, Comment], config=config)
```

## See Also

- [API Reference - Config](../api-reference/config.md) - Complete config reference
- [Deployment](../production/deployment.md) - Production deployment guides
