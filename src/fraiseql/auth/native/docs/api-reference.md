# API Reference - FraiseQL Native Authentication

Complete reference for all authentication endpoints with request/response examples.

## Base URL

All authentication endpoints are available under the `/auth` prefix:

```
https://your-app.com/auth
```

## Authentication

Most endpoints require an `Authorization` header with a Bearer token:

```
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
```

## Endpoints Overview

| Method | Endpoint | Description | Auth Required |
|--------|----------|-------------|---------------|
| `POST` | `/auth/register` | Create new user account | No |
| `POST` | `/auth/login` | Authenticate user | No |
| `POST` | `/auth/refresh` | Refresh access token | No |
| `POST` | `/auth/logout` | End user session | No |
| `GET` | `/auth/me` | Get current user info | Yes |
| `POST` | `/auth/forgot-password` | Request password reset | No |
| `POST` | `/auth/reset-password` | Complete password reset | No |
| `GET` | `/auth/sessions` | List active sessions | Yes |
| `DELETE` | `/auth/sessions/{session_id}` | Revoke session | Yes |

## User Registration

Register a new user account.

### `POST /auth/register`

**Request Body:**
```json
{
  "email": "user@example.com",
  "password": "SecurePassword123!",
  "name": "John Doe"
}
```

**Request Schema:**
- `email` (string, required): Valid email address
- `password` (string, required): Password meeting security requirements
- `name` (string, required): User display name

**Password Requirements:**
- Minimum 8 characters
- At least 1 uppercase letter
- At least 1 lowercase letter  
- At least 1 digit
- At least 1 special character

**Success Response (201):**
```json
{
  "user": {
    "id": "123e4567-e89b-12d3-a456-426614174000",
    "email": "user@example.com",
    "name": "John Doe",
    "roles": [],
    "permissions": [],
    "metadata": {},
    "is_active": true,
    "email_verified": false,
    "created_at": "2025-01-22T10:00:00.000Z",
    "updated_at": "2025-01-22T10:00:00.000Z"
  },
  "access_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "refresh_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "token_type": "bearer",
  "expires_at": "2025-01-22T10:15:00.000Z"
}
```

**Error Responses:**

*400 - Validation Error:*
```json
{
  "detail": {
    "code": "VALIDATION_ERROR",
    "message": "Password does not meet security requirements",
    "field": "password"
  }
}
```

*409 - Email Already Exists:*
```json
{
  "detail": {
    "code": "EMAIL_ALREADY_EXISTS",
    "message": "An account with this email already exists"
  }
}
```

**cURL Example:**
```bash
curl -X POST https://your-app.com/auth/register \
  -H "Content-Type: application/json" \
  -d '{
    "email": "user@example.com",
    "password": "SecurePassword123!",
    "name": "John Doe"
  }'
```

## User Login

Authenticate existing user with email and password.

### `POST /auth/login`

**Request Body:**
```json
{
  "email": "user@example.com",
  "password": "SecurePassword123!"
}
```

**Request Schema:**
- `email` (string, required): User email address
- `password` (string, required): User password

**Success Response (200):**
```json
{
  "user": {
    "id": "123e4567-e89b-12d3-a456-426614174000",
    "email": "user@example.com",
    "name": "John Doe",
    "roles": ["user"],
    "permissions": ["read:profile"],
    "metadata": {},
    "is_active": true,
    "email_verified": true,
    "created_at": "2025-01-22T10:00:00.000Z",
    "updated_at": "2025-01-22T10:00:00.000Z"
  },
  "access_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "refresh_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "token_type": "bearer",
  "expires_at": "2025-01-22T10:15:00.000Z"
}
```

**Error Responses:**

*401 - Invalid Credentials:*
```json
{
  "detail": {
    "code": "INVALID_CREDENTIALS",
    "message": "Invalid email or password"
  }
}
```

*423 - Account Locked:*
```json
{
  "detail": {
    "code": "ACCOUNT_LOCKED",
    "message": "Account is temporarily locked due to too many failed login attempts"
  }
}
```

*429 - Rate Limited:*
```json
{
  "detail": {
    "code": "RATE_LIMITED",
    "message": "Too many login attempts. Please try again later."
  }
}
```

**cURL Example:**
```bash
curl -X POST https://your-app.com/auth/login \
  -H "Content-Type: application/json" \
  -d '{
    "email": "user@example.com",
    "password": "SecurePassword123!"
  }'
```

## Token Refresh

