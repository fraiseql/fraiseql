# Configuration Reference

Complete configuration guide for FraiseQL applications covering all environment variables, settings, and deployment options.

## Overview

FraiseQL uses a type-safe configuration system built on Pydantic Settings. Configuration can be provided through environment variables, `.env` files, or directly in code. The configuration system supports both development and production environments with intelligent defaults and validation.

## Architecture

```mermaid
graph TD
    A[Environment Variables] --> D[FraiseQLConfig]
    B[.env File] --> D
    C[Code Configuration] --> D
    D --> E[App Creation]
    E --> F[Database Pool]
    E --> G[GraphQL Schema]
    E --> H[FastAPI App]
    E --> I[Auth Provider]
    E --> J[Middleware Stack]
```

## Configuration Methods

### Environment Variables

All configuration values can be set via environment variables prefixed with `FRAISEQL_`:

```bash
export FRAISEQL_DATABASE_URL="postgresql://user:pass@localhost/mydb"
export FRAISEQL_ENVIRONMENT="production"
export FRAISEQL_ENABLE_AUTH=true
export FRAISEQL_AUTH_PROVIDER="auth0"
export FRAISEQL_AUTH0_DOMAIN="myapp.auth0.com"
```

### .env File

Create a `.env` file in your project root:

```env
FRAISEQL_DATABASE_URL=postgresql://user:pass@localhost/mydb
FRAISEQL_ENVIRONMENT=production
FRAISEQL_DATABASE_POOL_SIZE=50
FRAISEQL_DATABASE_MAX_OVERFLOW=20
FRAISEQL_ENABLE_INTROSPECTION=false
```

### Code Configuration

```python
from fraiseql import FraiseQLConfig, create_fraiseql_app

config = FraiseQLConfig(
    database_url="postgresql://user:pass@localhost/mydb",
    environment="production",
    database_pool_size=50,
    database_max_overflow=20,
    enable_introspection=False,
    enable_playground=False,
    auth_enabled=True,
    auth_provider="auth0",
    auth0_domain="myapp.auth0.com",
    auth0_api_identifier="https://api.myapp.com"
)

app = create_fraiseql_app(types=[User, Post], config=config)
```

## Complete Configuration Reference

### Database Settings

| Parameter | Type | Default | Environment Variable | Description |
|-----------|------|---------|---------------------|-------------|
| `database_url` | str | **Required** | `FRAISEQL_DATABASE_URL` | PostgreSQL connection URL with JSONB support. Supports Unix sockets. |
| `database_pool_size` | int | 20 | `FRAISEQL_DATABASE_POOL_SIZE` | Maximum number of persistent connections in the pool |
| `database_max_overflow` | int | 10 | `FRAISEQL_DATABASE_MAX_OVERFLOW` | Maximum overflow connections beyond pool_size |
| `database_pool_timeout` | int | 30 | `FRAISEQL_DATABASE_POOL_TIMEOUT` | Seconds to wait before timing out when acquiring connection |
| `database_echo` | bool | False | `FRAISEQL_DATABASE_ECHO` | Enable SQL query logging (development only) |

#### Unix Socket Connections

FraiseQL supports PostgreSQL connections via Unix domain sockets:

```python
# Unix socket format
config = FraiseQLConfig(
    database_url="postgresql://user@/var/run/postgresql:5432/database"
)

# With password
config = FraiseQLConfig(
    database_url="postgresql://user:password@/var/run/postgresql:5432/database"
)
```

### Application Settings

| Parameter | Type | Default | Environment Variable | Description |
|-----------|------|---------|---------------------|-------------|
| `app_name` | str | "FraiseQL API" | `FRAISEQL_APP_NAME` | Application name shown in API documentation |
| `app_version` | str | "1.0.0" | `FRAISEQL_APP_VERSION` | Application version string |
| `environment` | str | "development" | `FRAISEQL_ENVIRONMENT` | Current environment: development, production, testing |

### GraphQL Settings

