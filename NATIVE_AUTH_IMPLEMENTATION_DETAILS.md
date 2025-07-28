# 🔐 FraiseQL Native Authentication - Implementation Details

## 📋 Overview

This document provides comprehensive implementation details for FraiseQL's native authentication system, based on the REST approach recommended in the unified proposal. It incorporates insights from PrintOptim's production authentication patterns and addresses the specific requirements of a modern SaaS application.

## 🛠️ REST API Endpoints Specification

### Authentication Endpoints

#### POST /auth/login
**Purpose**: Authenticate user and establish session

```typescript
// Request
{
  "email": "user@example.com",
  "password": "secure_password",
  "remember_me": false  // Optional: Use longer-lived refresh token
}

// Response (200 OK)
{
  "user": {
    "id": "uuid",
    "email": "user@example.com",
    "name": "John Doe",
    "roles": ["user", "admin"],
    "permissions": ["read:users", "write:projects"],
    "organization": {
      "id": "org_uuid",
      "name": "Acme Corp",
      "plan": "pro"
    }
  },
  "expires_at": "2024-01-01T12:00:00Z"
}

// Sets cookies:
// - access_token (httpOnly, secure, sameSite=lax, 15 min)
// - refresh_token (httpOnly, secure, sameSite=lax, 30 days)
// - csrf_token (readable, secure, sameSite=lax)
```

#### POST /auth/logout
**Purpose**: Terminate session and clear cookies

```typescript
// Request
{} // Empty body, authentication via cookies

// Response (200 OK)
{
  "message": "Successfully logged out"
}

// Clears all auth cookies
```

#### POST /auth/register
**Purpose**: Create new user account

```typescript
// Request
{
  "email": "user@example.com",
  "password": "secure_password",
  "name": "John Doe",
  "organization_name": "Acme Corp"  // Optional: Create new org
}

// Response (201 Created)
{
  "user": { /* Same as login response */ },
  "message": "Account created successfully"
}
```

#### POST /auth/refresh
**Purpose**: Refresh access token using refresh token

```typescript
// Request
{} // Empty body, uses refresh_token cookie

// Response (200 OK)
{
  "expires_at": "2024-01-01T12:15:00Z"
}

// Updates access_token cookie
// Rotates refresh_token for security
```

#### GET /auth/me
**Purpose**: Get current authenticated user

```typescript
// Response (200 OK)
{
  "user": { /* Full user object */ },
  "expires_at": "2024-01-01T12:00:00Z"
}
```

#### GET /auth/csrf-token
**Purpose**: Get CSRF token for forms

```typescript
// Response (200 OK)
{
  "csrf_token": "random_token_value"
}

// Also sets csrf_token cookie
```

#### POST /auth/forgot-password
**Purpose**: Initiate password reset flow

```typescript
// Request
{
  "email": "user@example.com"
}

// Response (200 OK)
{
  "message": "If the email exists, a reset link has been sent"
}
```

#### POST /auth/reset-password
**Purpose**: Complete password reset

```typescript
// Request
{
  "token": "reset_token_from_email",
  "password": "new_secure_password"
}

// Response (200 OK)
{
  "message": "Password reset successfully"
}
```

### Session Management Endpoints

#### GET /auth/sessions
**Purpose**: List all active sessions for current user

```typescript
// Response (200 OK)
{
  "sessions": [
    {
      "id": "session_uuid",
      "device": "Chrome on macOS",
      "ip_address": "192.168.1.1",
      "location": "San Francisco, CA",
      "created_at": "2024-01-01T10:00:00Z",
      "last_active": "2024-01-01T11:30:00Z",
      "is_current": true
    }
  ]
}
```

#### DELETE /auth/sessions/:id
**Purpose**: Revoke specific session

```typescript
// Response (200 OK)
{
  "message": "Session revoked successfully"
}
```

## 🗄️ Database Migration Strategy

### Phase 1: Preparation (Week 1)
1. **Export Auth0 Users**
   ```python
   # Script to export Auth0 users
   import auth0

   def export_auth0_users():
       # Connect to Auth0 Management API
       # Export user profiles, roles, permissions
       # Store in temporary migration table
   ```

2. **Create Migration Tables**
   ```sql
   -- Temporary migration tracking
   CREATE TABLE auth_migration (
       auth0_user_id TEXT PRIMARY KEY,
       fraiseql_user_id UUID,
       migrated_at TIMESTAMPTZ,
       status TEXT CHECK (status IN ('pending', 'completed', 'failed')),
       error_details JSONB
   );
   ```

