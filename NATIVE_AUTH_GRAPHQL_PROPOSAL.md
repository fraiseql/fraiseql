# FraiseQL Native Authentication - Pure GraphQL Implementation

## Overview

This proposal presents a pure GraphQL approach to native authentication in FraiseQL, eliminating REST endpoints in favor of a consistent, GraphQL-first design that leverages the framework's strengths while maintaining security best practices.

## Core Principles

1. **GraphQL-Only**: All auth operations through GraphQL mutations/queries
2. **Cookie-Based**: Secure httpOnly cookies for token storage
3. **Stateless JWT**: Self-contained tokens with user context
4. **RBAC Built-in**: Role-based access control as first-class feature
5. **Framework Integration**: Seamless with FraiseQL's patterns

## GraphQL Schema Design

### Authentication Types

```graphql
"""Authentication payload returned after successful auth operations"""
type AuthPayload {
  """Authenticated user details"""
  user: User!
  
  """Success message for the operation"""
  message: String!
  
  """Token expiration time (for client awareness)"""
  expiresAt: DateTime!
  
  """Whether this is a new user (useful for onboarding)"""
  isNewUser: Boolean!
}

"""User type with RBAC information"""
type User {
  id: ID!
  email: String!
  fullName: String
  avatarUrl: String
  emailVerified: Boolean!
  isActive: Boolean!
  createdAt: DateTime!
  lastLoginAt: DateTime
  
  """User's assigned roles"""
  roles: [Role!]!
  
  """Flattened permissions from all roles"""
  permissions: [Permission!]!
  
  """Check if user has specific permission"""
  hasPermission(resource: String!, action: String!): Boolean!
  
  """Check if user has specific role"""
  hasRole(identifier: String!): Boolean!
}

"""Role in the RBAC system"""
type Role {
  id: ID!
  identifier: String!
  name: String!
  description: String
  isSystem: Boolean!
  permissions: [Permission!]!
}

"""Permission in the RBAC system"""  
type Permission {
  id: ID!
  identifier: String!
  resource: String!
  action: String!
  description: String
}

"""Login input"""
input LoginInput {
  email: String!
  password: String!
  """Keep user logged in for extended period"""
  rememberMe: Boolean = false
}

"""Registration input"""
input RegisterInput {
  email: String!
  password: String!
  fullName: String
  """Optional invite code for closed registrations"""
  inviteCode: String
}

"""Password reset request input"""
input PasswordResetRequestInput {
  email: String!
}

"""Password reset completion input"""
input PasswordResetInput {
  token: String!
  newPassword: String!
}

"""Password change input for authenticated users"""
input ChangePasswordInput {
  currentPassword: String!
  newPassword: String!
}

"""Profile update input"""
input UpdateProfileInput {
  fullName: String
  avatarUrl: String
}

"""Two-factor authentication setup"""
input TwoFactorSetupInput {
  password: String!
}

"""Two-factor authentication verification"""
input TwoFactorVerifyInput {
  code: String!
}

"""Session information"""
type Session {
  id: ID!
  userAgent: String
  ipAddress: String
  location: String
  lastActivityAt: DateTime!
  current: Boolean!
}

"""Two-factor setup response"""
type TwoFactorSetupPayload {
  qrCode: String!
  secret: String!
  backupCodes: [String!]!
}
```

### Mutations

```graphql
type Mutation {
  """Register a new user account"""
  register(input: RegisterInput!): AuthPayload!
  
  """Login with email and password"""
  login(input: LoginInput!): AuthPayload!
  
  """Logout current session"""
  logout: Boolean!
  
  """Logout from all devices"""
  logoutAll: Boolean!
  
  """Refresh authentication token"""
  refreshAuth: AuthPayload!
  
  """Request password reset email"""
  requestPasswordReset(input: PasswordResetRequestInput!): Boolean!
  
  """Complete password reset with token"""
  resetPassword(input: PasswordResetInput!): AuthPayload!
  
  """Change password for authenticated user"""
  changePassword(input: ChangePasswordInput!): Boolean!
  
  """Update user profile"""
  updateProfile(input: UpdateProfileInput!): User!
  
  """Setup two-factor authentication"""
  setupTwoFactor(input: TwoFactorSetupInput!): TwoFactorSetupPayload!
  
  """Verify two-factor authentication code"""
  verifyTwoFactor(input: TwoFactorVerifyInput!): Boolean!
  
  """Disable two-factor authentication"""
  disableTwoFactor(password: String!): Boolean!
  
  """Resend email verification"""
  resendVerificationEmail: Boolean!
  
  """Verify email with token"""
  verifyEmail(token: String!): Boolean!
  
  """Revoke a specific session"""
  revokeSession(sessionId: ID!): Boolean!
}
```