| Parameter | Type | Default | Environment Variable | Description |
|-----------|------|---------|---------------------|-------------|
| `enable_introspection` | bool | True* | `FRAISEQL_ENABLE_INTROSPECTION` | Allow GraphQL schema introspection queries |
| `enable_playground` | bool | True* | `FRAISEQL_ENABLE_PLAYGROUND` | Enable GraphQL playground IDE |
| `playground_tool` | str | "graphiql" | `FRAISEQL_PLAYGROUND_TOOL` | IDE to use: graphiql or apollo-sandbox |
| `max_query_depth` | int | None | `FRAISEQL_MAX_QUERY_DEPTH` | Maximum allowed query nesting depth |
| `query_timeout` | int | 30 | `FRAISEQL_QUERY_TIMEOUT` | Query execution timeout in seconds |
| `auto_camel_case` | bool | True | `FRAISEQL_AUTO_CAMEL_CASE` | Convert snake_case to camelCase in GraphQL |

*Automatically disabled in production unless explicitly enabled

### Performance Settings

| Parameter | Type | Default | Environment Variable | Description |
|-----------|------|---------|---------------------|-------------|
| `enable_turbo_router` | bool | True | `FRAISEQL_ENABLE_TURBO_ROUTER` | Enable TurboRouter for registered queries |
| `turbo_router_cache_size` | int | 1000 | `FRAISEQL_TURBO_ROUTER_CACHE_SIZE` | Maximum number of queries to cache |
| `turbo_max_complexity` | int | 100 | `FRAISEQL_TURBO_MAX_COMPLEXITY` | Max complexity score for turbo caching |
| `turbo_max_total_weight` | float | 2000.0 | `FRAISEQL_TURBO_MAX_TOTAL_WEIGHT` | Max total weight of cached queries |
| `turbo_enable_adaptive_caching` | bool | True | `FRAISEQL_TURBO_ENABLE_ADAPTIVE_CACHING` | Enable complexity-based cache admission |
| `enable_query_caching` | bool | True | `FRAISEQL_ENABLE_QUERY_CACHING` | Enable general query result caching |
| `cache_ttl` | int | 300 | `FRAISEQL_CACHE_TTL` | Cache time-to-live in seconds |

### JSON Passthrough Optimization

| Parameter | Type | Default | Environment Variable | Description |
|-----------|------|---------|---------------------|-------------|
| `json_passthrough_enabled` | bool | True | `FRAISEQL_JSON_PASSTHROUGH_ENABLED` | Enable JSON passthrough optimization |
| `json_passthrough_in_production` | bool | True | `FRAISEQL_JSON_PASSTHROUGH_IN_PRODUCTION` | Auto-enable in production mode |
| `json_passthrough_cache_nested` | bool | True | `FRAISEQL_JSON_PASSTHROUGH_CACHE_NESTED` | Cache wrapped nested objects |
| `passthrough_complexity_limit` | int | 50 | `FRAISEQL_PASSTHROUGH_COMPLEXITY_LIMIT` | Max complexity for passthrough mode |
| `passthrough_max_depth` | int | 3 | `FRAISEQL_PASSTHROUGH_MAX_DEPTH` | Max depth for passthrough queries |

### JSONB Extraction

| Parameter | Type | Default | Environment Variable | Description |
|-----------|------|---------|---------------------|-------------|
| `jsonb_extraction_enabled` | bool | True | `FRAISEQL_JSONB_EXTRACTION_ENABLED` | Enable automatic JSONB column extraction |
| `jsonb_default_columns` | list | ["data", "json_data", "jsonb_data"] | `FRAISEQL_JSONB_DEFAULT_COLUMNS` | Default JSONB column names to search |
| `jsonb_auto_detect` | bool | True | `FRAISEQL_JSONB_AUTO_DETECT` | Auto-detect JSONB columns by content |
| `jsonb_field_limit_threshold` | int | 20 | `FRAISEQL_JSONB_FIELD_LIMIT_THRESHOLD` | Field count threshold for full data column |

