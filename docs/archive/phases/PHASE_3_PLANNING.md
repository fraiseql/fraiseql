# Phase 3: Custom Configuration & Advanced Features

**Status**: Planning

**Objective**: Enhance the Axum wrapper with production features (CORS configuration, middleware, API documentation, GraphQL playground).

---

## Overview

Phase 3 builds on the solid foundation of Phase 2 by adding:

1. **Custom CORS Configuration** - Production-ready CORS setup
2. **Middleware Pipeline** - Custom middleware support
3. **GraphQL Playground** - Built-in GraphQL IDE
4. **API Documentation** - OpenAPI/Swagger UI
5. **Advanced Configuration** - More granular control

---

## Feature 1: Custom CORS Configuration

### Current State (Phase 2)
- Axum server uses permissive CORS (allows all origins)
- Not suitable for production
- No customization possible

### Phase 3 Implementation

#### Configuration Extension
```python
class AxumFraiseQLConfig:
    # Existing fields...

    # CORS Configuration (Phase 3)
    cors_allow_origin: str | list[str] = Field(
        default="*",
        description="Allowed origins (string or list). Use '*' for all, or specific URLs"
    )
    cors_allow_credentials: bool = Field(default=True)
    cors_allow_methods: list[str] | None = Field(
        default=None,
        description="GET, POST, PUT, DELETE, OPTIONS. None = all standard methods"
    )
    cors_allow_headers: list[str] | None = Field(
        default=None,
        description="Custom headers to allow. None = standard headers"
    )
    cors_expose_headers: list[str] | None = Field(
        default=None,
        description="Headers exposed to client"
    )
    cors_max_age: int = Field(
        default=3600,
        description="How long (seconds) browser can cache preflight response"
    )
```

#### CORS Helper Module
```python
# src/fraiseql/axum/cors.py

class CORSConfig:
    """Production CORS configuration builder."""

    @staticmethod
    def permissive() -> dict:
        """Allow all origins (development only)."""
        return {"allow_origin": "*"}

    @staticmethod
    def from_urls(urls: list[str]) -> dict:
        """Create CORS from URL list."""
        # Validate and format URLs
        pass

    @staticmethod
    def production(domain: str) -> dict:
        """Production preset for single domain."""
        # E.g., "example.com" -> "https://example.com"
        pass

    @staticmethod
    def multi_tenant(domains: list[str]) -> dict:
        """Multi-domain production CORS."""
        pass
```

#### Usage Examples
```python
from fraiseql.axum import create_axum_fraiseql_app
from fraiseql.axum.cors import CORSConfig

# Development (permissive)
app = create_axum_fraiseql_app(
    database_url="...",
    types=[User],
    cors_config=CORSConfig.permissive()
)

# Production (single domain)
app = create_axum_fraiseql_app(
    database_url="...",
    types=[User],
    cors_config=CORSConfig.production("example.com")
)

# Production (multi-tenant)
app = create_axum_fraiseql_app(
    database_url="...",
    types=[User],
    cors_config=CORSConfig.multi_tenant([
        "app1.example.com",
        "app2.example.com"
    ])
)

# Custom
app = create_axum_fraiseql_app(
    database_url="...",
    types=[User],
    cors_allow_origin=["https://example.com", "https://app.example.com"],
    cors_max_age=7200,
    cors_allow_headers=["Authorization", "X-Custom-Header"]
)
```

### Tests Required
- CORS URL validation
- Multiple origin handling
- Max age calculation
- Header validation

---

## Feature 2: Middleware Pipeline

### Current State (Phase 2)
- Placeholder `add_middleware()` method
- No actual middleware support

### Phase 3 Implementation

#### Middleware Base Classes
```python
# src/fraiseql/axum/middleware.py

class AxumMiddleware:
    """Base class for Axum middleware."""

    async def process_request(self, request: Request) -> Request | None:
        """Process incoming request. Return None to block."""
        pass

    async def process_response(self, response: Response) -> Response:
        """Process outgoing response."""
        pass


class RequestLoggingMiddleware(AxumMiddleware):
    """Log all requests."""

    def __init__(self, log_body: bool = False):
        self.log_body = log_body

    async def process_request(self, request: Request) -> Request | None:
        logger.info(f"{request.method} {request.url}")
        if self.log_body:
            logger.debug(f"Body: {request.body}")
        return request


class AuthenticationMiddleware(AxumMiddleware):
    """Require authentication."""

    async def process_request(self, request: Request) -> Request | None:
        if "Authorization" not in request.headers:
            return None  # Block request
        return request


class RateLimitMiddleware(AxumMiddleware):
    """Rate limiting per IP."""

    def __init__(self, requests_per_minute: int = 100):
        self.limit = requests_per_minute


class CompressionMiddleware(AxumMiddleware):
    """Response compression."""

    def __init__(self, algorithm: str = "brotli", min_bytes: int = 256):
        self.algorithm = algorithm
        self.min_bytes = min_bytes
```

