# Starlette Configuration Guide

**Version**: 2.0.0+
**Reading Time**: 25 minutes
**Audience**: Developers configuring Starlette servers
**Prerequisites**: Completed [Starlette Getting Started](./01-getting-started.md)

---

## Overview

This guide shows you how to configure your Starlette HTTP server for different scenarios:
- ✅ CORS and cross-origin requests
- ✅ Authentication (JWT, Bearer tokens)
- ✅ Rate limiting and request throttling
- ✅ Custom middleware
- ✅ Security hardening
- ✅ Production configuration

---

## Basic Configuration Structure

Starlette configuration uses a builder pattern with middleware layers:

```python
from starlette.applications import Starlette
from starlette.middleware import Middleware
from starlette.middleware.cors import CORSMiddleware
from starlette.middleware.authentication import AuthenticationMiddleware
from starlette.middleware.base import BaseHTTPMiddleware

# Define middleware stack
middleware = [
    Middleware(CORSMiddleware, allow_origins=["*"]),
    Middleware(AuthenticationMiddleware, backend=JWTBackend()),
    Middleware(RateLimitMiddleware),
]

# Create app with middleware
app = Starlette(
    routes=routes,
    middleware=middleware,
    debug=False
)
```

---

## CORS Configuration

CORS (Cross-Origin Resource Sharing) controls which domains can access your API.

### Minimal CORS Setup

**Allow single origin**:
```python
from starlette.middleware.cors import CORSMiddleware

app = Starlette(routes=routes)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["http://localhost:3000"],
    allow_credentials=True,
    allow_methods=["GET", "POST", "OPTIONS"],
    allow_headers=["Content-Type"],
)
```

### Production CORS Configuration

**Restrict to specific origins**:
```python
from starlette.middleware.cors import CORSMiddleware
import os

cors_origins = os.environ.get(
    "CORS_ORIGINS",
    "http://localhost:3000"
).split(",")

app.add_middleware(
    CORSMiddleware,
    allow_origins=cors_origins,  # List of allowed domains
    allow_credentials=True,
    allow_methods=["GET", "POST", "OPTIONS", "PUT", "DELETE"],
    allow_headers=[
        "Content-Type",
        "Authorization",
        "X-Requested-With",
    ],
    expose_headers=["X-Total-Count"],  # Expose to client
    max_age=600,  # Cache preflight for 10 minutes
)
```

### CORS with Environment Variables

**Load from config**:
```python
import os
from starlette.middleware.cors import CORSMiddleware

def setup_cors(app: Starlette):
    """Setup CORS based on environment"""
    allowed_origins = os.environ.get(
        "CORS_ORIGINS",
        "http://localhost:3000"
    )

    origins = [
        origin.strip()
        for origin in allowed_origins.split(",")
    ]

    app.add_middleware(
        CORSMiddleware,
        allow_origins=origins,
        allow_credentials=True,
        allow_methods=["*"],
        allow_headers=["*"],
    )

    return app

# Usage
app = setup_cors(app)
```

**Run with environment variables**:
```bash
CORS_ORIGINS="https://example.com,https://app.example.com" \
  uvicorn main:app
```

---

## Authentication Configuration

### JWT Token Validation

**Extract and validate JWT tokens**:
```python
import jwt
from starlette.authentication import (
    AuthenticationBackend,
    SimpleUser,
    AuthenticationError,
)
from starlette.middleware.authentication import AuthenticationMiddleware
from starlette.requests import Request
import os

class JWTBackend(AuthenticationBackend):
    async def authenticate(self, request: Request):
        if "Authorization" not in request.headers:
            return None

        auth_header = request.headers["Authorization"]

        try:
            scheme, token = auth_header.split()
            if scheme.lower() != "bearer":
                return None
        except ValueError:
            raise AuthenticationError("Invalid authorization header")

        try:
            secret = os.environ.get("JWT_SECRET")
            payload = jwt.decode(
                token,
                secret,
                algorithms=["HS256"],
            )
            return SimpleUser(payload.get("sub"))
        except jwt.InvalidTokenError:
            raise AuthenticationError("Invalid token")

# Add to app
app.add_middleware(AuthenticationMiddleware, backend=JWTBackend())
```

### Custom Claims Extraction