### Phase 2: Parallel Authentication (Week 2-3)
1. **Dual Authentication Support**
   ```python
   class HybridAuthProvider(AuthProvider):
       def __init__(self, auth0_provider, native_provider):
           self.auth0 = auth0_provider
           self.native = native_provider

       async def verify_token(self, token: str) -> AuthInfo:
           # Try native auth first
           try:
               return await self.native.verify_token(token)
           except InvalidTokenError:
               # Fall back to Auth0
               return await self.auth0.verify_token(token)
   ```

2. **Progressive User Migration**
   ```python
   @router.post("/auth/migrate-on-login")
   async def migrate_on_login(auth0_token: str, password: str):
       # Verify Auth0 token
       # Create native account with provided password
       # Mark as migrated
       # Return native auth tokens
   ```

### Phase 3: Cutover (Week 4)
1. **Final Batch Migration**
   ```sql
   -- Migrate remaining users with password reset required
   INSERT INTO tb_user (email, name, requires_password_reset)
   SELECT
       auth0_email,
       auth0_name,
       TRUE
   FROM auth_migration
   WHERE status = 'pending';
   ```

2. **Decommission Auth0**
   - Remove Auth0 SDK from frontend
   - Remove Auth0 middleware from backend
   - Archive Auth0 configuration

## 🎨 Frontend Auth Composable Implementation

### Core Auth Composable
```typescript
// composables/useAuth.ts
import type { User, LoginCredentials, RegisterData } from '~/types/auth'

export const useAuth = () => {
  const user = useState<User | null>('auth.user', () => null)
  const isAuthenticated = computed(() => !!user.value)
  const isLoading = useState('auth.loading', () => false)

  // CSRF token management
  const csrfToken = useCookie('csrf_token', {
    httpOnly: false,
    secure: true,
    sameSite: 'lax'
  })

  const fetchCsrfToken = async () => {
    if (!csrfToken.value) {
      const { csrf_token } = await $fetch('/auth/csrf-token')
      csrfToken.value = csrf_token
    }
    return csrfToken.value
  }

  const login = async (credentials: LoginCredentials) => {
    isLoading.value = true
    try {
      const csrf = await fetchCsrfToken()
      const response = await $fetch('/auth/login', {
        method: 'POST',
        headers: { 'X-CSRF-Token': csrf },
        body: credentials,
        credentials: 'include'
      })

      user.value = response.user

      // Navigate to intended route or dashboard
      const redirect = useRoute().query.redirect as string
      await navigateTo(redirect || '/dashboard')

      return response
    } catch (error) {
      throw error
    } finally {
      isLoading.value = false
    }
  }

  const logout = async () => {
    isLoading.value = true
    try {
      await $fetch('/auth/logout', {
        method: 'POST',
        credentials: 'include'
      })

      user.value = null
      await navigateTo('/login')
    } finally {
      isLoading.value = false
    }
  }

  const register = async (data: RegisterData) => {
    // Similar to login
  }

  const checkAuth = async () => {
    try {
      const response = await $fetch('/auth/me', {
        credentials: 'include'
      })
      user.value = response.user
    } catch {
      user.value = null
    }
  }

  // RBAC helpers
  const hasRole = (role: string) => {
    return user.value?.roles?.includes(role) ?? false
  }

  const hasPermission = (permission: string) => {
    return user.value?.permissions?.includes(permission) ?? false
  }

  const can = (action: string, resource: string) => {
    return hasPermission(`${action}:${resource}`)
  }

  return {
    user: readonly(user),
    isAuthenticated: readonly(isAuthenticated),
    isLoading: readonly(isLoading),
    login,
    logout,
    register,
    checkAuth,
    hasRole,
    hasPermission,
    can
  }
}
```

### Auth Middleware
```typescript
// middleware/auth.ts
export default defineNuxtRouteMiddleware((to) => {
  const { isAuthenticated } = useAuth()

  // Skip for OAuth callback
  if (to.query.code) return

  if (!isAuthenticated.value) {
    return navigateTo('/login?redirect=' + encodeURIComponent(to.fullPath))
  }
})
```

### Protected Route Usage
```vue
<!-- pages/dashboard.vue -->
<script setup lang="ts">
definePageMeta({
  middleware: 'auth'
})

const { user, can } = useAuth()
</script>

<template>
  <div>
    <h1>Welcome {{ user?.name }}</h1>

    <AdminPanel v-if="can('read', 'admin')" />
  </div>
</template>
```

## 🔒 Security Headers and CSRF Implementation

### Security Headers Middleware
```python
# src/fraiseql/middleware/security.py
from fastapi import Request, Response
from fastapi.middleware.base import BaseHTTPMiddleware

class SecurityHeadersMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request: Request, call_next):
        response = await call_next(request)

        # Security headers
        response.headers["X-Content-Type-Options"] = "nosniff"
        response.headers["X-Frame-Options"] = "DENY"
        response.headers["X-XSS-Protection"] = "1; mode=block"
        response.headers["Referrer-Policy"] = "strict-origin-when-cross-origin"

        # CSP for XSS protection
        response.headers["Content-Security-Policy"] = (
            "default-src 'self'; "
            "script-src 'self' 'unsafe-inline' 'unsafe-eval'; "
            "style-src 'self' 'unsafe-inline'; "
            "img-src 'self' data: https:; "
            "connect-src 'self' https://api.fraiseql.com"
        )

        return response
```

