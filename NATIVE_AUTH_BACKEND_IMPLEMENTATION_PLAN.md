# FraiseQL Native Authentication - Backend Implementation Plan

## Executive Summary

This document outlines the complete backend implementation plan for FraiseQL's native authentication system with RBAC support. The system will replace Auth0 with a secure, performant, and cost-effective solution.

## Architecture Overview

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   Frontend      │────▶│  FraiseQL API   │────▶│   PostgreSQL    │
│  (Nuxt/Vue)     │◀────│  Auth System    │◀────│   Database      │
└─────────────────┘     └─────────────────┘     └─────────────────┘
         │                       │                         │
         │                       │                         │
    httpOnly cookies        JWT + CSRF              RBAC Tables
                          Session Management         Audit Logs
```

## Phase 1: Database Schema (Week 1, Days 1-2)

### 1.1 Create SQL Migration Files

```
src/fraiseql/auth/sql/
├── 001_schemas.sql           # Create auth, app, core schemas
├── 002_extensions.sql        # Enable pgcrypto, uuid-ossp
├── 003_types.sql            # Custom types (mutation_result, etc.)
├── 004_tables_core.sql      # Core tables (user, role, permission)
├── 005_tables_session.sql   # Session management tables
├── 006_tables_audit.sql     # Audit and security tables
├── 007_views.sql            # Query views (v_user, v_session, etc.)
├── 008_functions_core.sql   # Core functions (password, jwt)
├── 009_functions_auth.sql   # Auth functions (login, register)
├── 010_functions_rbac.sql   # RBAC functions
├── 011_indexes.sql          # Performance indexes
├── 012_initial_data.sql     # Default roles and permissions
└── 999_rollback.sql         # Complete uninstall script
```

### 1.2 Core Tables Structure

```sql
-- Users (following naming convention)
tb_user (pk_user, identifier, email, created_by, created_at, deleted_at)
tb_user_info (pk_user_info, fk_user, password_hash, full_name, email_verified, etc.)
tb_user_security (pk_user_security, fk_user, two_factor_enabled, backup_codes, etc.)

-- RBAC
tb_role (pk_role, identifier, name, description, is_system)
tb_permission (pk_permission, identifier, resource, action, description)
tb_user_role (fk_user, fk_role, granted_by, granted_at, expires_at)
tb_role_permission (fk_role, fk_permission)
tb_role_hierarchy (fk_parent_role, fk_child_role) -- For role inheritance

-- Sessions
tb_session (pk_session, fk_user, refresh_token_hash, family_id, expires_at, etc.)
tb_session_activity (pk_session_activity, fk_session, activity_type, timestamp)

-- Security
tb_auth_event (pk_auth_event, fk_user, event_type, ip_address, success, etc.)
tb_password_history (pk_password_history, fk_user, password_hash, changed_at)
tb_login_attempt (pk_login_attempt, email, ip_address, success, attempted_at)
```

### 1.3 Views for GraphQL Queries

```sql
-- User view with RBAC data
CREATE VIEW v_user AS
SELECT
    u.pk_user as id,
    u.pk_user as tenant_id,  -- For FraiseQL compatibility
    jsonb_build_object(
        'id', u.pk_user,
        'email', u.email,
        'fullName', ui.full_name,
        'emailVerified', ui.email_verified,
        'isActive', ui.is_active,
        'createdAt', u.created_at,
        'lastLoginAt', ui.last_login_at,
        'roles', COALESCE(json_agg(DISTINCT jsonb_build_object(
            'id', r.pk_role,
            'identifier', r.identifier,
            'name', r.name,
            'permissions', (
                SELECT json_agg(jsonb_build_object(
                    'id', p.pk_permission,
                    'identifier', p.identifier,
                    'resource', p.resource,
                    'action', p.action
                ))
                FROM tb_role_permission rp
                JOIN tb_permission p ON rp.fk_permission = p.pk_permission
                WHERE rp.fk_role = r.pk_role
            )
        )) FILTER (WHERE r.pk_role IS NOT NULL), '[]'::json)
    ) as data
