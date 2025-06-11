# Configuration

FraiseQL provides flexible configuration options through both code and environment variables. This guide covers all available configuration settings.

## Configuration Methods

### 1. Using FraiseQLConfig Object

The most explicit way to configure FraiseQL:

```python
from fraiseql.fastapi import create_fraiseql_app, FraiseQLConfig

config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    app_name="My GraphQL API",
    app_version="1.0.0",
    environment="development",
    enable_playground=True,
    enable_introspection=True,
    cors_enabled=True,
    cors_origins=["http://localhost:3000"],
    database_pool_size=20,
    database_pool_timeout=30
)

app = create_fraiseql_app(
    types=[User, Post],
    config=config
)
```

### 2. Using Function Parameters

Pass configuration directly to `create_fraiseql_app`:

```python
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User, Post],
    title="My API",
    version="1.0.0",
    production=False,  # Sets environment to development
    dev_auth_username="admin",
    dev_auth_password="secret"
)
```

### 3. Using Environment Variables

FraiseQL automatically reads configuration from environment variables:

```bash
# Database
export DATABASE_URL="postgresql://user:pass@localhost/mydb"
export FRAISEQL_DATABASE_URL="postgresql://user:pass@localhost/mydb"  # Alternative

# Application
export FRAISEQL_ENVIRONMENT="production"
export FRAISEQL_APP_NAME="My GraphQL API"
export FRAISEQL_APP_VERSION="1.0.0"

# GraphQL Playground
export FRAISEQL_ENABLE_PLAYGROUND="true"
export FRAISEQL_ENABLE_INTROSPECTION="true"

# Development Authentication
export FRAISEQL_DEV_AUTH_USERNAME="admin"
export FRAISEQL_DEV_AUTH_PASSWORD="secret123"

# CORS
export FRAISEQL_CORS_ENABLED="true"
export FRAISEQL_CORS_ORIGINS="http://localhost:3000,https://myapp.com"

# Database Pool
export FRAISEQL_DATABASE_POOL_SIZE="20"
export FRAISEQL_DATABASE_POOL_TIMEOUT="30"

# Query Compilation
export FRAISEQL_COMPILED_QUERIES_PATH="/app/compiled_queries"
```

## Configuration Options

### Core Settings

| Setting | Type | Default | Environment Variable | Description |
|---------|------|---------|---------------------|-------------|
| `database_url` | str | Required | `DATABASE_URL` or `FRAISEQL_DATABASE_URL` | PostgreSQL connection URL |
| `app_name` | str | "FraiseQL API" | `FRAISEQL_APP_NAME` | API title shown in docs |
| `app_version` | str | "0.1.0" | `FRAISEQL_APP_VERSION` | API version |
| `environment` | str | "development" | `FRAISEQL_ENVIRONMENT` | Runtime environment |

### GraphQL Settings

| Setting | Type | Default | Environment Variable | Description |
|---------|------|---------|---------------------|-------------|
| `enable_playground` | bool | True (dev) / False (prod) | `FRAISEQL_ENABLE_PLAYGROUND` | Enable GraphQL Playground at `/playground` |
| `enable_introspection` | bool | True (dev) / False (prod) | `FRAISEQL_ENABLE_INTROSPECTION` | Enable GraphQL schema introspection |
| `graphql_path` | str | "/graphql" | `FRAISEQL_GRAPHQL_PATH` | GraphQL endpoint path |
| `playground_path` | str | "/playground" | `FRAISEQL_PLAYGROUND_PATH` | GraphQL Playground path |

### Development Authentication

| Setting | Type | Default | Environment Variable | Description |
|---------|------|---------|---------------------|-------------|
| `dev_auth_username` | str | "admin" | `FRAISEQL_DEV_AUTH_USERNAME` | Dev auth username |
| `dev_auth_password` | str | None | `FRAISEQL_DEV_AUTH_PASSWORD` | Dev auth password (enables auth if set) |

### CORS Settings

| Setting | Type | Default | Environment Variable | Description |
|---------|------|---------|---------------------|-------------|
| `cors_enabled` | bool | True | `FRAISEQL_CORS_ENABLED` | Enable CORS middleware |
| `cors_origins` | list[str] | ["*"] | `FRAISEQL_CORS_ORIGINS` | Allowed origins (comma-separated) |
| `cors_methods` | list[str] | ["*"] | `FRAISEQL_CORS_METHODS` | Allowed HTTP methods |
| `cors_headers` | list[str] | ["*"] | `FRAISEQL_CORS_HEADERS` | Allowed headers |

### Database Pool Settings