```python
from starlette.authentication import (
    AuthenticationBackend,
    AuthCredentials,
    SimpleUser,
)
from starlette.requests import Request
import jwt
import os

class CustomJWTBackend(AuthenticationBackend):
    async def authenticate(self, request: Request):
        if "Authorization" not in request.headers:
            return None

        auth_header = request.headers["Authorization"]

        try:
            scheme, token = auth_header.split()
            if scheme.lower() != "bearer":
                return None
        except ValueError:
            return None

        try:
            secret = os.environ.get("JWT_SECRET")
            payload = jwt.decode(
                token,
                secret,
                algorithms=["HS256"],
            )

            # Extract custom claims
            user_id = payload.get("sub")
            roles = payload.get("roles", [])
            email = payload.get("email")

            # Store in request.user for access in handlers
            return AuthCredentials(roles), SimpleUser(user_id)
        except jwt.InvalidTokenError:
            return None

# Use in handlers
async def protected_endpoint(request: Request):
    if not request.user.is_authenticated:
        return JSONResponse(
            {"error": "Unauthorized"},
            status_code=401
        )

    user_id = request.user.username
    return JSONResponse({"user_id": user_id})
```

### OAuth2 Integration

```python
from starlette.authentication import AuthenticationBackend, AuthCredentials, SimpleUser
from authlib.integrations.starlette_client import OAuth
import os

oauth = OAuth()
oauth.register(
    name="google",
    client_id=os.environ.get("GOOGLE_CLIENT_ID"),
    client_secret=os.environ.get("GOOGLE_CLIENT_SECRET"),
    server_metadata_url="https://accounts.google.com/.well-known/openid-configuration",
    client_kwargs={"scope": "openid email profile"},
)

async def google_login(request: Request):
    redirect_uri = request.url_for("google_callback")
    return await oauth.google.authorize_redirect(request, redirect_uri)

async def google_callback(request: Request):
    token = await oauth.google.authorize_access_token(request)
    user = await oauth.google.parse_id_token(request, token)
    # Store user info in session or create JWT
    return JSONResponse({"user": user})

# Add routes
from starlette.routing import Route
routes = [
    Route("/auth/google", google_login),
    Route("/auth/callback", google_callback),
]
```

---

## Rate Limiting

### Token Bucket Rate Limiter

**Limit requests per IP**:
```python
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.requests import Request
from starlette.responses import JSONResponse
from slowapi import Limiter
from slowapi.util import get_remote_address
from slowapi.errors import RateLimitExceeded

limiter = Limiter(key_func=get_remote_address)

class RateLimitMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request: Request, call_next):
        try:
            return await call_next(request)
        except RateLimitExceeded:
            return JSONResponse(
                {"error": "Rate limit exceeded"},
                status_code=429
            )

app.add_middleware(RateLimitMiddleware)

# Decorate endpoints with rate limits
@app.route("/graphql", methods=["POST"])
@limiter.limit("100/minute")
async def graphql_handler(request: Request):
    # Handle GraphQL query
    pass
```

### Per-Operation Rate Limiting

**Limit mutations more strictly than queries**:
```python
from starlette.requests import Request
from starlette.responses import JSONResponse
from typing import Dict
from slowapi import Limiter

limiter = Limiter(key_func=lambda r: r.client.host)

async def graphql_handler(request: Request):
    data = await request.json()

    # Determine operation type
    query = data.get("query", "")
    is_mutation = "mutation" in query.lower()

    if is_mutation:
        # Strict limit for mutations
        try:
            limiter.limit("10/minute")(lambda: None)()
        except RateLimitExceeded:
            return JSONResponse(
                {"error": "Mutation rate limit exceeded"},
                status_code=429
            )
    else:
        # Relaxed limit for queries
        try:
            limiter.limit("1000/minute")(lambda: None)()
        except RateLimitExceeded:
            return JSONResponse(
                {"error": "Query rate limit exceeded"},
                status_code=429
            )

    # Execute GraphQL query
    result = await schema.execute(
        query=data.get("query"),
        variable_values=data.get("variables"),
    )

    return JSONResponse({
        "data": result.data,
        "errors": result.errors,
    })
```

---

## Security Headers

### Add Security Headers Automatically

```python
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.requests import Request

class SecurityHeadersMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request: Request, call_next):
        response = await call_next(request)

        # Add security headers
        response.headers["Strict-Transport-Security"] = \
            "max-age=31536000; includeSubDomains"
        response.headers["X-Content-Type-Options"] = "nosniff"
        response.headers["X-Frame-Options"] = "DENY"
        response.headers["Content-Security-Policy"] = \
            "default-src 'self'"
        response.headers["Referrer-Policy"] = "strict-origin-when-cross-origin"
        response.headers["Permissions-Policy"] = \
            "accelerometer=(), camera=(), microphone=()"

        return response

app.add_middleware(SecurityHeadersMiddleware)
```

---

## Custom Middleware