FROM tb_user u
LEFT JOIN tb_user_info ui ON u.pk_user = ui.fk_user
LEFT JOIN tb_user_role ur ON u.pk_user = ur.fk_user
    AND (ur.expires_at IS NULL OR ur.expires_at > NOW())
LEFT JOIN tb_role r ON ur.fk_role = r.pk_role
WHERE u.deleted_at IS NULL
GROUP BY u.pk_user, ui.full_name, ui.email_verified, ui.is_active,
         u.created_at, ui.last_login_at;
```

## Phase 2: Core Authentication Functions (Week 1, Days 3-4)

### 2.1 PostgreSQL Functions Structure

```
core schema (internal functions):
├── hash_password(password TEXT) → TEXT
├── verify_password(password TEXT, hash TEXT) → BOOLEAN
├── generate_jwt(claims JSONB, expires_in INTERVAL) → TEXT
├── verify_jwt(token TEXT) → JSONB
├── generate_refresh_token() → TEXT
├── generate_csrf_token() → TEXT
└── clean_expired_sessions() → VOID

app schema (API functions):
├── register_user(input JSONB) → mutation_result
├── login_user(input JSONB) → mutation_result
├── logout_user(input JSONB) → mutation_result
├── refresh_token(input JSONB) → mutation_result
├── verify_email(input JSONB) → mutation_result
├── reset_password(input JSONB) → mutation_result
├── change_password(input JSONB) → mutation_result
└── update_user_profile(input JSONB) → mutation_result
```

### 2.2 Key Function Implementation

```sql
-- Password hashing using pgcrypto
CREATE OR REPLACE FUNCTION core.hash_password(password TEXT)
RETURNS TEXT AS $$
BEGIN
    -- Use bcrypt with cost factor 12
    RETURN crypt(password, gen_salt('bf', 12));
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- User registration with transaction
CREATE OR REPLACE FUNCTION app.register_user(input JSONB)
RETURNS app.mutation_result AS $$
DECLARE
    v_user_id INTEGER;
    v_email TEXT := input->>'email';
    v_password TEXT := input->>'password';
    v_full_name TEXT := input->>'fullName';
    v_result app.mutation_result;
BEGIN
    -- Validate input
    IF v_email IS NULL OR v_password IS NULL THEN
        RETURN ROW(NULL, ARRAY[]::TEXT[], 'error',
                  'Email and password required', NULL,
                  jsonb_build_object('code', 'VALIDATION_ERROR'))::app.mutation_result;
    END IF;

    -- Check if user exists
    IF EXISTS (SELECT 1 FROM tb_user WHERE email = v_email) THEN
        RETURN ROW(NULL, ARRAY[]::TEXT[], 'error',
                  'Email already registered', NULL,
                  jsonb_build_object('code', 'EMAIL_EXISTS'))::app.mutation_result;
    END IF;

    -- Create user
    INSERT INTO tb_user (identifier, email)
    VALUES (v_email, v_email)
    RETURNING pk_user INTO v_user_id;

    -- Create user info
    INSERT INTO tb_user_info (
        fk_user, password_hash, full_name, email_verified
    ) VALUES (
        v_user_id,
        core.hash_password(v_password),
        v_full_name,
        FALSE
    );

    -- Assign default role
    INSERT INTO tb_user_role (fk_user, fk_role)
    SELECT v_user_id, pk_role
    FROM tb_role
    WHERE identifier = 'user';

    -- Create session
    -- ... session creation logic ...

    -- Return success
    SELECT * INTO v_result
    FROM core.build_auth_result(v_user_id, 'REGISTER');

    RETURN v_result;
EXCEPTION
    WHEN OTHERS THEN
        RAISE;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