Get a new access token using a valid refresh token.

### `POST /auth/refresh`

**Request Body:**
```json
{
  "refresh_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
}
```

**Request Schema:**
- `refresh_token` (string, required): Valid refresh token

**Success Response (200):**
```json
{
  "access_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "refresh_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "token_type": "bearer",
  "expires_at": "2025-01-22T10:30:00.000Z"
}
```

**Error Responses:**

*401 - Invalid Token:*
```json
{
  "detail": {
    "code": "INVALID_TOKEN",
    "message": "Refresh token is invalid or expired"
  }
}
```

*403 - Token Theft Detected:*
```json
{
  "detail": {
    "code": "TOKEN_THEFT_DETECTED", 
    "message": "Token reuse detected. All sessions have been invalidated for security."
  }
}
```

**cURL Example:**
```bash
curl -X POST https://your-app.com/auth/refresh \
  -H "Content-Type: application/json" \
  -d '{
    "refresh_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
  }'
```

## User Logout

End the current user session and invalidate tokens.

### `POST /auth/logout`

**Request Body:**
```json
{
  "refresh_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
}
```

**Request Schema:**
- `refresh_token` (string, required): Refresh token to invalidate

**Success Response (200):**
```json
{
  "message": "Successfully logged out"
}
```

**Error Response:**

*400 - Invalid Token:*
```json
{
  "detail": {
    "code": "INVALID_TOKEN",
    "message": "Invalid refresh token"
  }
}
```

**cURL Example:**
```bash
curl -X POST https://your-app.com/auth/logout \
  -H "Content-Type: application/json" \
  -d '{
    "refresh_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
  }'
```

## Get Current User

Get information about the currently authenticated user.

### `GET /auth/me`

**Headers:**
```
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
```

**Success Response (200):**
```json
{
  "id": "123e4567-e89b-12d3-a456-426614174000",
  "email": "user@example.com",
  "name": "John Doe",
  "roles": ["user", "admin"],
  "permissions": ["read:profile", "write:profile", "admin:users"],
  "metadata": {
    "department": "Engineering",
    "timezone": "UTC"
  },
  "is_active": true,
  "email_verified": true,
  "created_at": "2025-01-22T10:00:00.000Z",
  "updated_at": "2025-01-22T10:00:00.000Z"
}
```

**Error Responses:**

*401 - Unauthorized:*
```json
{
  "detail": {
    "code": "UNAUTHORIZED",
    "message": "Valid authentication required"
  }
}
```

*403 - Token Expired:*
```json
{
  "detail": {
    "code": "TOKEN_EXPIRED",
    "message": "Access token has expired"
  }
}
```

**cURL Example:**
```bash
curl -X GET https://your-app.com/auth/me \
  -H "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
```

## Password Reset Request

Request a password reset token to be sent to user's email.

### `POST /auth/forgot-password`

**Request Body:**
```json
{
  "email": "user@example.com"
}
```

**Request Schema:**
- `email` (string, required): Email address of the account

**Success Response (200):**
```json
{
  "message": "If an account with this email exists, a password reset link has been sent."
}
```

**Note:** For security, this endpoint always returns success, regardless of whether the email exists.

**cURL Example:**
```bash
curl -X POST https://your-app.com/auth/forgot-password \
  -H "Content-Type: application/json" \
  -d '{
    "email": "user@example.com"
  }'
```

## Complete Password Reset

Complete password reset using a token from email.

### `POST /auth/reset-password`

**Request Body:**
```json
{
  "token": "abc123def456ghi789...",
  "new_password": "NewSecurePassword123!"
}
```

**Request Schema:**
- `token` (string, required): Password reset token from email
- `new_password` (string, required): New password meeting security requirements

**Success Response (200):**
```json
{
  "message": "Password has been reset successfully"
}
```

**Error Responses:**

*400 - Invalid Token:*
```json
{
  "detail": {
    "code": "INVALID_RESET_TOKEN",
    "message": "Password reset token is invalid or expired"
  }
}
```

*400 - Weak Password:*
```json
{
  "detail": {
    "code": "WEAK_PASSWORD",
    "message": "Password does not meet security requirements"
  }
}
```

**cURL Example:**
```bash
curl -X POST https://your-app.com/auth/reset-password \
  -H "Content-Type: application/json" \
  -d '{
    "token": "abc123def456ghi789...",
    "new_password": "NewSecurePassword123!"
  }'
```

## List Active Sessions

Get all active sessions for the current user.