### Queries

```graphql
type Query {
  """Get current authenticated user"""
  me: User
  
  """List all active sessions for current user"""
  mySessions: [Session!]!
  
  """Check if email is available for registration"""
  emailAvailable(email: String!): Boolean!
  
  """Get authentication configuration"""
  authConfig: AuthConfig!
}

"""Public authentication configuration"""
type AuthConfig {
  """Whether registration is open"""
  registrationEnabled: Boolean!
  
  """Whether invite codes are required"""
  inviteCodeRequired: Boolean!
  
  """Supported OAuth providers"""
  oauthProviders: [String!]!
  
  """Password requirements"""
  passwordRequirements: PasswordRequirements!
  
  """Whether two-factor is available"""
  twoFactorEnabled: Boolean!
}

"""Password requirement rules"""
type PasswordRequirements {
  minLength: Int!
  requireUppercase: Boolean!
  requireLowercase: Boolean!
  requireNumbers: Boolean!
  requireSpecialChars: Boolean!
}
```

## Implementation Architecture

### 1. Cookie Strategy

```python
# src/fraiseql/auth/native/cookies.py
from typing import Optional
from datetime import datetime, timedelta

class CookieConfig:
    """Secure cookie configuration for auth tokens."""
    
    # Use __Host- prefix for maximum security
    ACCESS_TOKEN_NAME = "__Host-access-token"
    REFRESH_TOKEN_NAME = "__Host-refresh-token"
    
    # Cookie settings
    SECURE = True  # HTTPS only
    HTTPONLY = True  # No JS access
    SAMESITE = "lax"  # CSRF protection
    PATH = "/"
    
    # Expiration
    ACCESS_TOKEN_MINUTES = 15
    REFRESH_TOKEN_DAYS = 30
    REMEMBER_ME_DAYS = 90

def set_auth_cookies(
    response: Response,
    access_token: str,
    refresh_token: str,
    remember_me: bool = False
) -> None:
    """Set authentication cookies on response."""
    
    # Access token cookie (short-lived)
    response.set_cookie(
        key=CookieConfig.ACCESS_TOKEN_NAME,
        value=access_token,
        max_age=CookieConfig.ACCESS_TOKEN_MINUTES * 60,
        secure=CookieConfig.SECURE,
        httponly=CookieConfig.HTTPONLY,
        samesite=CookieConfig.SAMESITE,
        path=CookieConfig.PATH
    )
    
    # Refresh token cookie (long-lived)
    refresh_max_age = (
        CookieConfig.REMEMBER_ME_DAYS if remember_me 
        else CookieConfig.REFRESH_TOKEN_DAYS
    ) * 86400
    
    response.set_cookie(
        key=CookieConfig.REFRESH_TOKEN_NAME,
        value=refresh_token,
        max_age=refresh_max_age,
        secure=CookieConfig.SECURE,
        httponly=CookieConfig.HTTPONLY,
        samesite=CookieConfig.SAMESITE,
        path=CookieConfig.PATH
    )

def clear_auth_cookies(response: Response) -> None:
    """Clear all authentication cookies."""
    for cookie_name in [
        CookieConfig.ACCESS_TOKEN_NAME,
        CookieConfig.REFRESH_TOKEN_NAME
    ]:
        response.delete_cookie(
            key=cookie_name,
            path=CookieConfig.PATH
        )
```

### 2. GraphQL Mutations Implementation