```

## Phase 3: Python/FraiseQL Integration (Week 1, Days 5-7)

### 3.1 Package Structure

```
src/fraiseql/auth/native/
├── __init__.py
├── provider.py          # NativeAuthProvider implementing AuthProvider
├── config.py           # Configuration classes
├── types.py            # FraiseQL type definitions
├── mutations.py        # Auth mutations
├── queries.py          # Auth queries
├── middleware.py       # Auth middleware
├── decorators.py       # Enhanced decorators with RBAC
├── context.py          # Context builder
├── exceptions.py       # Custom exceptions
├── utils/
│   ├── __init__.py
│   ├── jwt.py         # JWT handling
│   ├── cookies.py     # Cookie utilities
│   ├── csrf.py        # CSRF protection
│   └── validation.py  # Input validation
└── sql/
    └── ... (SQL files from Phase 1)
```

### 3.2 Native Auth Provider

```python
# provider.py
from typing import Any, Optional
from datetime import datetime, timedelta
import jwt
from fraiseql.auth.base import AuthProvider, UserContext, AuthenticationError

class NativeAuthProvider(AuthProvider):
    """Native authentication provider for FraiseQL."""

    def __init__(
        self,
        secret_key: str,
        algorithm: str = "HS256",
        access_token_expire_minutes: int = 15,
        refresh_token_expire_days: int = 7,
        cookie_domain: Optional[str] = None,
        cookie_secure: bool = True,
        cookie_samesite: str = "lax"
    ):
        self.secret_key = secret_key
        self.algorithm = algorithm
        self.access_expire = timedelta(minutes=access_token_expire_minutes)
        self.refresh_expire = timedelta(days=refresh_token_expire_days)
        self.cookie_config = {
            "domain": cookie_domain,
            "secure": cookie_secure,
            "httponly": True,
            "samesite": cookie_samesite
        }

    async def validate_token(self, token: str) -> dict[str, Any]:
        """Validate JWT token from cookie."""
        try:
            payload = jwt.decode(
                token,
                self.secret_key,
                algorithms=[self.algorithm]
            )

            # Verify token type
            if payload.get("type") != "access":
                raise AuthenticationError("Invalid token type")

            return payload

        except jwt.ExpiredSignatureError:
            raise TokenExpiredError("Access token expired")
        except jwt.InvalidTokenError as e:
            raise InvalidTokenError(f"Invalid token: {e}")

    async def get_user_from_token(self, token: str) -> UserContext:
        """Extract user context from token."""
        payload = await self.validate_token(token)

        return UserContext(
            user_id=str(payload["sub"]),
            email=payload.get("email"),
            name=payload.get("name"),
            roles=payload.get("roles", []),
            permissions=payload.get("permissions", []),
            metadata={
                "email_verified": payload.get("email_verified", False),
                "token_iat": payload.get("iat"),
                "token_exp": payload.get("exp")
            }
        )

    async def create_tokens(self, user_data: dict) -> tuple[str, str]:
        """Create access and refresh tokens."""
        now = datetime.utcnow()

        # Access token with user data
        access_payload = {
            "sub": str(user_data["id"]),
            "email": user_data["email"],
            "name": user_data.get("fullName"),
            "roles": [r["identifier"] for r in user_data.get("roles", [])],
            "permissions": [
                f"{p['resource']}:{p['action']}"
                for r in user_data.get("roles", [])
                for p in r.get("permissions", [])
            ],
            "email_verified": user_data.get("emailVerified", False),
            "type": "access",
            "iat": now,
            "exp": now + self.access_expire
        }

        access_token = jwt.encode(
            access_payload,
            self.secret_key,
            algorithm=self.algorithm
        )

        # Refresh token (minimal data)
        refresh_payload = {
            "sub": str(user_data["id"]),
            "type": "refresh",
            "family": user_data.get("session_family_id"),
            "iat": now,
            "exp": now + self.refresh_expire
        }

        refresh_token = jwt.encode(
            refresh_payload,
            self.secret_key,
            algorithm=self.algorithm
        )

        return access_token, refresh_token
