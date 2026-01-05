# Authentication & Authorization

Complete guide to implementing enterprise-grade authentication and authorization in FraiseQL applications.

## Overview

FraiseQL provides comprehensive authentication and authorization features:

- **JWT Authentication**: Secure token-based authentication
- **Auth0 Integration**: Enterprise SSO support
- **Role-Based Access Control (RBAC)**: Field-level permissions
- **Row-Level Security (RLS)**: Automatic data filtering by user/tenant
- **Multi-Tenant Support**: Built-in tenant isolation

## Quick Start - JWT Authentication

```python
"""
Complete example: JWT authentication with FraiseQL.

Prerequisites:
- pip install fraiseql pyjwt cryptography
- Generate a secret key: python -c "import secrets; print(secrets.token_hex(32))"
"""

import asyncio
from datetime import datetime, timedelta
from uuid import UUID, uuid4
import jwt
import fraiseql
from fraiseql.auth import AuthProvider, UserContext, requires_auth, requires_role

# 1. Create custom JWT auth provider
class SimpleJWTProvider(AuthProvider):
    """Simple JWT authentication provider."""

    def __init__(self, secret_key: str):
        self.secret_key = secret_key
        self.algorithm = "HS256"

    async def validate_token(self, token: str) -> dict:
        """Validate and decode JWT token."""
        try:
            payload = jwt.decode(
                token,
                self.secret_key,
                algorithms=[self.algorithm]
            )
            return payload
        except jwt.ExpiredSignatureError:
            raise ValueError("Token has expired")
        except jwt.InvalidTokenError:
            raise ValueError("Invalid token")

    async def get_user_context(self, token_data: dict) -> UserContext:
        """Extract user context from token."""
        user_id = token_data.get("user_id")
        roles = token_data.get("roles", [])

        return UserContext(
            user_id=user_id,
            roles=roles,
            tenant_id=token_data.get("tenant_id"),
            permissions=token_data.get("permissions", [])
        )

# 2. Configure FraiseQL with authentication
auth_provider = SimpleJWTProvider(secret_key="your-secret-key-here")

app = fraiseql.create_app(
    database_url="postgresql://user:pass@localhost/db",
    auth_provider=auth_provider,
    schema=schema
)

# 3. Protect your GraphQL operations
@fraiseql.query
@requires_auth
async def get_user_profile(info, user_id: UUID) -> UserProfile:
    """Get user profile - requires authentication."""
    return await db.get_user_profile(user_id)

@fraiseql.query
@requires_role("admin")
async def get_all_users(info) -> list[User]:
    """Admin-only query to get all users."""
    return await db.get_all_users()

@fraiseql.mutation
@requires_auth
async def update_profile(info, profile_data: dict) -> UserProfile:
    """Update user profile - requires authentication."""
    user_id = info.context.user.user_id
    return await db.update_user_profile(user_id, profile_data)
```

## Authentication Methods

### JWT Authentication

FraiseQL supports standard JWT tokens with configurable validation:

```python
from fraiseql.auth.jwt import JWTAuthProvider

# Simple HS256 JWT
auth_provider = JWTAuthProvider(
    secret_key="your-secret-key",
    algorithm="HS256",
    token_expiration=timedelta(hours=24)
)

# RS256 with public key
auth_provider = JWTAuthProvider(
    public_key=public_key_pem,
    algorithm="RS256"
)
```

### Auth0 Integration

For enterprise applications, integrate with Auth0:

```python
from fraiseql.auth.auth0 import Auth0Provider

auth_provider = Auth0Provider(
    domain="your-domain.auth0.com",
    audience="your-api-audience",
    client_id="your-client-id"
)
```

## Authorization & Access Control

### Role-Based Access Control (RBAC)

Define roles and permissions at the field level:

```python
from fraiseql.rbac import Role, Permission

# Define roles
ADMIN = Role("admin", permissions=[
    Permission("user.*"),  # All user operations
    Permission("system.*") # All system operations
])

USER = Role("user", permissions=[
    Permission("user.read"),
    Permission("user.update_own")
])

# Apply to GraphQL schema
@fraiseql.type
class User:
    id: UUID
    email: str = Field(permissions=[Permission("user.email")])
    profile: UserProfile = Field(permissions=[Permission("user.profile")])

    @fraiseql.field(permissions=[Permission("admin")])
    async def sensitive_data(self) -> str:
        """Admin-only field."""
        return "sensitive"
```

### Row-Level Security (RLS)

Automatic filtering of data based on user context:

```python
from fraiseql.enterprise.rbac.middleware import create_rbac_middleware
from fraiseql.enterprise.rbac.rust_row_constraints import RustRowConstraintResolver

# Initialize row constraint resolver
row_resolver = RustRowConstraintResolver(database_pool, cache_capacity=10000)

# Add to middleware stack
middleware = create_rbac_middleware(
    auth_provider=auth_provider,
    row_constraint_resolver=row_resolver
)

app = fraiseql.create_app(
    database_url="postgresql://user:pass@localhost/db",
    middleware=[middleware],
    schema=schema
)

# Define row constraints
@fraiseql.type
class Document:
    id: UUID
    title: str
    content: str
    owner_id: UUID = Field(row_constraint="owner_id = $user_id")
    tenant_id: UUID = Field(row_constraint="tenant_id = $tenant_id")

# Users automatically see only their documents
query {
    documents {
        id
        title
    }
}
```