### Create Custom Middleware

```python
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.requests import Request
import time
import logging

logger = logging.getLogger(__name__)

class LoggingMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request: Request, call_next):
        start = time.time()

        response = await call_next(request)

        elapsed = time.time() - start

        logger.info(
            f"{request.method} {request.url.path} - "
            f"{response.status_code} ({elapsed:.3f}s)"
        )

        return response

app.add_middleware(LoggingMiddleware)
```

### Request ID Middleware

```python
import uuid
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.requests import Request

class RequestIDMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request: Request, call_next):
        request_id = str(uuid.uuid4())
        request.state.request_id = request_id

        response = await call_next(request)

        response.headers["X-Request-ID"] = request_id

        return response

app.add_middleware(RequestIDMiddleware)
```

### Error Handling Middleware

```python
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.responses import JSONResponse
import logging

logger = logging.getLogger(__name__)

class ErrorHandlingMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request: Request, call_next):
        try:
            return await call_next(request)
        except ValueError as e:
            logger.error(f"Validation error: {e}")
            return JSONResponse(
                {"error": "Validation failed"},
                status_code=400
            )
        except Exception as e:
            logger.error(f"Unexpected error: {e}")
            return JSONResponse(
                {"error": "Internal server error"},
                status_code=500
            )

app.add_middleware(ErrorHandlingMiddleware)
```

---

## Response Compression

### Enable Compression

```python
from starlette.middleware.gzip import GZIPMiddleware

# Enable gzip compression for responses > 500 bytes
app.add_middleware(GZIPMiddleware, minimum_size=500)
```

### Compression with Multiple Formats

```python
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.requests import Request
import gzip

class CompressionMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request: Request, call_next):
        response = await call_next(request)

        # Check if client accepts gzip
        if "gzip" in request.headers.get("accept-encoding", "").lower():
            if len(response.body) > 500:  # Only compress if large
                response.body = gzip.compress(response.body)
                response.headers["Content-Encoding"] = "gzip"

        return response

app.add_middleware(CompressionMiddleware)
```

---

## Timeout Configuration

### Request Timeout

```python
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.responses import JSONResponse
import asyncio
import time

class TimeoutMiddleware(BaseHTTPMiddleware):
    def __init__(self, app, timeout_seconds: int = 30):
        super().__init__(app)
        self.timeout_seconds = timeout_seconds

    async def dispatch(self, request: Request, call_next):
        try:
            return await asyncio.wait_for(
                call_next(request),
                timeout=self.timeout_seconds
            )
        except asyncio.TimeoutError:
            return JSONResponse(
                {"error": "Request timeout"},
                status_code=504
            )

app.add_middleware(TimeoutMiddleware, timeout_seconds=30)
```

### Body Size Limits

```python
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.responses import JSONResponse
from starlette.requests import Request

class BodyLimitMiddleware(BaseHTTPMiddleware):
    def __init__(self, app, max_size: int = 1048576):  # 1MB
        super().__init__(app)
        self.max_size = max_size

    async def dispatch(self, request: Request, call_next):
        content_length = request.headers.get("content-length")
        if content_length and int(content_length) > self.max_size:
            return JSONResponse(
                {"error": "Request body too large"},
                status_code=413
            )
        return await call_next(request)

app.add_middleware(BodyLimitMiddleware, max_size=10485760)  # 10MB
```

---

## Production Configuration

### Complete Production Setup

```python
from starlette.applications import Starlette
from starlette.middleware import Middleware
from starlette.middleware.cors import CORSMiddleware
from starlette.middleware.gzip import GZIPMiddleware
from starlette.middleware.authentication import AuthenticationMiddleware
from starlette.routing import Route
import os
import logging

# Setup logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

# Authentication backend
class JWTBackend(AuthenticationBackend):
    # ... (implementation from above)
    pass

# Define middleware stack
middleware = [
    Middleware(GZIPMiddleware, minimum_size=500),
    Middleware(
        CORSMiddleware,
        allow_origins=os.environ.get(
            "CORS_ORIGINS",
            "http://localhost:3000"
        ).split(","),
        allow_credentials=True,
        allow_methods=["GET", "POST", "OPTIONS"],
        allow_headers=["Content-Type", "Authorization"],
    ),
    Middleware(AuthenticationMiddleware, backend=JWTBackend()),
]

# Routes
async def graphql_handler(request: Request):
    data = await request.json()
    result = await schema.execute(
        query=data.get("query"),
        variable_values=data.get("variables"),
    )
    return JSONResponse({
        "data": result.data,
        "errors": result.errors,
    })

async def health_check(request: Request):
    return JSONResponse({"status": "ok"})

routes = [
    Route("/graphql", graphql_handler, methods=["POST"]),
    Route("/health", health_check, methods=["GET"]),
]

# Create app
app = Starlette(
    routes=routes,
    middleware=middleware,
    debug=False
)
```