#### Middleware Registration
```python
app = create_axum_fraiseql_app(
    database_url="...",
    types=[User],
    middleware=[
        RequestLoggingMiddleware(log_body=True),
        AuthenticationMiddleware(),
        RateLimitMiddleware(requests_per_minute=1000),
    ]
)
```

### Tests Required
- Middleware execution order
- Request blocking
- Response modification
- Error handling in middleware

---

## Feature 3: GraphQL Playground

### Current State (Phase 2)
- No GraphQL IDE included
- Users must use external tools

### Phase 3 Implementation

#### Playground Configuration
```python
class PlaygroundConfig:
    """GraphQL Playground configuration."""

    enabled: bool = True
    path: str = "/playground"  # URL path
    title: str = "GraphQL Playground"
    subscriptions_endpoint: str | None = "/graphql/subscriptions"
    settings: dict | None = None
```

#### Built-in HTML Template
```html
<!-- Served at /playground -->
<!DOCTYPE html>
<html>
  <head>
    <meta charset="utf-8" />
    <title>GraphQL Playground</title>
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <link rel="stylesheet" href="//cdn.jsdelivr.net/npm/graphql-playground-react/build/static/css/index.css" />
    <script src="//cdn.jsdelivr.net/npm/graphql-playground-react/umd/graphql-playground.min.js"></script>
  </head>
  <body>
    <div id="root"></div>
    <script>
      window.addEventListener('load', function (event) {
        GraphQLPlayground.init(document.getElementById('root'), {
          endpoint: '/graphql',
          subscriptionEndpoint: '/graphql/subscriptions',
          settings: {...}
        })
      })
    </script>
  </body>
</html>
```

#### Usage
```python
from fraiseql.axum import PlaygroundConfig

app = create_axum_fraiseql_app(
    database_url="...",
    types=[User],
    playground_enabled=True,
    playground_path="/playground"
)

# Or with full config
app = create_axum_fraiseql_app(
    database_url="...",
    types=[User],
    playground=PlaygroundConfig(
        enabled=True,
        path="/gql",
        title="My API Playground"
    )
)
```

### Tests Required
- HTML generation
- Endpoint URL configuration
- Disable/enable toggle
- Custom settings

---

## Feature 4: OpenAPI/Swagger Documentation

### Current State (Phase 2)
- No API documentation
- /docs and /redoc endpoints return 404

### Phase 3 Implementation

#### OpenAPI Schema Generation
```python
# src/fraiseql/axum/openapi.py

class OpenAPIGenerator:
    """Generate OpenAPI schema from GraphQL."""

    def generate_schema(
        self,
        title: str,
        description: str,
        version: str,
        graphql_endpoint: str = "/graphql"
    ) -> dict:
        """Generate OpenAPI 3.0 schema."""

        # Convert GraphQL schema to OpenAPI
        # /graphql endpoint: POST
        # /graphql/subscriptions endpoint: WebSocket
        # Include query/mutation/subscription details
        pass
```

#### Swagger UI Endpoint
```python
# GET /docs -> Swagger UI (interactive)
# GET /redoc -> ReDoc UI (read-only)
# GET /openapi.json -> OpenAPI schema
```

#### Configuration
```python
class DocsConfig:
    """API documentation configuration."""

    enable_swagger: bool = True
    swagger_path: str = "/docs"

    enable_redoc: bool = True
    redoc_path: str = "/redoc"

    openapi_path: str = "/openapi.json"

    title: str = "FraiseQL API"
    description: str = ""
    version: str = "1.0.0"
```

#### Usage
```python
app = create_axum_fraiseql_app(
    database_url="...",
    types=[User],
    docs_enabled=True,
    title="My API",
    description="A GraphQL API",
    version="1.0.0"
)
```

### Tests Required
- Schema generation
- Endpoint serving
- Custom titles/descriptions
- Disable/enable toggle

---

## Feature 5: Advanced Configuration

### New Configuration Options