## Multi-Tenant Applications

FraiseQL provides built-in multi-tenant support:

```python
from fraiseql.multitenant import TenantContext

# Configure tenant isolation
app = fraiseql.create_app(
    database_url="postgresql://user:pass@localhost/db",
    tenant_provider=TenantContext(
        tenant_column="tenant_id",
        tenant_from_header="X-Tenant-ID"
    )
)

# All queries automatically filtered by tenant
@fraiseql.type
class Organization:
    id: UUID
    name: str
    tenant_id: UUID = Field(hidden=True)  # Auto-populated
```

## Security Best Practices

### Token Management

```python
# Use secure token generation
import secrets
secret_key = secrets.token_hex(32)  # 256-bit key

# Set reasonable expiration times
token_expiration = timedelta(hours=1)  # Short-lived tokens

# Implement refresh tokens for long sessions
@fraiseql.mutation
async def refresh_token(info, refresh_token: str) -> TokenPair:
    """Refresh access token using refresh token."""
    # Validate refresh token
    # Issue new access token
    pass
```

### Input Validation

```python
from pydantic import BaseModel, validator

class LoginRequest(BaseModel):
    email: str
    password: str

    @validator('email')
    def validate_email(cls, v):
        if '@' not in v:
            raise ValueError('Invalid email')
        return v

@fraiseql.mutation
async def login(info, request: LoginRequest) -> TokenResponse:
    """Secure login with validation."""
    # Authenticate user
    pass
```

### Audit Logging

```python
from fraiseql.audit import AuditLogger

audit_logger = AuditLogger(
    database_url="postgresql://user:pass@localhost/audit_db",
    log_auth_events=True,
    log_permission_checks=True
)

# All authentication and authorization events logged
app = fraiseql.create_app(
    database_url="postgresql://user:pass@localhost/db",
    audit_logger=audit_logger,
    schema=schema
)
```

## Troubleshooting

### Common Issues

**"Authentication required" errors:**
- Check that `@requires_auth` decorator is applied
- Verify token is included in `Authorization: Bearer <token>` header
- Ensure token hasn't expired

**Permission denied:**
- Check user roles against required permissions
- Verify RBAC configuration
- Review row-level constraints

**Token validation fails:**
- Confirm correct algorithm (HS256/RS256)
- Check token expiration
- Validate issuer and audience claims

### Testing Authentication

```python
import pytest
from fraiseql.testing import GraphQLTestClient

@pytest.fixture
async def auth_client():
    """Test client with authentication."""
    client = GraphQLTestClient(app)

    # Login to get token
    response = await client.query("""
        mutation Login($email: String!, $password: String!) {
            login(email: $email, password: $password) {
                token
            }
        }
    """, variables={"email": "test@example.com", "password": "password"})

    token = response.data["login"]["token"]
    client.set_auth_token(token)

    return client

@pytest.mark.asyncio
async def test_protected_query(auth_client):
    """Test authenticated query."""
    response = await auth_client.query("""
        query GetProfile {
            userProfile {
                id
                email
            }
        }
    """)

    assert response.errors is None
    assert "userProfile" in response.data
```

## Integration Examples

### FastAPI Integration

```python
from fastapi import FastAPI, Request, HTTPException
from fastapi.security import HTTPBearer, HTTPAuthorizationCredentials
import fraiseql

app = FastAPI()
security = HTTPBearer()

# FraiseQL app as sub-app
fraiseql_app = fraiseql.create_app(
    database_url="postgresql://user:pass@localhost/db",
    schema=schema
)

@app.middleware("http")
async def auth_middleware(request: Request, call_next):
    """Extract and validate JWT token."""
    try:
        credentials: HTTPAuthorizationCredentials = await security(request)
        token = credentials.credentials

        # Validate token and set user context
        user_context = await validate_jwt_token(token)
        request.state.user = user_context

    except Exception:
        raise HTTPException(status_code=401, detail="Invalid authentication")

    response = await call_next(request)
    return response

# Mount FraiseQL at /graphql
app.mount("/graphql", fraiseql_app)
```

### Django Integration

```python
# settings.py
INSTALLED_APPS = [
    # ... other apps
    'fraiseql.django',
]

# Configure authentication
FRAISEQL = {
    'DATABASE_URL': 'postgresql://user:pass@localhost/db',
    'AUTH_PROVIDER': 'myapp.auth.CustomAuthProvider',
}
```

## Next Steps

- [API Reference](../api/index.md) - Complete API documentation
- [Architecture Overview](../architecture/README.md) - System design
- [Performance Guide](../guides/performance-guide.md) - Performance optimization
- [Troubleshooting](../guides/troubleshooting.md) - Common issues
