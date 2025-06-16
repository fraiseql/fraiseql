# FraiseQL Configuration

FraiseQL uses environment variables for configuration. All FraiseQL-specific environment variables must be prefixed with `FRAISEQL_` to avoid conflicts with other applications.

## Environment Variable Prefix

**Important**: FraiseQL only reads environment variables that start with `FRAISEQL_`. This prevents conflicts with common environment variables used by other applications.

For example:
- ✅ `FRAISEQL_DATABASE_URL` - Will be read by FraiseQL
- ❌ `DATABASE_URL` - Will be ignored
- ❌ `ENV` - Will be ignored
- ✅ `FRAISEQL_ENVIRONMENT` - Will be read by FraiseQL

## Configuration Options

### Required Settings

| Environment Variable | Description | Example |
|---------------------|-------------|---------|
| `FRAISEQL_DATABASE_URL` | PostgreSQL connection URL | `postgresql://user:pass@localhost/dbname` |

### Application Settings

| Environment Variable | Description | Default |
|---------------------|-------------|---------|
| `FRAISEQL_APP_NAME` | Application name | `"FraiseQL API"` |
| `FRAISEQL_APP_VERSION` | Application version | `"1.0.0"` |
| `FRAISEQL_ENVIRONMENT` | Environment mode | `"development"` |

Allowed values for `FRAISEQL_ENVIRONMENT`:
- `"development"` - Enables introspection and playground
- `"production"` - Disables introspection and playground for security
- `"testing"` - For test environments

### Database Settings

| Environment Variable | Description | Default |
|---------------------|-------------|---------|
| `FRAISEQL_DATABASE_POOL_SIZE` | Connection pool size | `20` |
| `FRAISEQL_DATABASE_MAX_OVERFLOW` | Max overflow connections | `10` |
| `FRAISEQL_DATABASE_POOL_TIMEOUT` | Pool timeout (seconds) | `30` |
| `FRAISEQL_DATABASE_ECHO` | Enable SQL logging | `false` |

### GraphQL Settings

| Environment Variable | Description | Default |
|---------------------|-------------|---------|
| `FRAISEQL_ENABLE_INTROSPECTION` | Enable GraphQL introspection | `true` (false in production) |
| `FRAISEQL_ENABLE_PLAYGROUND` | Enable GraphQL playground | `true` (false in production) |
| `FRAISEQL_MAX_QUERY_DEPTH` | Maximum query depth | `null` (unlimited) |
| `FRAISEQL_QUERY_TIMEOUT` | Query timeout (seconds) | `30` |
| `FRAISEQL_AUTO_CAMEL_CASE` | Convert snake_case to camelCase | `false` |

### Authentication Settings

| Environment Variable | Description | Default |
|---------------------|-------------|---------|
| `FRAISEQL_AUTH_ENABLED` | Enable authentication | `true` |
| `FRAISEQL_AUTH_PROVIDER` | Auth provider type | `"none"` |
| `FRAISEQL_DEV_AUTH_USERNAME` | Dev auth username | `"admin"` |
| `FRAISEQL_DEV_AUTH_PASSWORD` | Dev auth password | `null` |

Auth provider options:
- `"none"` - No authentication
- `"auth0"` - Auth0 authentication
- `"custom"` - Custom authentication provider

### Auth0 Settings (when using Auth0)

| Environment Variable | Description | Default |
|---------------------|-------------|---------|
| `FRAISEQL_AUTH0_DOMAIN` | Auth0 domain | `null` |
| `FRAISEQL_AUTH0_API_IDENTIFIER` | Auth0 API identifier | `null` |
| `FRAISEQL_AUTH0_ALGORITHMS` | JWT algorithms | `["RS256"]` |

### Performance Settings

| Environment Variable | Description | Default |
|---------------------|-------------|---------|
| `FRAISEQL_ENABLE_QUERY_CACHING` | Enable query result caching | `true` |
| `FRAISEQL_CACHE_TTL` | Cache TTL (seconds) | `300` |
| `FRAISEQL_ENABLE_QUERY_COMPILATION` | Enable query compilation | `false` |
| `FRAISEQL_COMPILED_QUERIES_PATH` | Path to compiled queries | `null` |

### CORS Settings

| Environment Variable | Description | Default |
|---------------------|-------------|---------|
| `FRAISEQL_CORS_ENABLED` | Enable CORS | `true` |
| `FRAISEQL_CORS_ORIGINS` | Allowed origins | `["*"]` |
| `FRAISEQL_CORS_METHODS` | Allowed methods | `["GET", "POST"]` |
| `FRAISEQL_CORS_HEADERS` | Allowed headers | `["*"]` |

## Example .env File

```bash
# Database
FRAISEQL_DATABASE_URL=postgresql://myuser:mypass@localhost/mydb

# Environment
FRAISEQL_ENVIRONMENT=development

# Application
FRAISEQL_APP_NAME=My GraphQL API
FRAISEQL_APP_VERSION=2.0.0

# GraphQL
FRAISEQL_AUTO_CAMEL_CASE=true
FRAISEQL_MAX_QUERY_DEPTH=10

# Development Auth
FRAISEQL_DEV_AUTH_PASSWORD=secret123

# Performance
FRAISEQL_DATABASE_POOL_SIZE=50
FRAISEQL_CACHE_TTL=600
```

## Using Existing Environment Variables

If you have existing environment variables without the `FRAISEQL_` prefix that you want to use, you have two options:

1. **Rename them** (Recommended):
   ```bash
   export FRAISEQL_DATABASE_URL=$DATABASE_URL
   ```

2. **Pass values directly to create_fraiseql_app**:
   ```python
   import os
   from fraiseql.fastapi import create_fraiseql_app
   
   app = create_fraiseql_app(
       database_url=os.getenv("DATABASE_URL"),  # Use existing var
       production=(os.getenv("ENV") == "production"),
   )
   ```

## Case Sensitivity

Environment variable names are case-insensitive. These are all equivalent:
- `FRAISEQL_DATABASE_URL`
- `fraiseql_database_url`
- `FrAiSeQL_DaTaBaSe_UrL`

## Extra Environment Variables

FraiseQL will ignore any environment variables that don't start with `FRAISEQL_` or aren't recognized configuration options. This prevents validation errors from unrelated environment variables in your system.