```

### 3.3 FraiseQL Types

```python
# types.py
from dataclasses import dataclass
from datetime import datetime
from typing import Optional, List
from uuid import UUID
import fraiseql

@fraiseql.fraise_type
@dataclass
class Role:
    id: int
    identifier: str
    name: str
    description: Optional[str]
    permissions: List["Permission"]

@fraiseql.fraise_type
@dataclass
class Permission:
    id: int
    identifier: str
    resource: str
    action: str
    description: Optional[str]

@fraiseql.fraise_type
@dataclass
class User:
    id: int
    email: str
    full_name: Optional[str]
    email_verified: bool
    is_active: bool
    created_at: datetime
    last_login_at: Optional[datetime]
    roles: List[Role]

@fraiseql.fraise_type
@dataclass
class AuthPayload:
    user: User
    message: str

@fraiseql.fraise_type
@dataclass
class LoginInput:
    email: str
    password: str
    remember_me: bool = False

@fraiseql.fraise_type
@dataclass
class RegisterInput:
    email: str
    password: str
    full_name: Optional[str]
```

### 3.4 Authentication Mutations

```python
# mutations.py
import fraiseql
from fraiseql import GraphQLResolveInfo
from starlette.responses import Response
from .types import LoginInput, RegisterInput, AuthPayload
from .utils.cookies import set_auth_cookies, clear_auth_cookies
from .utils.csrf import generate_csrf_token, verify_csrf_token

@fraiseql.mutation
async def login(
    info: GraphQLResolveInfo,
    input: LoginInput
) -> AuthPayload:
    """Authenticate user and set cookies."""
    db = info.context["db"]
    response: Response = info.context["response"]

    # Verify CSRF token
    csrf_token = info.context["request"].headers.get("X-CSRF-Token")
    if not verify_csrf_token(csrf_token):
        raise GraphQLError("Invalid CSRF token")

    # Execute login function
    result = await db.execute_function(
        "app.login_user",
        {
            "email": input.email,
            "password": input.password
        }
    )

    if result["change_status"] != "success":
        raise GraphQLError(
            result["message"],
            extensions={"code": result["extra_metadata"]["code"]}
        )

    # Get user data
    user_data = result["row_data"]["user"]

    # Create tokens
    provider = info.context["auth_provider"]
    access_token, refresh_token = await provider.create_tokens(user_data)

    # Set cookies
    set_auth_cookies(
        response,
        access_token,
        refresh_token,
        result["row_data"]["csrf_token"],
        provider.cookie_config
    )

    # Return user
    return AuthPayload(
        user=user_data,
        message="Login successful"
    )

@fraiseql.mutation
async def logout(info: GraphQLResolveInfo) -> bool:
    """Logout user and clear cookies."""
    db = info.context["db"]
    response: Response = info.context["response"]
    user = info.context.get("user")

    if user:
        # Revoke refresh token in database
        refresh_token = info.context["request"].cookies.get("__Host-refresh-token")
        if refresh_token:
            await db.execute_function(
                "app.logout_user",
                {
                    "user_id": user.user_id,
                    "refresh_token": refresh_token
                }
            )

    # Clear cookies
    clear_auth_cookies(response)

    return True

@fraiseql.mutation
async def register(
    info: GraphQLResolveInfo,
    input: RegisterInput
) -> AuthPayload:
    """Register new user."""
    db = info.context["db"]
    response: Response = info.context["response"]

    # Verify CSRF token
    csrf_token = info.context["request"].headers.get("X-CSRF-Token")
    if not verify_csrf_token(csrf_token):
        raise GraphQLError("Invalid CSRF token")

    # Execute registration
    result = await db.execute_function(
        "app.register_user",
        {
            "email": input.email,
            "password": input.password,
            "fullName": input.full_name
        }
    )

    if result["change_status"] != "success":
        raise GraphQLError(
            result["message"],
            extensions={"code": result["extra_metadata"]["code"]}
        )

    # Auto-login after registration
    user_data = result["row_data"]["user"]
    provider = info.context["auth_provider"]
    access_token, refresh_token = await provider.create_tokens(user_data)

    set_auth_cookies(
        response,
        access_token,
        refresh_token,
        result["row_data"]["csrf_token"],
        provider.cookie_config
    )

    return AuthPayload(
        user=user_data,
        message="Registration successful"
    )
