# Setup Guide - FraiseQL Native Authentication

This guide walks you through setting up FraiseQL's native authentication system from scratch.

## Prerequisites

- **Python 3.11+** (Python 3.13 recommended)
- **PostgreSQL 15+** (PostgreSQL 16 recommended)  
- **FraiseQL v0.1.0b31+**

## Installation

### 1. Install FraiseQL with Auth Support

```bash
# Install with authentication dependencies
pip install "fraiseql[auth]>=0.1.0b31"

# Or if you already have FraiseQL installed
pip install --upgrade "fraiseql>=0.1.0b31"
```

### 2. Verify Installation

```python
# Verify native auth components are available
from fraiseql.auth.native import (
    create_native_auth_provider,
    apply_native_auth_schema,
    get_native_auth_router
)
print("✅ FraiseQL native auth installed successfully!")
```

## Database Setup

### 1. Create Database

```bash
# PostgreSQL command line
createdb your_app_db

# Or using psql
psql -c "CREATE DATABASE your_app_db;"
```

### 2. Set Database URL

```bash
# Environment variable (recommended)
export DATABASE_URL="postgresql://username:password@localhost:5432/your_app_db"

# Or create .env file
echo "DATABASE_URL=postgresql://username:password@localhost:5432/your_app_db" > .env
```

### 3. Apply Authentication Schema

Create a migration script to set up the required tables:

```python
# migrate.py
import asyncio
import os
from psycopg_pool import AsyncConnectionPool
from fraiseql.auth.native import apply_native_auth_schema

async def migrate():
    database_url = os.environ.get("DATABASE_URL")
    if not database_url:
        raise ValueError("DATABASE_URL environment variable required")
    
    # Create connection pool
    pool = AsyncConnectionPool(database_url)
    
    try:
        # Apply native auth schema
        await apply_native_auth_schema(pool)
        print("✅ Database schema applied successfully!")
    finally:
        await pool.close()

if __name__ == "__main__":
    asyncio.run(migrate())
```

Run the migration:

```bash
python migrate.py
```

### 4. Verify Database Schema

Connect to your database and verify the tables were created:

```bash
psql your_app_db -c "\\dt"
```

You should see these tables:
- `tb_user` - User accounts  
- `tb_session` - Active sessions
- `tb_used_refresh_token` - Token replay prevention
- `tb_password_reset` - Password reset tokens
- `tb_auth_audit` - Authentication event log

## Environment Configuration

### 1. Required Environment Variables

```bash
# JWT secret key (required)
export JWT_SECRET_KEY="your-super-secure-secret-key-change-in-production"

# Database connection (required)
export DATABASE_URL="postgresql://username:password@localhost:5432/your_app_db"
```

### 2. Optional Configuration

```bash
# Token lifetimes (optional - these are defaults)
export ACCESS_TOKEN_TTL_MINUTES=15
export REFRESH_TOKEN_TTL_DAYS=30

# Security settings (optional - these are defaults)
export ENABLE_RATE_LIMITING=true
export RATE_LIMIT_REQUESTS_PER_MINUTE=60
export RATE_LIMIT_AUTH_REQUESTS_PER_MINUTE=5

# Multi-tenant settings (optional)
export DEFAULT_SCHEMA=public
```

### 3. Generate Secure JWT Secret

**Important**: Never use a weak secret in production!

```python
import secrets

# Generate a cryptographically secure secret (recommended)
secret = secrets.token_urlsafe(64)
print(f"JWT_SECRET_KEY={secret}")
```

Or use command line:

```bash
# Generate 64-character random string
python -c "import secrets; print(f'JWT_SECRET_KEY={secrets.token_urlsafe(64)}')"
```

## Application Integration

### 1. Basic FastAPI Integration

```python
# main.py
import os
from contextlib import asynccontextmanager
from fastapi import FastAPI
from psycopg_pool import AsyncConnectionPool
from fraiseql.auth.native import (
    create_native_auth_provider,
    get_native_auth_router,
    add_security_middleware
)
from fraiseql.fastapi.app import create_fraiseql_app

# Global connection pool
db_pool = None

@asynccontextmanager
async def lifespan(app: FastAPI):
    global db_pool
    
    # Startup
    database_url = os.environ.get("DATABASE_URL")
    db_pool = AsyncConnectionPool(database_url)
    
    # Create auth provider
    auth_provider = await create_native_auth_provider(
        db_pool,
        secret_key=os.environ.get("JWT_SECRET_KEY")
    )
    
    # Store in app state
    app.state.db_pool = db_pool
    app.state.auth_provider = auth_provider
    
    yield
    
    # Shutdown
    await db_pool.close()

# Create FastAPI application
app = FastAPI(
    title="Your App with FraiseQL Auth",
    lifespan=lifespan
)

# Add authentication endpoints
auth_router = get_native_auth_router()
app.include_router(auth_router, prefix="/auth", tags=["Authentication"])

# Add security middleware
add_security_middleware(
    app,
    secret_key=os.environ.get("JWT_SECRET_KEY"),
    enable_rate_limiting=True,
    enable_security_headers=True
)

# Your application routes
@app.get("/")
async def root():
    return {"message": "Hello World with FraiseQL Native Auth!"}

# Protected endpoint example
@app.get("/protected")
async def protected_endpoint(request):
    # Extract user from JWT token
    auth_header = request.headers.get("authorization")
    if not auth_header or not auth_header.startswith("Bearer "):
        return {"error": "Authentication required"}
    
    token = auth_header.split(" ")[1]
    try:
        user = await request.app.state.auth_provider.get_user_from_token(token)
        return {"message": f"Hello {user.name}!", "user_id": user.user_id}
    except Exception as e:
        return {"error": "Invalid token"}
```