```python
# src/fraiseql/auth/native/mutations.py
import fraiseql
from fraiseql import GraphQLResolveInfo
from typing import Optional
from .types import AuthPayload, LoginInput, RegisterInput
from .cookies import set_auth_cookies, clear_auth_cookies
from .security import verify_rate_limit, log_auth_event

@fraiseql.mutation
async def login(
    info: GraphQLResolveInfo,
    input: LoginInput
) -> AuthPayload:
    """Authenticate user and set secure cookies."""
    db = info.context["db"]
    request = info.context["request"]
    response = info.context["response"]
    
    # Rate limiting
    client_ip = request.client.host
    if not await verify_rate_limit(f"login:{client_ip}", max_attempts=5):
        raise GraphQLError(
            "Too many login attempts. Please try again later.",
            extensions={"code": "RATE_LIMIT_EXCEEDED"}
        )
    
    # Execute login database function
    result = await db.execute_function(
        "app.login_user",
        {
            "email": input.email,
            "password": input.password,
            "ip_address": client_ip,
            "user_agent": request.headers.get("user-agent", "")
        }
    )
    
    if result["change_status"] != "success":
        # Log failed attempt
        await log_auth_event(
            event_type="login_failed",
            email=input.email,
            ip_address=client_ip,
            details=result.get("extra_metadata")
        )
        
        raise GraphQLError(
            result["message"],
            extensions={
                "code": result["extra_metadata"].get("code", "LOGIN_FAILED")
            }
        )
    
    # Extract user and tokens from result
    user_data = result["row_data"]["user"]
    access_token = result["row_data"]["access_token"]
    refresh_token = result["row_data"]["refresh_token"]
    expires_at = result["row_data"]["expires_at"]
    
    # Set secure cookies
    set_auth_cookies(
        response=response,
        access_token=access_token,
        refresh_token=refresh_token,
        remember_me=input.remember_me
    )
    
    # Log successful login
    await log_auth_event(
        event_type="login_success",
        user_id=user_data["id"],
        ip_address=client_ip
    )
    
    return AuthPayload(
        user=user_data,
        message="Successfully logged in",
        expires_at=expires_at,
        is_new_user=False
    )

@fraiseql.mutation
async def logout(info: GraphQLResolveInfo) -> bool:
    """Logout current session and clear cookies."""
    db = info.context["db"]
    request = info.context["request"]
    response = info.context["response"]
    user = info.context.get("user")
    
    if user:
        # Get refresh token from cookie to identify session
        refresh_token = request.cookies.get(CookieConfig.REFRESH_TOKEN_NAME)
        
        if refresh_token:
            # Revoke session in database
            await db.execute_function(
                "app.revoke_session",
                {
                    "user_id": user.user_id,
                    "refresh_token": refresh_token
                }
            )
        
        # Log logout event
        await log_auth_event(
            event_type="logout",
            user_id=user.user_id,
            ip_address=request.client.host
        )
    
    # Clear cookies regardless of auth status
    clear_auth_cookies(response)
    
    return True

@fraiseql.mutation
async def refresh_auth(info: GraphQLResolveInfo) -> AuthPayload:
    """Refresh authentication using refresh token from cookie."""
    db = info.context["db"]
    request = info.context["request"]
    response = info.context["response"]
    
    # Get refresh token from cookie
    refresh_token = request.cookies.get(CookieConfig.REFRESH_TOKEN_NAME)
    
    if not refresh_token:
        raise GraphQLError(
            "No refresh token provided",
            extensions={"code": "REFRESH_TOKEN_MISSING"}
        )
    
    # Execute token refresh
    result = await db.execute_function(
        "app.refresh_token",
        {
            "refresh_token": refresh_token,
            "ip_address": request.client.host,
            "user_agent": request.headers.get("user-agent", "")
        }
    )
    
    if result["change_status"] != "success":
        # Clear invalid cookies
        clear_auth_cookies(response)
        
        raise GraphQLError(
            result["message"],
            extensions={
                "code": result["extra_metadata"].get("code", "REFRESH_FAILED")
            }
        )
    
    # Set new cookies
    user_data = result["row_data"]["user"]
    new_access_token = result["row_data"]["access_token"]
    new_refresh_token = result["row_data"]["refresh_token"]
    expires_at = result["row_data"]["expires_at"]
    
    set_auth_cookies(
        response=response,
        access_token=new_access_token,
        refresh_token=new_refresh_token,
        remember_me=result["row_data"].get("remember_me", False)
    )
    
    return AuthPayload(
        user=user_data,
        message="Authentication refreshed",
        expires_at=expires_at,
        is_new_user=False
    )

@fraiseql.mutation
async def register(
    info: GraphQLResolveInfo,
    input: RegisterInput
) -> AuthPayload:
    """Register new user and auto-login."""
    db = info.context["db"]
    request = info.context["request"]
    response = info.context["response"]
    
    # Check registration settings
    auth_config = await db.find_one("v_auth_config")
    if not auth_config["registration_enabled"]:
        raise GraphQLError(
            "Registration is currently disabled",
            extensions={"code": "REGISTRATION_DISABLED"}
        )
    
    # Validate invite code if required
    if auth_config["invite_code_required"] and not input.invite_code:
        raise GraphQLError(
            "Invite code is required for registration",
            extensions={"code": "INVITE_CODE_REQUIRED"}
        )
    
    # Rate limiting
    client_ip = request.client.host
    if not await verify_rate_limit(f"register:{client_ip}", max_attempts=3):
        raise GraphQLError(
            "Too many registration attempts. Please try again later.",
            extensions={"code": "RATE_LIMIT_EXCEEDED"}
        )
    
    # Execute registration
    result = await db.execute_function(
        "app.register_user",
        {
            "email": input.email,
            "password": input.password,
            "full_name": input.full_name,
            "invite_code": input.invite_code,
            "ip_address": client_ip
        }
    )
    
    if result["change_status"] != "success":
        raise GraphQLError(
            result["message"],
            extensions={
                "code": result["extra_metadata"].get("code", "REGISTRATION_FAILED")
            }
        )
    
    # Auto-login after registration
    user_data = result["row_data"]["user"]
    access_token = result["row_data"]["access_token"]
    refresh_token = result["row_data"]["refresh_token"]
    expires_at = result["row_data"]["expires_at"]
    
    set_auth_cookies(
        response=response,
        access_token=access_token,
        refresh_token=refresh_token,
        remember_me=False
    )
    
    # Log registration event
    await log_auth_event(
        event_type="registration",
        user_id=user_data["id"],
        ip_address=client_ip
    )
    
    return AuthPayload(
        user=user_data,
        message="Registration successful",
        expires_at=expires_at,
        is_new_user=True
    )
```