### `GET /auth/sessions`

**Headers:**
```
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
```

**Success Response (200):**
```json
{
  "sessions": [
    {
      "id": "sess_123e4567-e89b-12d3-a456-426614174000",
      "device_info": {
        "user_agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        "platform": "Windows",
        "browser": "Chrome"
      },
      "ip_address": "203.0.113.1",
      "created_at": "2025-01-22T10:00:00.000Z",
      "last_active": "2025-01-22T10:30:00.000Z",
      "is_current": true
    },
    {
      "id": "sess_456e7890-e89b-12d3-a456-426614174111", 
      "device_info": {
        "user_agent": "Mobile App v1.2.0",
        "platform": "iOS",
        "browser": null
      },
      "ip_address": "203.0.113.2",
      "created_at": "2025-01-21T15:00:00.000Z",
      "last_active": "2025-01-21T18:00:00.000Z",
      "is_current": false
    }
  ]
}
```

**Error Response:**

*401 - Unauthorized:*
```json
{
  "detail": {
    "code": "UNAUTHORIZED",
    "message": "Valid authentication required"
  }
}
```

**cURL Example:**
```bash
curl -X GET https://your-app.com/auth/sessions \
  -H "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
```

## Revoke Session

Revoke a specific session, logging out the user from that device.

### `DELETE /auth/sessions/{session_id}`

**Path Parameters:**
- `session_id` (string, required): ID of the session to revoke

**Headers:**
```
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
```

**Success Response (200):**
```json
{
  "message": "Session revoked successfully"
}
```

**Error Responses:**

*401 - Unauthorized:*
```json
{
  "detail": {
    "code": "UNAUTHORIZED", 
    "message": "Valid authentication required"
  }
}
```

*404 - Session Not Found:*
```json
{
  "detail": {
    "code": "SESSION_NOT_FOUND",
    "message": "Session not found or already revoked"
  }
}
```

*403 - Cannot Revoke Current Session:*
```json
{
  "detail": {
    "code": "CANNOT_REVOKE_CURRENT_SESSION",
    "message": "Cannot revoke your current session. Use logout instead."
  }
}
```

**cURL Example:**
```bash
curl -X DELETE https://your-app.com/auth/sessions/sess_123e4567-e89b-12d3-a456-426614174000 \
  -H "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
```

## Error Codes Reference

| Code | HTTP Status | Description |
|------|-------------|-------------|
| `VALIDATION_ERROR` | 400 | Request validation failed |
| `EMAIL_ALREADY_EXISTS` | 409 | Email already registered |
| `INVALID_CREDENTIALS` | 401 | Wrong email or password |
| `ACCOUNT_LOCKED` | 423 | Account temporarily locked |
| `ACCOUNT_DISABLED` | 403 | Account has been disabled |
| `RATE_LIMITED` | 429 | Too many requests |
| `UNAUTHORIZED` | 401 | Authentication required |
| `TOKEN_EXPIRED` | 403 | Access token expired |
| `INVALID_TOKEN` | 401 | Token is invalid |
| `TOKEN_THEFT_DETECTED` | 403 | Token reuse detected |
| `INVALID_RESET_TOKEN` | 400 | Password reset token invalid |
| `WEAK_PASSWORD` | 400 | Password requirements not met |
| `SESSION_NOT_FOUND` | 404 | Session does not exist |
| `CANNOT_REVOKE_CURRENT_SESSION` | 403 | Cannot revoke active session |

## Rate Limiting

Authentication endpoints are rate limited to prevent abuse:

- **General endpoints**: 60 requests per minute per IP
- **Auth endpoints** (login, register): 5 requests per minute per IP

Rate limit headers are included in responses:

```
X-RateLimit-Limit: 60
X-RateLimit-Remaining: 45
X-RateLimit-Reset: 1642857600
```

When rate limited, you'll receive:

```json
{
  "detail": {
    "code": "RATE_LIMITED",
    "message": "Rate limit exceeded. Try again in 60 seconds."
  }
}
```

## Security Headers

All responses include security headers:

```
Strict-Transport-Security: max-age=31536000; includeSubDomains
X-Content-Type-Options: nosniff
X-Frame-Options: DENY
X-XSS-Protection: 1; mode=block
Content-Security-Policy: default-src 'self'
```

## CSRF Protection

When CSRF protection is enabled, you need to include a CSRF token:

1. Get CSRF token from cookie: `csrf_token`
2. Include in request header: `X-CSRF-Token: <token_value>`