### Authentication Settings

| Parameter | Type | Default | Environment Variable | Description |
|-----------|------|---------|---------------------|-------------|
| `auth_enabled` | bool | True | `FRAISEQL_AUTH_ENABLED` | Enable authentication system |
| `auth_provider` | str | "none" | `FRAISEQL_AUTH_PROVIDER` | Provider: auth0, custom, none |
| `auth0_domain` | str | None | `FRAISEQL_AUTH0_DOMAIN` | Auth0 tenant domain |
| `auth0_api_identifier` | str | None | `FRAISEQL_AUTH0_API_IDENTIFIER` | Auth0 API identifier |
| `auth0_algorithms` | list | ["RS256"] | `FRAISEQL_AUTH0_ALGORITHMS` | JWT signing algorithms |
| `dev_auth_username` | str | "admin" | `FRAISEQL_DEV_AUTH_USERNAME` | Development auth username |
| `dev_auth_password` | str | None | `FRAISEQL_DEV_AUTH_PASSWORD` | Development auth password |

### Token Revocation

| Parameter | Type | Default | Environment Variable | Description |
|-----------|------|---------|---------------------|-------------|
| `revocation_enabled` | bool | True | `FRAISEQL_REVOCATION_ENABLED` | Enable token revocation system |
| `revocation_check_enabled` | bool | True | `FRAISEQL_REVOCATION_CHECK_ENABLED` | Check for revoked tokens |
| `revocation_ttl` | int | 86400 | `FRAISEQL_REVOCATION_TTL` | Revocation TTL in seconds (24h) |
| `revocation_cleanup_interval` | int | 3600 | `FRAISEQL_REVOCATION_CLEANUP_INTERVAL` | Cleanup interval (1h) |
| `revocation_store_type` | str | "memory" | `FRAISEQL_REVOCATION_STORE_TYPE` | Store type: memory or redis |

### Query Complexity

| Parameter | Type | Default | Environment Variable | Description |
|-----------|------|---------|---------------------|-------------|
| `complexity_enabled` | bool | True | `FRAISEQL_COMPLEXITY_ENABLED` | Enable query complexity analysis |
| `complexity_max_score` | int | 1000 | `FRAISEQL_COMPLEXITY_MAX_SCORE` | Maximum allowed complexity score |
| `complexity_max_depth` | int | 10 | `FRAISEQL_COMPLEXITY_MAX_DEPTH` | Maximum query depth |
| `complexity_default_list_size` | int | 10 | `FRAISEQL_COMPLEXITY_DEFAULT_LIST_SIZE` | Assumed list size for complexity |
| `complexity_include_in_response` | bool | False | `FRAISEQL_COMPLEXITY_INCLUDE_IN_RESPONSE` | Include complexity in response |
| `complexity_field_multipliers` | dict | {} | `FRAISEQL_COMPLEXITY_FIELD_MULTIPLIERS` | Field-specific multipliers |

### Rate Limiting

| Parameter | Type | Default | Environment Variable | Description |
|-----------|------|---------|---------------------|-------------|
| `rate_limit_enabled` | bool | True | `FRAISEQL_RATE_LIMIT_ENABLED` | Enable rate limiting |
| `rate_limit_requests_per_minute` | int | 60 | `FRAISEQL_RATE_LIMIT_REQUESTS_PER_MINUTE` | Requests per minute limit |
| `rate_limit_requests_per_hour` | int | 1000 | `FRAISEQL_RATE_LIMIT_REQUESTS_PER_HOUR` | Requests per hour limit |
| `rate_limit_burst_size` | int | 10 | `FRAISEQL_RATE_LIMIT_BURST_SIZE` | Burst size for rate limiting |
| `rate_limit_window_type` | str | "sliding" | `FRAISEQL_RATE_LIMIT_WINDOW_TYPE` | Window type: sliding or fixed |
| `rate_limit_whitelist` | list | [] | `FRAISEQL_RATE_LIMIT_WHITELIST` | IP addresses to whitelist |
| `rate_limit_blacklist` | list | [] | `FRAISEQL_RATE_LIMIT_BLACKLIST` | IP addresses to blacklist |