```

### 3.5 Protected Queries

```python
# queries.py
import fraiseql
from fraiseql import GraphQLResolveInfo
from .decorators import require_auth, require_role
from .types import User

@fraiseql.query
@require_auth
async def me(info: GraphQLResolveInfo) -> User:
    """Get current authenticated user."""
    user_context = info.context["user"]
    db = info.context["db"]

    user = await db.find_one("v_user", id=int(user_context.user_id))
    if not user:
        raise GraphQLError("User not found")

    return user

@fraiseql.query
@require_role("admin")
async def users(
    info: GraphQLResolveInfo,
    limit: int = 100,
    offset: int = 0
) -> List[User]:
    """List all users (admin only)."""
    db = info.context["db"]
    return await db.find("v_user", limit=limit, offset=offset)
```

## Phase 4: Middleware & Context (Week 2, Days 1-2)

### 4.1 Authentication Middleware

```python
# middleware.py
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.requests import Request
from starlette.responses import Response

class AuthenticationMiddleware(BaseHTTPMiddleware):
    """Extract and verify auth tokens from cookies."""

    def __init__(self, app, auth_provider):
        super().__init__(app)
        self.auth_provider = auth_provider

    async def dispatch(self, request: Request, call_next):
        # Extract access token from cookie
        access_token = request.cookies.get("__Host-access-token")

        if access_token:
            try:
                # Validate token
                user_context = await self.auth_provider.get_user_from_token(
                    access_token
                )
                request.state.user = user_context
            except TokenExpiredError:
                # Try to refresh
                refresh_token = request.cookies.get("__Host-refresh-token")
                if refresh_token:
                    # Attempt refresh (implementation depends on your needs)
                    pass
            except Exception:
                # Invalid token, continue without user
                pass

        response = await call_next(request)
        return response
```

### 4.2 Context Builder

```python
# context.py
from typing import Dict, Any
from starlette.requests import Request
from starlette.responses import Response

async def build_auth_context(
    request: Request,
    response: Response,
    db: Any
) -> Dict[str, Any]:
    """Build GraphQL context with auth information."""
    context = {
        "request": request,
        "response": response,
        "db": db,
        "user": getattr(request.state, "user", None),
        "is_authenticated": hasattr(request.state, "user")
    }

    # Add auth provider
    from .provider import NativeAuthProvider
    context["auth_provider"] = request.app.state.auth_provider

    return context
```

## Phase 5: API Endpoints (Week 2, Days 3-4)

### 5.1 REST Endpoints for Auth

```python
# endpoints.py
from fastapi import APIRouter, Depends, HTTPException, Response, Request
from .types import LoginInput, RegisterInput

router = APIRouter(prefix="/auth", tags=["Authentication"])

@router.post("/login")
async def login_endpoint(
    input: LoginInput,
    request: Request,
    response: Response,
    db = Depends(get_db)
):
    """REST endpoint for login."""
    # Similar to GraphQL mutation but returns JSON
    pass

@router.post("/logout")
async def logout_endpoint(
    request: Request,
    response: Response,
    db = Depends(get_db)
):
    """REST endpoint for logout."""
    pass

@router.get("/me")
async def me_endpoint(
    request: Request,
    user = Depends(get_current_user)
):
    """Get current user."""
    return {"user": user}