### 3. Security Middleware

```python
# src/fraiseql/auth/native/middleware.py
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.requests import Request
import jwt
from typing import Optional

class GraphQLAuthMiddleware(BaseHTTPMiddleware):
    """Extract and validate auth from cookies for GraphQL requests."""
    
    def __init__(self, app, auth_provider):
        super().__init__(app)
        self.auth_provider = auth_provider
    
    async def dispatch(self, request: Request, call_next):
        # Only process GraphQL requests
        if request.url.path == "/graphql":
            # Extract access token from cookie
            access_token = request.cookies.get(CookieConfig.ACCESS_TOKEN_NAME)
            
            if access_token:
                try:
                    # Validate and decode token
                    user_context = await self.auth_provider.get_user_from_token(
                        access_token
                    )
                    request.state.user = user_context
                    request.state.auth_token = access_token
                except TokenExpiredError:
                    # Token expired, will be handled by refresh logic
                    request.state.token_expired = True
                except Exception:
                    # Invalid token, continue without auth
                    pass
        
        response = await call_next(request)
        return response
```

### 4. Automatic Token Refresh

```python
# src/fraiseql/auth/native/refresh.py
from functools import wraps
from graphql import GraphQLError

def auto_refresh(f):
    """Decorator to automatically refresh expired tokens."""
    @wraps(f)
    async def wrapper(info, *args, **kwargs):
        request = info.context["request"]
        
        # Check if token is expired
        if getattr(request.state, "token_expired", False):
            # Get refresh token
            refresh_token = request.cookies.get(CookieConfig.REFRESH_TOKEN_NAME)
            
            if refresh_token:
                try:
                    # Attempt refresh
                    refresh_result = await info.context["db"].execute_function(
                        "app.refresh_token",
                        {"refresh_token": refresh_token}
                    )
                    
                    if refresh_result["change_status"] == "success":
                        # Update context with new user data
                        user_data = refresh_result["row_data"]["user"]
                        info.context["user"] = UserContext.from_dict(user_data)
                        
                        # Set new cookies on response
                        response = info.context["response"]
                        set_auth_cookies(
                            response=response,
                            access_token=refresh_result["row_data"]["access_token"],
                            refresh_token=refresh_result["row_data"]["refresh_token"]
                        )
                        
                        # Clear expired flag
                        request.state.token_expired = False
                        request.state.user = info.context["user"]
                except:
                    # Refresh failed, continue without auth
                    pass
        
        # Execute original function
        return await f(info, *args, **kwargs)
    
    return wrapper
```