### CORS Settings

| Parameter | Type | Default | Environment Variable | Description |
|-----------|------|---------|---------------------|-------------|
| `cors_enabled` | bool | True | `FRAISEQL_CORS_ENABLED` | Enable CORS middleware |
| `cors_origins` | list | ["*"] | `FRAISEQL_CORS_ORIGINS` | Allowed origins |
| `cors_methods` | list | ["GET", "POST"] | `FRAISEQL_CORS_METHODS` | Allowed HTTP methods |
| `cors_headers` | list | ["*"] | `FRAISEQL_CORS_HEADERS` | Allowed headers |

### Execution Mode

| Parameter | Type | Default | Environment Variable | Description |
|-----------|------|---------|---------------------|-------------|
| `unified_executor_enabled` | bool | True | `FRAISEQL_UNIFIED_EXECUTOR_ENABLED` | Enable unified query executor |
| `execution_mode_priority` | list | ["turbo", "passthrough", "normal"] | `FRAISEQL_EXECUTION_MODE_PRIORITY` | Mode priority order |
| `include_execution_metadata` | bool | False | `FRAISEQL_INCLUDE_EXECUTION_METADATA` | Include execution metadata |
| `execution_timeout_ms` | int | 30000 | `FRAISEQL_EXECUTION_TIMEOUT_MS` | Execution timeout in milliseconds |
| `enable_mode_hints` | bool | True | `FRAISEQL_ENABLE_MODE_HINTS` | Enable query mode hints |

## Development vs Production

### Development Configuration

```python
# Development optimized for debugging and fast iteration
config = FraiseQLConfig(
    database_url="postgresql://dev:dev@localhost/dev_db",
    environment="development",
    database_echo=True,  # See SQL queries
    enable_introspection=True,  # Allow schema exploration
    enable_playground=True,  # GraphQL IDE
    auth_enabled=False,  # Simplified auth
    complexity_include_in_response=True,  # Debug complexity
)
```

### Production Configuration

```python
# Production optimized for security and performance
config = FraiseQLConfig(
    database_url=os.getenv("DATABASE_URL"),
    environment="production",
    database_pool_size=50,
    database_max_overflow=20,
    database_echo=False,
    enable_introspection=False,  # Hide schema
    enable_playground=False,  # No IDE
    auth_enabled=True,
    auth_provider="auth0",
    auth0_domain=os.getenv("AUTH0_DOMAIN"),
    auth0_api_identifier=os.getenv("AUTH0_API_IDENTIFIER"),
    rate_limit_enabled=True,
    rate_limit_requests_per_minute=100,
    complexity_max_score=500,
    turbo_router_cache_size=5000,
)
```

## Docker Configuration

### Dockerfile Example

```dockerfile
FROM python:3.11-slim

# Set environment for production
ENV FRAISEQL_ENVIRONMENT=production
ENV FRAISEQL_DATABASE_POOL_SIZE=30
ENV FRAISEQL_ENABLE_INTROSPECTION=false
ENV FRAISEQL_ENABLE_PLAYGROUND=false

WORKDIR /app
COPY requirements.txt .
RUN pip install -r requirements.txt
COPY . .

CMD ["uvicorn", "main:app", "--host", "0.0.0.0", "--port", "8000"]
```

### Docker Compose

