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
| `FRAISEQL_DATABASE_URL` | PostgreSQL connection URL | See [Database URL Formats](#database-url-formats) below |

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

## Database URL Formats

FraiseQL supports multiple PostgreSQL connection string formats:

### 1. Standard URL Format (Recommended)

```bash
postgresql://[user[:password]@][host][:port][/dbname][?param1=value1&...]
```

Examples:
```bash
# Basic connection
FRAISEQL_DATABASE_URL=postgresql://localhost/mydb

# With authentication
FRAISEQL_DATABASE_URL=postgresql://user:password@localhost/mydb

# With port and parameters
FRAISEQL_DATABASE_URL=postgresql://user:pass@host:5432/mydb?sslmode=require&connect_timeout=10

# Using postgres:// scheme (also supported)
FRAISEQL_DATABASE_URL=postgres://user@localhost/mydb
```

### 2. psycopg2 Connection String Format

FraiseQL also accepts psycopg2-style connection strings and automatically converts them:

```bash
"dbname='database' user='username' host='hostname' port='5432' password='password'"
```

Examples:
```bash
# Basic psycopg2 format
FRAISEQL_DATABASE_URL="dbname='mydb' user='myuser' host='localhost'"

# With password and port
FRAISEQL_DATABASE_URL="dbname='mydb' user='myuser' password='mypass' host='localhost' port='5432'"

# With additional parameters
FRAISEQL_DATABASE_URL="dbname='mydb' user='myuser' host='localhost' sslmode='require' connect_timeout='10'"
```

### 3. Automatic Format Detection

FraiseQL automatically detects and converts between formats:

```python
from fraiseql import create_fraiseql_app

# Both of these work identically:
app = create_fraiseql_app(
    database_url="postgresql://user@localhost/mydb"
)

app = create_fraiseql_app(
    database_url="dbname='mydb' user='user' host='localhost'"
)
```

### 4. Connection Parameters

Common PostgreSQL connection parameters:

| Parameter | Description | Example |
|-----------|-------------|---------|
| `sslmode` | SSL connection mode | `require`, `disable`, `prefer` |
| `connect_timeout` | Connection timeout in seconds | `10` |
| `application_name` | Application name for pg_stat_activity | `my_app` |
| `options` | Command-line options | `-c statement_timeout=5min` |

### 5. Special Characters in Passwords

If your password contains special characters:

**URL format**: URL-encode special characters
```bash
# Password: my@pass#word!
FRAISEQL_DATABASE_URL=postgresql://user:my%40pass%23word%21@localhost/mydb
```

**psycopg2 format**: Use quotes
```bash
FRAISEQL_DATABASE_URL="dbname='mydb' user='user' password='my@pass#word!' host='localhost'"
```

### 6. Connection Pool Configuration

Additional pool settings via environment variables:

```bash
# Connection pool size
FRAISEQL_DATABASE_POOL_SIZE=50

# Maximum overflow connections
FRAISEQL_DATABASE_MAX_OVERFLOW=10

# Pool timeout in seconds
FRAISEQL_DATABASE_POOL_TIMEOUT=30
```

### 7. Example Configurations

**Development**:
```bash
FRAISEQL_DATABASE_URL=postgresql://localhost/myapp_dev
```

**Production with SSL**:
```bash
FRAISEQL_DATABASE_URL=postgresql://user:pass@db.example.com:5432/myapp_prod?sslmode=require&connect_timeout=10
```

**Docker Compose**:
```bash
FRAISEQL_DATABASE_URL=postgresql://postgres:postgres@db:5432/myapp
```

**Cloud Providers**:
```bash
# Heroku (automatically set as DATABASE_URL)
FRAISEQL_DATABASE_URL=$DATABASE_URL

# AWS RDS
FRAISEQL_DATABASE_URL=postgresql://user:pass@mydb.abc123.us-east-1.rds.amazonaws.com:5432/myapp

# Google Cloud SQL
FRAISEQL_DATABASE_URL=postgresql://user:pass@/myapp?host=/cloudsql/project:region:instance
```