```python
class AxumFraiseQLConfig:
    # Existing fields...

    # Phase 3: Advanced features
    enable_playground: bool = True
    playground_path: str = "/playground"

    enable_swagger: bool = True
    swagger_path: str = "/docs"

    enable_redoc: bool = True
    redoc_path: str = "/redoc"

    openapi_path: str = "/openapi.json"

    # Request/Response
    max_request_body_size: int = Field(
        default=1000000,  # 1MB
        description="Maximum request body size in bytes"
    )

    request_timeout: int = Field(
        default=30,
        description="Request timeout in seconds"
    )

    # Logging
    log_requests: bool = Field(
        default=True,
        description="Log all GraphQL requests"
    )

    log_level: str = Field(
        default="INFO",
        pattern="^(DEBUG|INFO|WARNING|ERROR|CRITICAL)$"
    )

    # Security
    enable_introspection_in_production: bool = Field(
        default=False,
        description="Allow schema introspection in production"
    )

    require_https: bool = Field(
        default=False,
        description="Redirect HTTP to HTTPS"
    )
```

---

## Implementation Plan

### Phase 3A: CORS Configuration (1-2 days)
- [ ] Implement CORSConfig builder
- [ ] Update config validation
- [ ] Add CORS tests
- [ ] Update documentation

### Phase 3B: Middleware Pipeline (2-3 days)
- [ ] Implement middleware base class
- [ ] Create common middleware (logging, auth, rate limit)
- [ ] Integrate with AxumServer
- [ ] Add middleware tests

### Phase 3C: GraphQL Playground (1 day)
- [ ] Implement HTML template
- [ ] Add playground endpoint
- [ ] Configure via config
- [ ] Add tests

### Phase 3D: API Documentation (1-2 days)
- [ ] Generate OpenAPI schema
- [ ] Implement Swagger UI endpoint
- [ ] Implement ReDoc endpoint
- [ ] Add tests

### Phase 3E: Advanced Config (1 day)
- [ ] Add new config fields
- [ ] Implement validators
- [ ] Update documentation
- [ ] Add tests

### Phase 3F: Testing & Polish (1 day)
- [ ] Unit tests for all features
- [ ] Integration tests
- [ ] Examples
- [ ] Documentation

---

## Testing Strategy

### Unit Tests
- CORS configuration validation
- Middleware execution order
- Playground HTML generation
- OpenAPI schema generation
- Config field validation

### Integration Tests
- Full server with CORS
- Middleware in request flow
- Playground access
- OpenAPI endpoint access
- Documentation UIs

### Examples
- CORS for development vs production
- Custom middleware pipeline
- Playground customization
- API documentation setup

---

## Success Criteria

### CORS Configuration
- ✅ Flexible origin configuration
- ✅ Production-ready presets
- ✅ URL validation
- ✅ Comprehensive tests

### Middleware
- ✅ Easy to extend
- ✅ Common middleware provided
- ✅ Proper error handling
- ✅ Execution order guaranteed

### Playground & Docs
- ✅ Works out-of-the-box
- ✅ Customizable
- ✅ GraphQL introspection
- ✅ Beautiful UI

### Advanced Config
- ✅ All options validated
- ✅ Production defaults
- ✅ Development-friendly defaults
- ✅ Clear documentation

---

## Timeline

- **CORS Configuration**: 2 days
- **Middleware Pipeline**: 3 days
- **GraphQL Playground**: 1 day
- **API Documentation**: 2 days
- **Advanced Config**: 1 day
- **Testing & Polish**: 1 day

**Total**: ~10 days (could be parallelized for 5-6 days)

---

## Files to Create/Modify

### New Files
- `src/fraiseql/axum/cors.py` - CORS configuration
- `src/fraiseql/axum/middleware.py` - Middleware base classes
- `src/fraiseql/axum/playground.py` - GraphQL Playground
- `src/fraiseql/axum/openapi.py` - OpenAPI schema generation
- `src/fraiseql/axum/docs.py` - Documentation UI endpoints

### Modified Files
- `src/fraiseql/axum/config.py` - Add new config fields
- `src/fraiseql/axum/server.py` - Integrate features
- `src/fraiseql/axum/app.py` - Update factory function
- `src/fraiseql/axum/__init__.py` - Export new classes

### Tests
- `tests/unit/axum/test_cors.py`
- `tests/unit/axum/test_middleware.py`
- `tests/unit/axum/test_playground.py`
- `tests/unit/axum/test_openapi.py`

---

## Notes

- All features will maintain backward compatibility
- Sensible defaults for development (permissive CORS, playground enabled)
- Production-ready configuration available
- Extensive documentation and examples
- Full test coverage

---

**Next**: Begin Phase 3A (CORS Configuration)