```yaml
version: '3.8'

services:
  api:
    build: .
    environment:
      FRAISEQL_DATABASE_URL: postgresql://fraiseql:password@db:5432/fraiseql
      FRAISEQL_ENVIRONMENT: production
      FRAISEQL_DATABASE_POOL_SIZE: 30
      FRAISEQL_AUTH0_DOMAIN: ${AUTH0_DOMAIN}
      FRAISEQL_AUTH0_API_IDENTIFIER: ${AUTH0_API_IDENTIFIER}
    depends_on:
      - db
    ports:
      - "8000:8000"

  db:
    image: postgres:15
    environment:
      POSTGRES_DB: fraiseql
      POSTGRES_USER: fraiseql
      POSTGRES_PASSWORD: password
    volumes:
      - postgres_data:/var/lib/postgresql/data

volumes:
  postgres_data:
```

## Kubernetes Configuration

### ConfigMap

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: fraiseql-config
data:
  FRAISEQL_ENVIRONMENT: "production"
  FRAISEQL_DATABASE_POOL_SIZE: "50"
  FRAISEQL_ENABLE_INTROSPECTION: "false"
  FRAISEQL_ENABLE_PLAYGROUND: "false"
  FRAISEQL_RATE_LIMIT_ENABLED: "true"
  FRAISEQL_COMPLEXITY_MAX_SCORE: "1000"
```

### Secret

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: fraiseql-secrets
type: Opaque
stringData:
  FRAISEQL_DATABASE_URL: "postgresql://user:pass@postgres:5432/db"
  FRAISEQL_AUTH0_DOMAIN: "myapp.auth0.com"
  FRAISEQL_AUTH0_API_IDENTIFIER: "https://api.myapp.com"
```

### Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql-api
spec:
  replicas: 3
  template:
    spec:
      containers:
      - name: api
        image: myapp/fraiseql:latest
        envFrom:
        - configMapRef:
            name: fraiseql-config
        - secretRef:
            name: fraiseql-secrets
        resources:
          requests:
            memory: "256Mi"
            cpu: "250m"
          limits:
            memory: "512Mi"
            cpu: "500m"
```

## Performance Tuning

### Database Connection Pool

Optimal pool size depends on your workload:

```python
# Calculate optimal pool size
# pool_size = (number_of_workers × average_connections_per_worker)
# Example: 4 workers × 10 connections = 40

config = FraiseQLConfig(
    database_pool_size=40,  # Core connections
    database_max_overflow=20,  # Burst capacity
    database_pool_timeout=10,  # Fail fast on overload
)
```

### Query Optimization

```python
# Enable all performance features
config = FraiseQLConfig(
    # TurboRouter for repeated queries
    enable_turbo_router=True,
    turbo_router_cache_size=5000,
    turbo_max_complexity=200,

    # JSON passthrough for simple queries
    json_passthrough_enabled=True,
    passthrough_complexity_limit=100,

    # JSONB extraction for large objects
    jsonb_extraction_enabled=True,
    jsonb_field_limit_threshold=15,

    # General caching
    enable_query_caching=True,
    cache_ttl=600,  # 10 minutes
)
```

### Memory Management

```python
# Conservative memory usage
config = FraiseQLConfig(
    database_pool_size=20,  # Fewer connections
    turbo_router_cache_size=500,  # Smaller cache
    complexity_max_score=500,  # Limit query complexity
    rate_limit_requests_per_minute=30,  # Throttle requests
)
```

## Configuration Validation

FraiseQL validates configuration at startup:

```python
from fraiseql import FraiseQLConfig
from pydantic import ValidationError

try:
    config = FraiseQLConfig(
        database_url="invalid-url",  # Will fail validation
        environment="production",
        auth_provider="auth0",  # Requires auth0_domain
    )
except ValidationError as e:
    print(f"Configuration error: {e}")
    # Configuration error:
    # - database_url: Invalid PostgreSQL URL
    # - auth0_domain: Required when using Auth0 provider
```

## Environment-Specific Overrides

```python
import os
from fraiseql import FraiseQLConfig

# Base configuration
config_defaults = {
    "database_pool_size": 20,
    "enable_query_caching": True,
    "cache_ttl": 300,
}

# Environment-specific overrides
if os.getenv("ENVIRONMENT") == "production":
    config_defaults.update({
        "database_pool_size": 50,
        "enable_introspection": False,
        "enable_playground": False,
        "rate_limit_enabled": True,
    })