@router.get("/csrf-token")
async def csrf_token_endpoint(response: Response):
    """Get CSRF token."""
    token = generate_csrf_token()
    response.set_cookie(
        "csrf-token",
        token,
        httponly=False,  # Must be readable by JS
        secure=True,
        samesite="strict"
    )
    return {"csrf_token": token}
```

## Phase 6: Security Hardening (Week 2, Days 5-7)

### 6.1 Rate Limiting

```python
# utils/rate_limit.py
from datetime import datetime, timedelta
from collections import defaultdict
import asyncio

class RateLimiter:
    def __init__(self, max_attempts: int, window_minutes: int):
        self.max_attempts = max_attempts
        self.window = timedelta(minutes=window_minutes)
        self.attempts = defaultdict(list)
        self._cleanup_task = None

    async def check_rate_limit(self, key: str) -> bool:
        """Check if rate limit exceeded."""
        now = datetime.utcnow()
        cutoff = now - self.window

        # Clean old attempts
        self.attempts[key] = [
            attempt for attempt in self.attempts[key]
            if attempt > cutoff
        ]

        # Check limit
        if len(self.attempts[key]) >= self.max_attempts:
            return False

        # Record attempt
        self.attempts[key].append(now)
        return True

    async def start_cleanup(self):
        """Periodically clean old entries."""
        while True:
            await asyncio.sleep(300)  # Every 5 minutes
            cutoff = datetime.utcnow() - self.window
            for key in list(self.attempts.keys()):
                self.attempts[key] = [
                    attempt for attempt in self.attempts[key]
                    if attempt > cutoff
                ]
                if not self.attempts[key]:
                    del self.attempts[key]

# Global rate limiters
login_limiter = RateLimiter(max_attempts=5, window_minutes=1)
api_limiter = RateLimiter(max_attempts=100, window_minutes=1)
```

### 6.2 Security Headers

```python
# middleware/security_headers.py
from starlette.middleware.base import BaseHTTPMiddleware

class SecurityHeadersMiddleware(BaseHTTPMiddleware):
    """Add security headers to all responses."""

    async def dispatch(self, request, call_next):
        response = await call_next(request)

        # Security headers
        response.headers["X-Content-Type-Options"] = "nosniff"
        response.headers["X-Frame-Options"] = "DENY"
        response.headers["X-XSS-Protection"] = "1; mode=block"
        response.headers["Strict-Transport-Security"] = "max-age=31536000; includeSubDomains"
        response.headers["Referrer-Policy"] = "strict-origin-when-cross-origin"

        # Remove server header
        response.headers.pop("Server", None)

        return response
```

## Phase 7: Testing Suite (Week 3)

### 7.1 Test Structure

```
tests/auth/
├── test_registration.py
├── test_login.py
├── test_logout.py
├── test_token_refresh.py
├── test_rbac.py
├── test_rate_limiting.py
├── test_security.py
├── test_middleware.py
└── fixtures/
    ├── users.py
    ├── roles.py
    └── database.py
```

### 7.2 Example Tests

```python
# test_registration.py
import pytest
from httpx import AsyncClient

@pytest.mark.asyncio
async def test_user_registration(client: AsyncClient, db):
    """Test user registration flow."""
    # Get CSRF token
    csrf_response = await client.get("/auth/csrf-token")
    csrf_token = csrf_response.json()["csrf_token"]

    # Register user
    response = await client.post(
        "/graphql",
        json={
            "query": """
                mutation Register($input: RegisterInput!) {
                    register(input: $input) {
                        user {
                            id
                            email
                            fullName
                            roles {
                                identifier
                            }
                        }
                        message
                    }
                }
            """,
            "variables": {
                "input": {
                    "email": "test@example.com",
                    "password": "SecurePassword123!",
                    "fullName": "Test User"
                }
            }
        },
        headers={"X-CSRF-Token": csrf_token}
    )

    assert response.status_code == 200
    data = response.json()["data"]["register"]
    assert data["user"]["email"] == "test@example.com"
    assert data["user"]["roles"][0]["identifier"] == "user"

    # Check cookies were set
    assert "__Host-access-token" in response.cookies
    assert "__Host-refresh-token" in response.cookies