### 5. CSRF Protection

```python
# src/fraiseql/auth/native/csrf.py
from typing import Optional
import hmac
import secrets
from datetime import datetime, timedelta

class CSRFProtection:
    """CSRF protection for GraphQL mutations."""
    
    @staticmethod
    def should_check_csrf(info: GraphQLResolveInfo) -> bool:
        """Determine if CSRF check is needed."""
        # Skip for queries
        if info.operation.operation == "query":
            return False
        
        # Skip for introspection
        if info.field_name == "__schema":
            return False
        
        # Skip for public mutations
        public_mutations = {
            "login", "register", "requestPasswordReset",
            "resetPassword", "verifyEmail"
        }
        if info.field_name in public_mutations:
            return False
        
        return True
    
    @staticmethod
    def verify_csrf(request: Request) -> bool:
        """Verify CSRF protection via custom header."""
        # Check for custom header (most reliable for GraphQL)
        custom_header = request.headers.get("X-GraphQL-Request")
        if custom_header == "true":
            return True
        
        # Check Origin/Referer for same-origin
        origin = request.headers.get("Origin")
        if origin:
            # Verify origin matches expected domain
            # This should be configured based on your setup
            expected_origin = request.url.scheme + "://" + request.url.netloc
            return origin == expected_origin
        
        return False

def csrf_protection(f):
    """Decorator to enforce CSRF protection on mutations."""
    @wraps(f)
    async def wrapper(info, *args, **kwargs):
        if CSRFProtection.should_check_csrf(info):
            request = info.context["request"]
            if not CSRFProtection.verify_csrf(request):
                raise GraphQLError(
                    "CSRF verification failed",
                    extensions={"code": "CSRF_FAILED"}
                )
        
        return await f(info, *args, **kwargs)
    
    return wrapper
```

## Frontend Integration

### 1. Apollo Client Setup

```typescript
// apollo-client.ts
import { ApolloClient, InMemoryCache, createHttpLink } from '@apollo/client'
import { setContext } from '@apollo/client/link/context'

// HTTP link with credentials
const httpLink = createHttpLink({
  uri: '/graphql',
  credentials: 'include' // Essential for cookies
})

// Add custom header for CSRF protection
const authLink = setContext((_, { headers }) => {
  return {
    headers: {
      ...headers,
      'X-GraphQL-Request': 'true' // CSRF protection
    }
  }
})

// Create Apollo Client
export const apolloClient = new ApolloClient({
  link: authLink.concat(httpLink),
  cache: new InMemoryCache(),
  defaultOptions: {
    watchQuery: {
      fetchPolicy: 'cache-and-network'
    }
  }
})
```

### 2. Auth Composable (Vue 3/Nuxt 3)