| Setting | Type | Default | Environment Variable | Description |
|---------|------|---------|---------------------|-------------|
| `database_pool_size` | int | 10 | `FRAISEQL_DATABASE_POOL_SIZE` | Maximum pool connections |
| `database_pool_timeout` | int | 30 | `FRAISEQL_DATABASE_POOL_TIMEOUT` | Connection timeout (seconds) |
| `database_pool_min_size` | int | 1 | `FRAISEQL_DATABASE_POOL_MIN_SIZE` | Minimum pool connections |

### Performance Settings

| Setting | Type | Default | Environment Variable | Description |
|---------|------|---------|---------------------|-------------|
| `compiled_queries_path` | str | None | `FRAISEQL_COMPILED_QUERIES_PATH` | Path to compiled queries (production) |
| `query_cache_size` | int | 1000 | `FRAISEQL_QUERY_CACHE_SIZE` | Query cache entries |
| `enable_query_logging` | bool | False | `FRAISEQL_ENABLE_QUERY_LOGGING` | Log SQL queries |

## Environment-Specific Behavior

### Development Mode (Default)

When `environment="development"` or `production=False`:

- GraphQL Playground enabled at `/playground`
- Schema introspection enabled
- Detailed error messages
- Query validation enabled
- Development authentication available

### Production Mode

When `environment="production"` or `production=True`:

- GraphQL Playground disabled (security)
- Schema introspection disabled (security)
- Minimal error messages
- Optimized query execution
- Development authentication disabled

Override production defaults if needed:

```python
# Force enable playground in production (not recommended)
config = FraiseQLConfig(
    environment="production",
    enable_playground=True,  # Override
    enable_introspection=True  # Override
)
```

## Configuration Precedence

Configuration values are resolved in this order (highest to lowest priority):

1. Explicit function parameters to `create_fraiseql_app()`
2. `FraiseQLConfig` object values
3. Environment variables
4. Default values

Example:

```python
# Environment variable set: FRAISEQL_APP_NAME="From Env"

# This will use "My API" (explicit parameter wins)
app = create_fraiseql_app(
    types=[User],
    title="My API",  # Wins
    config=FraiseQLConfig(app_name="From Config")
)
```

## Common Configuration Patterns

### Local Development

```python
# Simple development setup
app = create_fraiseql_app(
    database_url="postgresql://localhost/dev_db",
    types=[User, Post],
    dev_auth_password="dev123"  # Enable simple auth
)
```

### Production with Auth0

```python
from fraiseql.auth import Auth0Config

app = create_fraiseql_app(
    types=[User, Post],
    production=True,
    auth=Auth0Config(
        domain="myapp.auth0.com",
        api_identifier="https://api.myapp.com"
    ),
    config=FraiseQLConfig(
        cors_origins=["https://myapp.com", "https://www.myapp.com"],
        database_pool_size=50
    )
)
```

### Testing Configuration

```python
# Minimal config for tests
app = create_fraiseql_app(
    database_url="postgresql://localhost/test_db",
    types=[User],
    config=FraiseQLConfig(
        enable_playground=False,
        cors_enabled=False,
        database_pool_size=1
    )
)
```

### Docker Configuration

```dockerfile
# Dockerfile
ENV FRAISEQL_ENVIRONMENT=production
ENV FRAISEQL_DATABASE_POOL_SIZE=20
ENV FRAISEQL_CORS_ORIGINS=https://api.myapp.com

# docker-compose.yml
environment:
  - DATABASE_URL=postgresql://postgres:postgres@db:5432/myapp
  - FRAISEQL_ENABLE_QUERY_LOGGING=true
  - AUTH0_DOMAIN=myapp.auth0.com
```

## Debugging Configuration

View active configuration:

```python
from fraiseql.fastapi import FraiseQLConfig

# Load from environment
config = FraiseQLConfig()
print(config.model_dump())  # Shows all settings

# Check specific values
print(f"Playground enabled: {config.enable_playground}")
print(f"Environment: {config.environment}")
print(f"Database URL: {config.database_url[:20]}...")  # Truncate for security
```

## Security Considerations

1. **Never commit secrets**: Use environment variables for sensitive data
2. **Disable playground in production**: Set `production=True`
3. **Restrict CORS origins**: Don't use `["*"]` in production
4. **Use connection pooling**: Prevent database overload
5. **Enable HTTPS**: Always use HTTPS in production
6. **Rotate credentials**: Regularly update database passwords and API keys

## Next Steps

- Learn about [Authentication](./authentication.md) configuration
- Explore [Performance](./performance.md) optimization settings
- Review [Security](./security.md) best practices