### CSRF Protection
```python
# src/fraiseql/auth/csrf.py
import secrets
from fastapi import HTTPException, Request, Response

class CSRFProtection:
    def __init__(self, secret_key: str):
        self.secret_key = secret_key

    def generate_token(self) -> str:
        return secrets.token_urlsafe(32)

    def set_csrf_cookie(self, response: Response, token: str):
        response.set_cookie(
            key="csrf_token",
            value=token,
            secure=True,
            httponly=False,  # Must be readable by JS
            samesite="lax",
            max_age=3600  # 1 hour
        )

    def verify_csrf_token(self, request: Request):
        # Get token from header
        header_token = request.headers.get("X-CSRF-Token")
        if not header_token:
            raise HTTPException(status_code=403, detail="CSRF token missing")

        # Get token from cookie
        cookie_token = request.cookies.get("csrf_token")
        if not cookie_token:
            raise HTTPException(status_code=403, detail="CSRF cookie missing")

        # Compare tokens
        if not secrets.compare_digest(header_token, cookie_token):
            raise HTTPException(status_code=403, detail="CSRF token mismatch")
```

## 🔄 Session Management and Token Rotation

### Token Rotation Strategy
```python
# src/fraiseql/auth/tokens.py
from datetime import datetime, timedelta
import jwt
import uuid

class TokenManager:
    def __init__(self, secret_key: str):
        self.secret_key = secret_key
        self.access_token_ttl = timedelta(minutes=15)
        self.refresh_token_ttl = timedelta(days=30)

    def create_token_family(self, user_id: str) -> str:
        """Create a new token family for tracking token lineage"""
        return str(uuid.uuid4())

    def generate_tokens(self, user_id: str, family_id: str = None):
        if not family_id:
            family_id = self.create_token_family(user_id)

        # Access token with user data
        access_payload = {
            "sub": user_id,
            "type": "access",
            "exp": datetime.utcnow() + self.access_token_ttl,
            "iat": datetime.utcnow(),
            "jti": str(uuid.uuid4())
        }

        # Refresh token with family tracking
        refresh_payload = {
            "sub": user_id,
            "type": "refresh",
            "family": family_id,
            "exp": datetime.utcnow() + self.refresh_token_ttl,
            "iat": datetime.utcnow(),
            "jti": str(uuid.uuid4())
        }

        access_token = jwt.encode(access_payload, self.secret_key, algorithm="HS256")
        refresh_token = jwt.encode(refresh_payload, self.secret_key, algorithm="HS256")

        return {
            "access_token": access_token,
            "refresh_token": refresh_token,
            "family_id": family_id,
            "expires_at": access_payload["exp"]
        }

    async def rotate_refresh_token(self, old_token: str, db):
        """Rotate refresh token and detect token theft"""
        try:
            payload = jwt.decode(old_token, self.secret_key, algorithms=["HS256"])

            # Check if token was already used
            used_token = await db.find_one("used_refresh_tokens", {
                "token_jti": payload["jti"]
            })

            if used_token:
                # Token theft detected! Invalidate entire family
                await self.invalidate_token_family(payload["family"], db)
                raise SecurityError("Token reuse detected - possible theft")

            # Mark token as used
            await db.insert("used_refresh_tokens", {
                "token_jti": payload["jti"],
                "family_id": payload["family"],
                "used_at": datetime.utcnow()
            })

            # Generate new tokens with same family
            return self.generate_tokens(payload["sub"], payload["family"])

        except jwt.ExpiredSignatureError:
            raise AuthError("Refresh token expired")
```

### Session Storage
```sql
-- Session tracking for user activity
CREATE TABLE tb_session (
    pk_session UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    fk_user UUID NOT NULL REFERENCES tb_user(pk_user),
    token_family UUID NOT NULL,
    device_info JSONB,
    ip_address INET,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    last_active TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    revoked_at TIMESTAMPTZ
);

-- Index for fast family lookups
CREATE INDEX idx_session_family ON tb_session(token_family) WHERE revoked_at IS NULL;

-- Used tokens to prevent replay attacks
CREATE TABLE tb_used_refresh_token (
    token_jti TEXT PRIMARY KEY,
    family_id UUID NOT NULL,
    used_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

-- Auto-cleanup old used tokens
CREATE INDEX idx_used_token_cleanup ON tb_used_refresh_token(used_at);
```