```typescript
// composables/useAuth.ts
import { ref, computed } from 'vue'
import { useMutation, useQuery } from '@vue/apollo-composable'
import { 
  LOGIN_MUTATION, 
  LOGOUT_MUTATION, 
  REGISTER_MUTATION,
  REFRESH_AUTH_MUTATION,
  ME_QUERY 
} from '~/graphql/auth'

export const useAuth = () => {
  const { result: meResult, loading, refetch: refetchMe } = useQuery(ME_QUERY)
  
  const user = computed(() => meResult.value?.me || null)
  const isAuthenticated = computed(() => !!user.value)
  
  // Login mutation
  const { mutate: loginMutation, loading: loginLoading } = useMutation(LOGIN_MUTATION)
  
  const login = async (email: string, password: string, rememberMe = false) => {
    try {
      const { data } = await loginMutation({
        input: { email, password, rememberMe }
      })
      
      // Refetch current user to update cache
      await refetchMe()
      
      return { success: true, user: data.login.user }
    } catch (error) {
      return { 
        success: false, 
        error: error.graphQLErrors?.[0]?.extensions?.code || 'LOGIN_FAILED' 
      }
    }
  }
  
  // Logout mutation
  const { mutate: logoutMutation } = useMutation(LOGOUT_MUTATION)
  
  const logout = async () => {
    await logoutMutation()
    // Clear Apollo cache
    await apolloClient.clearStore()
    // Navigate to home
    await navigateTo('/')
  }
  
  // Register mutation
  const { mutate: registerMutation, loading: registerLoading } = useMutation(REGISTER_MUTATION)
  
  const register = async (input: RegisterInput) => {
    try {
      const { data } = await registerMutation({ input })
      
      // Refetch current user
      await refetchMe()
      
      return { success: true, user: data.register.user }
    } catch (error) {
      return {
        success: false,
        error: error.graphQLErrors?.[0]?.extensions?.code || 'REGISTRATION_FAILED'
      }
    }
  }
  
  // Auto-refresh setup
  const { mutate: refreshMutation } = useMutation(REFRESH_AUTH_MUTATION)
  
  const setupAutoRefresh = () => {
    // Refresh 1 minute before expiry
    const refreshInterval = setInterval(async () => {
      if (user.value) {
        try {
          await refreshMutation()
          await refetchMe()
        } catch (error) {
          // Refresh failed, user needs to login again
          clearInterval(refreshInterval)
        }
      }
    }, 14 * 60 * 1000) // 14 minutes
    
    // Cleanup on unmount
    onUnmounted(() => clearInterval(refreshInterval))
  }
  
  // Initialize auto-refresh if authenticated
  if (isAuthenticated.value) {
    setupAutoRefresh()
  }
  
  // RBAC helpers
  const hasRole = (role: string) => 
    user.value?.roles?.some(r => r.identifier === role) ?? false
  
  const hasPermission = (resource: string, action: string) =>
    user.value?.permissions?.some(p => 
      p.resource === resource && p.action === action
    ) ?? false
  
  return {
    // State
    user: readonly(user),
    isAuthenticated: readonly(isAuthenticated),
    loading: readonly(loading),
    loginLoading: readonly(loginLoading),
    registerLoading: readonly(registerLoading),
    
    // Actions
    login,
    logout,
    register,
    refetchMe,
    
    // RBAC
    hasRole,
    hasPermission
  }
}
```

### 3. Protected Route Middleware

```typescript
// middleware/auth.ts
export default defineNuxtRouteMiddleware((to) => {
  const { isAuthenticated } = useAuth()
  
  if (!isAuthenticated.value) {
    return navigateTo(`/login?redirect=${encodeURIComponent(to.fullPath)}`)
  }
})

// middleware/role.ts
export default defineNuxtRouteMiddleware((to) => {
  const { hasRole } = useAuth()
  
  // Extract required role from route meta
  const requiredRole = to.meta.role as string
  
  if (requiredRole && !hasRole(requiredRole)) {
    throw createError({
      statusCode: 403,
      statusMessage: 'Insufficient permissions'
    })
  }
})
```

### 4. GraphQL Queries/Mutations

```typescript
// graphql/auth.ts
import { gql } from '@apollo/client/core'

export const USER_FRAGMENT = gql`
  fragment UserDetails on User {
    id
    email
    fullName
    avatarUrl
    emailVerified
    isActive
    roles {
      id
      identifier
      name
    }
    permissions {
      id
      resource
      action
    }
  }
`

export const LOGIN_MUTATION = gql`
  mutation Login($input: LoginInput!) {
    login(input: $input) {
      user {
        ...UserDetails
      }
      message
      expiresAt
    }
  }
  ${USER_FRAGMENT}
`

export const LOGOUT_MUTATION = gql`
  mutation Logout {
    logout
  }
`

export const ME_QUERY = gql`
  query Me {
    me {
      ...UserDetails
    }
  }
  ${USER_FRAGMENT}
`

export const REFRESH_AUTH_MUTATION = gql`
  mutation RefreshAuth {
    refreshAuth {
      user {
        ...UserDetails
      }
      expiresAt
    }
  }
  ${USER_FRAGMENT}
`
```

## Security Best Practices

### 1. Cookie Security
- Use `__Host-` prefix for maximum security
- Set `Secure`, `HttpOnly`, and `SameSite` flags
- Short-lived access tokens (15 minutes)
- Longer-lived refresh tokens (30 days)

### 2. Token Rotation
- New refresh token issued on each refresh
- Token families to detect token theft
- Immediate revocation of compromised families

### 3. Rate Limiting
- 5 login attempts per minute per IP
- 3 registration attempts per hour per IP
- Exponential backoff for repeated failures

### 4. CSRF Protection
- Custom header `X-GraphQL-Request: true`
- Origin/Referer validation as fallback
- SameSite cookies as additional protection

