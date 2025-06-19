# Application

The main application factory and configuration for FraiseQL FastAPI applications.

## create_fraiseql_app

The primary function for creating a FraiseQL-powered GraphQL API.

```python
from fraiseql.fastapi import create_fraiseql_app

app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User, Post, Comment],
    mutations=[create_user, update_post],
    queries=[QueryRoot],
    production=False
)
```

### Parameters

#### Required Parameters

- **`database_url`** (str | None): PostgreSQL connection URL. If not provided, uses `DATABASE_URL` environment variable.
- **`types`** (Sequence[type]): List of `@fraiseql.type` decorated classes to include in the schema.

#### Schema Configuration

- **`mutations`** (Sequence[Callable]): List of mutation resolver functions.
- **`queries`** (Sequence[type]): List of query root types. If not provided, uses `types`.

#### Optional Configuration

- **`config`** (FraiseQLConfig | None): Complete configuration object. Overrides other parameters.
- **`auth`** (Auth0Config | AuthProvider | None): Authentication configuration.
- **`title`** (str | None): API title for documentation.
- **`version`** (str | None): API version.
- **`description`** (str | None): API description.
- **`production`** (bool): Enable production optimizations. Default: False.

#### Development Configuration

- **`dev_auth_username`** (str | None): Username for development authentication.
- **`dev_auth_password`** (str | None): Password for development authentication. Enables dev auth when set.

#### FastAPI Integration

- **`app`** (FastAPI | None): Existing FastAPI app to extend. Creates new app if None.

### Return Value

Returns a configured `FastAPI` application instance with:
- GraphQL endpoint at `/graphql`
- GraphQL Playground at `/playground` (development mode)
- Health check at `/health`
- CORS middleware (if enabled)
- Authentication middleware (if configured)

## GraphQL Endpoints

### /graphql

The main GraphQL endpoint that handles queries and mutations.

- **Methods**: GET, POST
- **Content-Type**: `application/json` for POST
- **Authentication**: Uses configured auth provider
- **Introspection**: Enabled in development, disabled in production

Example POST request:
```json
{
  "query": "query { users { id name } }",
  "variables": {},
  "operationName": null
}
```

### /playground

Interactive GraphQL Playground for exploring and testing your API.

- **Enabled by default in development mode**
- **Disabled by default in production mode**
- **URL**: `http://localhost:8000/playground`
- **Features**:
  - Interactive query editor with auto-completion
  - Schema documentation browser
  - Query history and tabs
  - Variable and header editors
  - Real-time syntax validation

To explicitly control playground availability:

```python
# Enable in production (not recommended)
app = create_fraiseql_app(
    types=[User],
    production=True,
    config=FraiseQLConfig(
        enable_playground=True,
        enable_introspection=True  # Required for playground
    )
)

# Disable in development
app = create_fraiseql_app(
    types=[User],
    config=FraiseQLConfig(enable_playground=False)
)
```

### /health

Simple health check endpoint for monitoring and load balancers.

Response:
```json
{
  "status": "healthy",
  "service": "fraiseql"
}
```

## Configuration

### FraiseQLConfig

Complete configuration object for fine-grained control.

```python
from fraiseql.fastapi import FraiseQLConfig

config = FraiseQLConfig(
    # Database
    database_url="postgresql://localhost/mydb",
    database_pool_size=20,
    database_pool_timeout=30,

    # Application
    app_name="My GraphQL API",
    app_version="1.0.0",
    environment="development",

    # GraphQL
    enable_playground=True,
    enable_introspection=True,
    graphql_path="/graphql",
    playground_path="/playground",

    # CORS
    cors_enabled=True,
    cors_origins=["http://localhost:3000"],
    cors_methods=["GET", "POST"],
    cors_headers=["Content-Type", "Authorization"],

    # Development
    dev_auth_username="admin",
    dev_auth_password="secret123"
)

app = create_fraiseql_app(types=[User], config=config)
```

### Environment Variables

All configuration can be set via environment variables:

```bash
# Database
export DATABASE_URL="postgresql://user:pass@localhost/db"

# GraphQL
export FRAISEQL_ENABLE_PLAYGROUND="true"
export FRAISEQL_ENABLE_INTROSPECTION="true"

# Application
export FRAISEQL_ENVIRONMENT="production"
export FRAISEQL_APP_NAME="My API"

# Development Auth
export FRAISEQL_DEV_AUTH_USERNAME="admin"
export FRAISEQL_DEV_AUTH_PASSWORD="secret"
```

## Usage Examples

### Basic Development Setup

```python
# Minimal setup for development
app = create_fraiseql_app(
    database_url="postgresql://localhost/dev_db",
    types=[User, Post]
)

# Run with uvicorn
if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
```

### Production Setup

```python
from fraiseql.auth import Auth0Config

app = create_fraiseql_app(
    types=[User, Post, Comment],
    mutations=[create_user, update_post],
    production=True,  # Disables playground, enables optimizations
    auth=Auth0Config(
        domain="myapp.auth0.com",
        api_identifier="https://api.myapp.com"
    )
)
```

### Extending Existing FastAPI App

```python
from fastapi import FastAPI

# Create your FastAPI app
existing_app = FastAPI(title="My App")

# Add custom routes
@existing_app.get("/")
def root():
    return {"message": "Welcome"}

# Add FraiseQL GraphQL endpoint
app = create_fraiseql_app(
    app=existing_app,  # Extend existing app
    types=[User, Post]
)
```

### Development with Authentication

```python
# Simple dev authentication
app = create_fraiseql_app(
    types=[User],
    dev_auth_password="dev123"  # Enables HTTP Basic Auth
)

# Access requires credentials:
# Username: admin (default)
# Password: dev123
```

## Advanced Features

### Production Mode Optimizations

When `production=True`:
- Disables GraphQL Playground
- Disables schema introspection
- Enables query compilation and caching
- Optimizes error messages (less detail)
- Disables development authentication

### Connection Pooling

```python
config = FraiseQLConfig(
    database_pool_size=50,      # Max connections
    database_pool_min_size=10,  # Min connections
    database_pool_timeout=30    # Timeout in seconds
)
```

### Custom Middleware

```python
app = create_fraiseql_app(types=[User])

# Add custom middleware
@app.middleware("http")
async def add_custom_header(request, call_next):
    response = await call_next(request)
    response.headers["X-Custom"] = "value"
    return response
```

## Best Practices

1. **Use production mode in production**: Set `production=True`
2. **Configure CORS properly**: Don't use `["*"]` origins in production
3. **Use environment variables**: For secrets and environment-specific config
4. **Enable authentication**: Use Auth0 or custom auth in production
5. **Monitor health endpoint**: Use `/health` for load balancer checks
6. **Use connection pooling**: Configure appropriate pool sizes

## Error Handling

FraiseQL provides different error formats based on environment:

**Development mode**:
```json
{
  "errors": [{
    "message": "Field 'users' not found on type Query",
    "extensions": {
      "code": "GRAPHQL_VALIDATION_FAILED",
      "stacktrace": ["..."]
    }
  }]
}
```

**Production mode**:
```json
{
  "errors": [{
    "message": "Internal server error",
    "extensions": {
      "code": "INTERNAL_SERVER_ERROR"
    }
  }]
}
```

## See Also

- [Configuration Guide](../advanced/configuration.md) - Detailed configuration options
- [Authentication](../advanced/authentication.md) - Setting up authentication
- [GraphQL Playground](../getting-started/graphql-playground.md) - Using the interactive playground
- [Configuration Guide](../advanced/configuration.md) - Detailed configuration options