elif os.getenv("ENVIRONMENT") == "staging":
    config_defaults.update({
        "database_pool_size": 30,
        "enable_introspection": True,  # Allow debugging
        "enable_playground": False,
    })

config = FraiseQLConfig(**config_defaults)
```

## Best Practices

### 1. Use Environment Variables for Secrets

Never hardcode sensitive values:

```python
# ❌ Bad
config = FraiseQLConfig(
    database_url="postgresql://user:password@localhost/db"
)

# ✅ Good
config = FraiseQLConfig(
    database_url=os.getenv("DATABASE_URL")
)
```

### 2. Validate Configuration Early

```python
def validate_production_config(config: FraiseQLConfig):
    """Ensure production configuration is secure."""
    assert config.environment == "production"
    assert not config.enable_introspection
    assert not config.enable_playground
    assert config.auth_enabled
    assert config.rate_limit_enabled
    assert config.database_pool_size >= 20
```

### 3. Monitor Configuration Changes

```python
import logging

logger = logging.getLogger(__name__)

def log_configuration(config: FraiseQLConfig):
    """Log non-sensitive configuration for debugging."""
    logger.info(f"Environment: {config.environment}")
    logger.info(f"Database pool size: {config.database_pool_size}")
    logger.info(f"Auth enabled: {config.auth_enabled}")
    logger.info(f"Rate limiting: {config.rate_limit_enabled}")
    # Never log sensitive values like database_url
```

### 4. Use Configuration Profiles

```python
# config_profiles.py
PROFILES = {
    "development": {
        "database_echo": True,
        "enable_introspection": True,
        "enable_playground": True,
        "auth_enabled": False,
    },
    "staging": {
        "database_echo": False,
        "enable_introspection": True,
        "enable_playground": False,
        "auth_enabled": True,
    },
    "production": {
        "database_echo": False,
        "enable_introspection": False,
        "enable_playground": False,
        "auth_enabled": True,
        "rate_limit_enabled": True,
    }
}

# Usage
profile = os.getenv("CONFIG_PROFILE", "development")
config = FraiseQLConfig(**PROFILES[profile])
```

## Common Pitfalls

### Pitfall: Introspection Enabled in Production
**Solution**: FraiseQL automatically disables introspection in production unless explicitly enabled.

### Pitfall: Database Pool Exhaustion
**Solution**: Monitor pool usage and increase `database_pool_size` and `database_max_overflow`.

### Pitfall: Missing Auth Configuration
**Solution**: FraiseQL validates Auth0 configuration when `auth_provider="auth0"`.

### Pitfall: Slow Queries Without Caching
**Solution**: Enable TurboRouter and query caching for frequently accessed data.

## Troubleshooting

### Error: Database Connection Failed
```python
# Check connection string format
# Standard: postgresql://user:pass@host:5432/database
# Unix socket: postgresql://user@/var/run/postgresql:5432/database

# Test connection
import psycopg
conn = psycopg.connect(database_url)
```

### Error: Configuration Validation Failed
```python
# Enable debug logging
import logging
logging.basicConfig(level=logging.DEBUG)

# Validate configuration step by step
config = FraiseQLConfig.model_validate({
    "database_url": "...",
    # Add fields one by one
})
```

### Error: Pool Timeout
```python
# Increase pool size or timeout
config = FraiseQLConfig(
    database_pool_size=50,  # More connections
    database_pool_timeout=60,  # Wait longer
    database_max_overflow=30,  # More burst capacity
)
```

## See Also

- [Security Guide](./security.md) - Security configuration best practices
- [Authentication Patterns](./authentication.md) - Auth provider configuration
- [Performance Guide](./performance.md) - Performance tuning parameters
- [TurboRouter](./turbo-router.md) - Query caching configuration
- [Database Views](../core-concepts/database-views.md) - Database setup requirements