### 5. Security Headers
- Strict CSP for GraphQL endpoint
- HSTS enforcement
- X-Frame-Options: DENY

## Database Schema Highlights

### Session Management
```sql
-- Token families for refresh token rotation
CREATE TABLE tb_session (
    pk_session SERIAL PRIMARY KEY,
    fk_user INTEGER REFERENCES tb_user(pk_user),
    family_id UUID NOT NULL,
    refresh_token_hash TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    ip_address INET,
    user_agent TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Detect token theft via family tracking
CREATE OR REPLACE FUNCTION app.refresh_token(input JSONB)
RETURNS app.mutation_result AS $$
DECLARE
    v_session RECORD;
    v_family_id UUID;
BEGIN
    -- Verify refresh token
    SELECT * INTO v_session
    FROM tb_session
    WHERE refresh_token_hash = crypt(input->>'refresh_token', refresh_token_hash)
      AND expires_at > NOW();
    
    IF NOT FOUND THEN
        -- Check if token was already used (potential theft)
        SELECT family_id INTO v_family_id
        FROM tb_session_history
        WHERE refresh_token_hash = crypt(input->>'refresh_token', refresh_token_hash);
        
        IF FOUND THEN
            -- Token reuse detected! Revoke entire family
            DELETE FROM tb_session WHERE family_id = v_family_id;
            RETURN app.build_error_result('TOKEN_THEFT_DETECTED', 
                'Security alert: Token reuse detected. All sessions revoked.');
        END IF;
        
        RETURN app.build_error_result('INVALID_REFRESH_TOKEN', 
            'Invalid or expired refresh token');
    END IF;
    
    -- Archive old session
    INSERT INTO tb_session_history 
    SELECT * FROM tb_session WHERE pk_session = v_session.pk_session;
    
    -- Create new session with same family
    INSERT INTO tb_session (
        fk_user, family_id, refresh_token_hash, expires_at, ip_address, user_agent
    ) VALUES (
        v_session.fk_user,
        v_session.family_id,  -- Same family
        crypt(gen_random_uuid()::text, gen_salt('bf')),
        NOW() + INTERVAL '30 days',
        (input->>'ip_address')::INET,
        input->>'user_agent'
    );
    
    -- Delete old session
    DELETE FROM tb_session WHERE pk_session = v_session.pk_session;
    
    -- Return new tokens
    RETURN app.build_auth_result(v_session.fk_user, 'TOKEN_REFRESHED');
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
```

## Migration Path

### From REST-based Auth
1. Keep REST endpoints temporarily for backward compatibility
2. Add GraphQL mutations alongside
3. Update frontend to use GraphQL
4. Deprecate and remove REST endpoints

### From Auth0
1. Implement user migration endpoint
2. Lazy migration on first login
3. Bulk migration for active users
4. Maintain Auth0 as fallback initially

## Testing Strategy

### Unit Tests
```python
@pytest.mark.asyncio
async def test_login_mutation():
    """Test login via GraphQL mutation."""
    query = """
        mutation Login($input: LoginInput!) {
            login(input: $input) {
                user { id email }
                message
            }
        }
    """
    
    result = await execute_graphql(
        query,
        variables={
            "input": {
                "email": "test@example.com",
                "password": "correct_password"
            }
        }
    )
    
    assert result.data["login"]["user"]["email"] == "test@example.com"
    assert "__Host-access-token" in result.cookies
```

### Integration Tests
- Full auth flow testing
- Token refresh scenarios
- Rate limiting verification
- CSRF protection validation

## Performance Considerations

### Caching Strategy
```python
# Cache user permissions for fast access
@cached(ttl=300)  # 5 minutes
async def get_user_permissions(user_id: int) -> list[str]:
    """Get flattened user permissions from cache or database."""
    # This is called frequently for authorization checks
    pass
```

### Database Optimization
- Indexes on email, refresh_token_hash
- Partial indexes for active sessions
- Materialized view for permission flattening

## Conclusion

This pure GraphQL approach to authentication in FraiseQL provides:

1. **Consistency**: All operations through GraphQL
2. **Security**: Modern cookie-based approach with CSRF protection  
3. **Simplicity**: No REST/GraphQL split
4. **Performance**: Efficient token refresh and caching
5. **Developer Experience**: Clean, predictable API

The implementation leverages FraiseQL's strengths while maintaining security best practices, resulting in a robust authentication system that feels native to GraphQL.