### 2. With FraiseQL GraphQL Integration

```python
# main.py with GraphQL
from fraiseql.fastapi.app import create_fraiseql_app
from fraiseql.auth.decorators import requires_auth

# Create FraiseQL app with auth provider
app = create_fraiseql_app(
    connection_pool=db_pool,
    auth_provider=auth_provider,  # Your native auth provider
    debug=True
)

# Add auth routes
auth_router = get_native_auth_router()
app.include_router(auth_router, prefix="/auth")

# GraphQL types with auth
@fraiseql.type
class User:
    id: UUID
    email: str
    name: str

@fraiseql.query
@requires_auth  # This will use your native auth provider
async def current_user(info) -> User:
    user_context = info.context["user"]  # Provided by native auth
    return User(
        id=user_context.user_id,
        email=user_context.email,
        name=user_context.name
    )
```

### 3. Multi-Tenant Setup

```python
# Multi-tenant configuration
async def create_tenant_auth_provider(tenant_id: str):
    schema = f"tenant_{tenant_id}"
    
    # Apply schema for new tenant
    await apply_native_auth_schema(db_pool, schema=schema)
    
    # Create tenant-specific provider
    return await create_native_auth_provider(
        db_pool,
        schema=schema,
        secret_key=os.environ.get("JWT_SECRET_KEY")
    )

# Middleware to resolve tenant
@app.middleware("http")
async def tenant_middleware(request, call_next):
    # Extract tenant from subdomain, header, or token
    tenant_id = extract_tenant_id(request)
    if tenant_id:
        request.state.tenant_id = tenant_id
        # You could set tenant-specific auth provider here
    
    response = await call_next(request)
    return response
```

## Testing the Setup

### 1. Start the Application

```bash
# Development server
uvicorn main:app --reload --port 8000

# Production server
gunicorn main:app -w 4 -k uvicorn.workers.UvicornWorker --bind 0.0.0.0:8000
```

### 2. Test User Registration

```bash
curl -X POST http://localhost:8000/auth/register \
  -H "Content-Type: application/json" \
  -d '{
    "email": "test@example.com",
    "password": "TestPassword123!",
    "name": "Test User"
  }'
```

Expected response:
```json
{
  "user": {
    "id": "123e4567-e89b-12d3-a456-426614174000",
    "email": "test@example.com", 
    "name": "Test User",
    "roles": [],
    "permissions": [],
    "is_active": true,
    "email_verified": false,
    "created_at": "2025-01-22T10:00:00Z"
  },
  "access_token": "eyJ...",
  "refresh_token": "eyJ...",
  "token_type": "bearer",
  "expires_at": "2025-01-22T10:15:00Z"
}
```

### 3. Test User Login

```bash
curl -X POST http://localhost:8000/auth/login \
  -H "Content-Type: application/json" \
  -d '{
    "email": "test@example.com",
    "password": "TestPassword123!"
  }'
```

### 4. Test Protected Endpoint

```bash
# Extract token from login response, then:
curl -X GET http://localhost:8000/auth/me \
  -H "Authorization: Bearer YOUR_ACCESS_TOKEN_HERE"
```

### 5. Test Token Refresh

```bash
curl -X POST http://localhost:8000/auth/refresh \
  -H "Content-Type: application/json" \
  -d '{
    "refresh_token": "YOUR_REFRESH_TOKEN_HERE"
  }'
```

## Verification Checklist

After setup, verify these items work:

- [ ] **Database connection**: App starts without database errors
- [ ] **User registration**: Can create new accounts
- [ ] **User login**: Can authenticate with credentials  
- [ ] **Token validation**: Protected endpoints work with valid tokens
- [ ] **Token refresh**: Can get new access tokens with refresh tokens
- [ ] **Session management**: Can list and revoke sessions
- [ ] **Password reset**: Can request and complete password resets
- [ ] **Rate limiting**: Gets 429 errors after too many requests
- [ ] **Security headers**: Response includes security headers

## Common Issues

### Database Connection Issues
```bash
# Test database connectivity
psql $DATABASE_URL -c "SELECT version();"
```

### JWT Secret Key Issues
```python
# Verify JWT secret is loaded
import os
secret = os.environ.get("JWT_SECRET_KEY")
print(f"Secret loaded: {'✅' if secret else '❌'}")
print(f"Secret length: {len(secret) if secret else 0} characters")
```

### Import Issues
```python
# Test all required imports
try:
    from fraiseql.auth.native import (
        create_native_auth_provider,
        apply_native_auth_schema,
        get_native_auth_router
    )
    print("✅ All imports successful")
except ImportError as e:
    print(f"❌ Import error: {e}")
```

## Next Steps

1. **[API Reference](api-reference.md)** - Learn all available endpoints
2. **[Security Guide](security.md)** - Understand security features
3. **[Migration Guide](migration.md)** - Migrate from other auth providers  
4. **[Deployment Guide](deployment.md)** - Deploy to production
5. **[Multi-Tenant Guide](multi-tenant.md)** - Set up multi-tenancy

## Getting Help

- **Documentation Issues**: [Open an issue](https://github.com/fraiseql/fraiseql/issues)
- **Setup Problems**: Check [Troubleshooting Guide](troubleshooting.md)
- **Community Support**: [GitHub Discussions](https://github.com/fraiseql/fraiseql/discussions)