## OpenAPI/Swagger Documentation

Interactive API documentation is available when running in development mode:

- **Swagger UI**: `http://localhost:8000/docs`
- **ReDoc**: `http://localhost:8000/redoc`
- **OpenAPI Spec**: `http://localhost:8000/openapi.json`

## SDK Examples

### JavaScript/TypeScript

```javascript
class FraiseQLAuth {
  constructor(baseURL) {
    this.baseURL = baseURL;
    this.accessToken = localStorage.getItem('access_token');
    this.refreshToken = localStorage.getItem('refresh_token');
  }

  async register(email, password, name) {
    const response = await fetch(`${this.baseURL}/auth/register`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email, password, name })
    });
    
    if (response.ok) {
      const data = await response.json();
      this.setTokens(data.access_token, data.refresh_token);
      return data;
    }
    throw new Error('Registration failed');
  }

  async login(email, password) {
    const response = await fetch(`${this.baseURL}/auth/login`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email, password })
    });
    
    if (response.ok) {
      const data = await response.json();
      this.setTokens(data.access_token, data.refresh_token);
      return data;
    }
    throw new Error('Login failed');
  }

  async getCurrentUser() {
    const response = await fetch(`${this.baseURL}/auth/me`, {
      headers: { 'Authorization': `Bearer ${this.accessToken}` }
    });
    
    if (response.status === 401) {
      await this.refreshAccessToken();
      return this.getCurrentUser();
    }
    
    if (response.ok) {
      return response.json();
    }
    throw new Error('Failed to get user');
  }

  async refreshAccessToken() {
    const response = await fetch(`${this.baseURL}/auth/refresh`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ refresh_token: this.refreshToken })
    });
    
    if (response.ok) {
      const data = await response.json();
      this.setTokens(data.access_token, data.refresh_token);
      return data;
    }
    throw new Error('Token refresh failed');
  }

  setTokens(accessToken, refreshToken) {
    this.accessToken = accessToken;
    this.refreshToken = refreshToken;
    localStorage.setItem('access_token', accessToken);
    localStorage.setItem('refresh_token', refreshToken);
  }
}

// Usage
const auth = new FraiseQLAuth('https://your-app.com');
await auth.login('user@example.com', 'password');
const user = await auth.getCurrentUser();
```

### Python

```python
import requests
import json
from datetime import datetime, timedelta

class FraiseQLAuth:
    def __init__(self, base_url):
        self.base_url = base_url
        self.access_token = None
        self.refresh_token = None
        self.expires_at = None
    
    def register(self, email, password, name):
        response = requests.post(f"{self.base_url}/auth/register", 
            json={"email": email, "password": password, "name": name})
        
        if response.status_code == 201:
            data = response.json()
            self._set_tokens(data)
            return data
        raise Exception("Registration failed")
    
    def login(self, email, password):
        response = requests.post(f"{self.base_url}/auth/login",
            json={"email": email, "password": password})
        
        if response.status_code == 200:
            data = response.json()
            self._set_tokens(data)
            return data
        raise Exception("Login failed")
    
    def get_current_user(self):
        headers = {"Authorization": f"Bearer {self.access_token}"}
        response = requests.get(f"{self.base_url}/auth/me", headers=headers)
        
        if response.status_code == 401:
            self.refresh_access_token()
            return self.get_current_user()
        
        if response.status_code == 200:
            return response.json()
        raise Exception("Failed to get user")
    
    def refresh_access_token(self):
        response = requests.post(f"{self.base_url}/auth/refresh",
            json={"refresh_token": self.refresh_token})
        
        if response.status_code == 200:
            data = response.json()
            self._set_tokens(data)
            return data
        raise Exception("Token refresh failed")
    
    def _set_tokens(self, data):
        self.access_token = data['access_token']
        self.refresh_token = data['refresh_token']
        self.expires_at = datetime.fromisoformat(data['expires_at'].replace('Z', '+00:00'))

# Usage
auth = FraiseQLAuth('https://your-app.com')
auth.login('user@example.com', 'password')
user = auth.get_current_user()
```

## Testing

Use the comprehensive test script to validate your API:

```bash
python scripts/test-native-auth.py
```

This script tests all endpoints, security features, and edge cases.

## Next Steps

- **[Security Guide](security.md)** - Understand security features
- **[Migration Guide](migration.md)** - Migrate from other auth providers
- **[Deployment Guide](deployment.md)** - Deploy to production