---

## Environment Variables

### Configuration via Environment

Create `.env` file:
```env
# Server
STARLETTE_HOST=0.0.0.0
STARLETTE_PORT=8000
STARLETTE_DEBUG=false

# Security
JWT_SECRET=your-secret-key-here
CORS_ORIGINS=https://example.com,https://app.example.com

# Database
DATABASE_URL=postgresql://user:pass@localhost/dbname
DATABASE_POOL_SIZE=20

# Application
MAX_REQUEST_BODY_SIZE=10485760
REQUEST_TIMEOUT=30

# Logging
LOG_LEVEL=info
```

Load in your code:
```python
import os
from dotenv import load_dotenv

load_dotenv()

# Read configuration
HOST = os.environ.get("STARLETTE_HOST", "127.0.0.1")
PORT = int(os.environ.get("STARLETTE_PORT", "8000"))
DEBUG = os.environ.get("STARLETTE_DEBUG", "false").lower() == "true"
JWT_SECRET = os.environ.get("JWT_SECRET")
DATABASE_URL = os.environ.get("DATABASE_URL")

print(f"Starting server on {HOST}:{PORT}")
```

---

## Common Configuration Scenarios

### Scenario 1: Public API

```python
# Allow public access with rate limiting
middleware = [
    Middleware(GZIPMiddleware, minimum_size=500),
    Middleware(
        CORSMiddleware,
        allow_origins=["*"],  # Allow any origin
        allow_methods=["*"],
        allow_headers=["*"],
    ),
]

# Aggressive rate limiting
@app.route("/graphql", methods=["POST"])
@limiter.limit("100/minute")
async def graphql_handler(request: Request):
    # Handle query
    pass
```

### Scenario 2: Internal API

```python
# Restrict to internal IPs
middleware = [
    Middleware(
        CORSMiddleware,
        allow_origins=["http://10.0.0.0/8"],  # Internal network
        allow_credentials=True,
    ),
    Middleware(AuthenticationMiddleware, backend=JWTBackend()),
]

# Larger body size for internal tools
@app.route("/graphql", methods=["POST"])
async def graphql_handler(request: Request):
    # Handle query - no strict rate limits needed
    pass
```

### Scenario 3: Mobile App Backend

```python
# Mobile-specific CORS
middleware = [
    Middleware(
        CORSMiddleware,
        allow_origins=[
            "http://localhost:8100",  # Ionic dev
            "capacitor://localhost",   # Mobile native
        ],
        allow_credentials=True,
    ),
    Middleware(AuthenticationMiddleware, backend=JWTBackend()),
]

# Token-based auth with moderate rate limiting
@app.route("/graphql", methods=["POST"])
@limiter.limit("50/minute")
async def graphql_handler(request: Request):
    # Handle query
    pass
```

---

## Verification Checklist

After configuring your server:

- [ ] CORS allows expected origins
- [ ] Authentication is working
- [ ] Rate limiting is in place
- [ ] Security headers are present
- [ ] Request timeouts configured
- [ ] Body size limits appropriate
- [ ] Logging is enabled
- [ ] Compression is active

**Test your configuration**:
```bash
# Check CORS
curl -X OPTIONS http://localhost:8000/graphql \
  -H "Origin: http://example.com" \
  -H "Access-Control-Request-Method: POST" \
  -v

# Check headers
curl -i http://localhost:8000/health

# Check rate limit
for i in {1..101}; do curl http://localhost:8000/graphql; done
```

---

## Next Steps

- **Ready to deploy?** → [Production Deployment](./03-deployment.md)
- **Need performance tuning?** → [Performance Tuning](./04-performance.md)
- **Something not working?** → [Troubleshooting](./05-troubleshooting.md)

---

## Quick Reference

| Configuration | Method | Example |
|---------------|--------|---------|
| CORS | `CORSMiddleware` | Allow origins |
| Auth | `AuthenticationMiddleware` | JWT validation |
| Rate limit | `@limiter.limit()` | 100/minute |
| Headers | Custom middleware | Security headers |
| Compression | `GZIPMiddleware` | Gzip compression |
| Timeout | Custom middleware | 30 seconds |
| Body limit | Custom middleware | 10MB |

---

**Your server is now configured!** Time to deploy? → [Production Deployment](./03-deployment.md)