## 🧪 Testing Strategy

### Unit Tests
```python
# tests/auth/test_token_manager.py
import pytest
from datetime import datetime, timedelta
from fraiseql.auth.tokens import TokenManager

class TestTokenManager:
    def test_generate_tokens(self):
        manager = TokenManager("secret")
        tokens = manager.generate_tokens("user123")

        assert "access_token" in tokens
        assert "refresh_token" in tokens
        assert "family_id" in tokens

    def test_token_rotation_detects_theft(self):
        # Use token twice to simulate theft
        # Verify family invalidation
        pass
```

### Integration Tests
```python
# tests/auth/test_auth_endpoints.py
import pytest
from httpx import AsyncClient

@pytest.mark.asyncio
async def test_login_flow(client: AsyncClient):
    # Get CSRF token
    csrf_response = await client.get("/auth/csrf-token")
    csrf_token = csrf_response.json()["csrf_token"]

    # Login
    login_response = await client.post(
        "/auth/login",
        json={"email": "test@example.com", "password": "password"},
        headers={"X-CSRF-Token": csrf_token}
    )

    assert login_response.status_code == 200
    assert "user" in login_response.json()

    # Verify cookies are set
    assert "access_token" in login_response.cookies
    assert "refresh_token" in login_response.cookies
```

### E2E Tests
```typescript
// tests/e2e/auth.spec.ts
import { test, expect } from '@playwright/test'

test('complete auth flow', async ({ page }) => {
  // Navigate to login
  await page.goto('/login')

  // Fill form
  await page.fill('[name="email"]', 'test@example.com')
  await page.fill('[name="password"]', 'password')

  // Submit
  await page.click('[type="submit"]')

  // Verify redirect to dashboard
  await expect(page).toHaveURL('/dashboard')

  // Verify user info displayed
  await expect(page.locator('text=Welcome')).toBeVisible()
})
```

### Security Tests
```python
# tests/auth/test_security.py
import pytest
from fraiseql.auth.csrf import CSRFProtection

class TestSecurity:
    def test_csrf_token_validation(self):
        csrf = CSRFProtection("secret")
        token = csrf.generate_token()

        # Valid token passes
        # Mismatched token fails
        # Missing token fails

    def test_rate_limiting(self):
        # Verify 5 attempts/minute limit
        # Test lockout behavior
        pass
```

## 🚀 Implementation Timeline

### Week 1: Backend Foundation
- [ ] Database schema creation
- [ ] Auth endpoint implementation
- [ ] Token management system
- [ ] CSRF protection
- [ ] Security headers

### Week 2: Frontend Integration
- [ ] Auth composable
- [ ] Login/register pages
- [ ] Protected route middleware
- [ ] Session management UI
- [ ] Error handling

### Week 3: Migration & Testing
- [ ] Auth0 data export
- [ ] Parallel auth support
- [ ] Migration scripts
- [ ] Comprehensive testing
- [ ] Performance optimization

### Week 4: Production Rollout
- [ ] Staged deployment
- [ ] Monitoring setup
- [ ] User communication
- [ ] Auth0 decommission
- [ ] Documentation

## 📊 Monitoring and Observability

### Key Metrics
```python
# Track authentication metrics
auth_metrics = {
    "login_attempts": Counter("auth_login_attempts_total"),
    "login_success": Counter("auth_login_success_total"),
    "login_failures": Counter("auth_login_failures_total"),
    "token_refreshes": Counter("auth_token_refreshes_total"),
    "token_theft_detected": Counter("auth_token_theft_total"),
    "session_duration": Histogram("auth_session_duration_seconds")
}
```

### Audit Logging
```sql
-- Comprehensive audit trail
CREATE TABLE tb_auth_audit (
    pk_audit UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type TEXT NOT NULL,
    user_id UUID,
    ip_address INET,
    user_agent TEXT,
    event_data JSONB,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

-- Index for user activity queries
CREATE INDEX idx_audit_user ON tb_auth_audit(user_id, created_at DESC);
```

## 🎯 Success Criteria

1. **Security**
   - Zero auth bypasses in penetration testing
   - All OWASP Top 10 vulnerabilities addressed
   - Token theft detection working correctly

2. **Performance**
   - Login < 200ms (p95)
   - Token refresh < 100ms (p95)
   - Zero downtime migration

3. **User Experience**
   - Seamless migration from Auth0
   - No forced password resets for active users
   - Improved login speed

4. **Developer Experience**
   - Simple, intuitive auth composable
   - Clear documentation
   - Comprehensive test coverage

---

*This implementation guide provides the detailed roadmap for FraiseQL's native authentication system. Follow the weekly milestones and leverage the provided code examples to ensure a smooth transition from Auth0.*