@pytest.mark.asyncio
async def test_duplicate_registration(client: AsyncClient, existing_user):
    """Test registration with existing email."""
    csrf_response = await client.get("/auth/csrf-token")
    csrf_token = csrf_response.json()["csrf_token"]

    response = await client.post(
        "/graphql",
        json={
            "query": """
                mutation Register($input: RegisterInput!) {
                    register(input: $input) {
                        user { id }
                        message
                    }
                }
            """,
            "variables": {
                "input": {
                    "email": existing_user.email,
                    "password": "AnyPassword123!"
                }
            }
        },
        headers={"X-CSRF-Token": csrf_token}
    )

    assert response.status_code == 200
    errors = response.json().get("errors", [])
    assert len(errors) == 1
    assert errors[0]["extensions"]["code"] == "EMAIL_EXISTS"
```

## Phase 8: Documentation (Week 3-4)

### 8.1 API Documentation

Create comprehensive API docs:
- OpenAPI/Swagger spec for REST endpoints
- GraphQL schema documentation
- Authentication flow diagrams
- RBAC permission matrix
- Security best practices

### 8.2 Developer Guide

- Quick start guide
- Migration from Auth0
- Custom provider implementation
- Extending RBAC
- Troubleshooting guide

## Phase 9: Production Deployment (Week 4)

### 9.1 Deployment Checklist

- [ ] Environment variables configured
- [ ] SSL certificates in place
- [ ] Database migrations run
- [ ] Initial roles/permissions created
- [ ] Admin user created
- [ ] Rate limiting configured
- [ ] Monitoring/alerting setup
- [ ] Backup strategy in place
- [ ] Security audit completed
- [ ] Load testing performed

### 9.2 Monitoring & Metrics

```python
# monitoring/auth_metrics.py
from prometheus_client import Counter, Histogram, Gauge

# Metrics
login_attempts = Counter('auth_login_attempts_total', 'Total login attempts', ['status'])
active_sessions = Gauge('auth_active_sessions', 'Number of active sessions')
token_refresh = Counter('auth_token_refresh_total', 'Token refresh attempts', ['status'])
auth_latency = Histogram('auth_operation_duration_seconds', 'Auth operation latency', ['operation'])
```

## Migration Strategy

### Phase 1: Parallel Operation (Week 1)
- Deploy native auth alongside Auth0
- Enable feature flag for testing
- Monitor both systems

### Phase 2: Gradual Migration (Week 2)
- Migrate internal users first
- Enable for new registrations
- Provide migration tool for existing users

### Phase 3: Full Cutover (Week 3)
- Migrate remaining users
- Disable Auth0 integration
- Monitor for issues

### Phase 4: Cleanup (Week 4)
- Remove Auth0 code
- Archive Auth0 data
- Update documentation

## Success Metrics

1. **Performance**
   - Login latency < 200ms
   - Token refresh < 100ms
   - Session lookup < 50ms

2. **Security**
   - Zero auth bypasses
   - Successful rate limiting
   - Proper token expiration

3. **Reliability**
   - 99.9% uptime
   - Graceful failure handling
   - Automatic recovery

4. **User Experience**
   - Seamless migration
   - No breaking changes
   - Improved performance

## Risk Mitigation

1. **Data Loss**
   - Full backup before migration
   - Incremental backups during
   - Rollback procedures ready

2. **Security Breach**
   - Security audit before launch
   - Penetration testing
   - Bug bounty program

3. **Performance Issues**
   - Load testing at 10x capacity
   - Database optimization
   - Caching strategy

4. **User Disruption**
   - Gradual rollout
   - Feature flags
   - Quick rollback capability

This comprehensive plan provides a production-ready authentication system that integrates seamlessly with FraiseQL while providing enterprise-grade security and performance.
