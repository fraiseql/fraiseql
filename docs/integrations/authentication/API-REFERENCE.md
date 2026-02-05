# FraiseQL Authentication API Reference

Complete API documentation for FraiseQL's OAuth 2.0 / OIDC authentication system.

## Base URL

```text
Development: http://localhost:8000
Production: https://api.yourdomain.com
```text

## Endpoints

### 1. POST /auth/start

Initiate the OAuth authentication flow.

**Request**

```bash
curl -X POST http://localhost:8000/auth/start \
  -H "Content-Type: application/json" \
  -d '{
    "provider": "google"
  }'
```text

**Request Body**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `provider` | string | No | OAuth provider name (default: "default") |

**Response (200 OK)**

```json
{
  "authorization_url": "https://accounts.google.com/o/oauth2/v2/auth?client_id=...&state=..."
}
```text

**Response Fields**

| Field | Type | Description |
|-------|------|-------------|
| `authorization_url` | string | URL to redirect user to for authentication |

**Error Responses**

| Status | Error | Description |
|--------|-------|-------------|
| 500 | `auth_error` | OAuth provider configuration error |

**Usage**

1. Redirect user to the returned `authorization_url`
2. User authenticates with the provider
3. Provider redirects to `/auth/callback` with code and state

---

### 2. GET /auth/callback

OAuth provider redirects here after user authentication.

**Query Parameters**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `code` | string | Yes | Authorization code from provider |
| `state` | string | Yes | CSRF protection state parameter |
| `error` | string | No | Error code if authentication failed |
| `error_description` | string | No | Detailed error message |

**Example Request**

```text
GET http://localhost:8000/auth/callback?code=4/0A...&state=xyz123
```text

**Response (200 OK)**

```json
{
  "access_token": "access_token_user123_3600_uuid",
  "refresh_token": "base64_encoded_refresh_token",
  "token_type": "Bearer",
  "expires_in": 3600
}
```text

**Response Fields**

| Field | Type | Description |
|-------|------|-------------|
| `access_token` | string | JWT token for API requests (1 hour expiry) |
| `refresh_token` | string | Token to refresh access token (7 days expiry) |
| `token_type` | string | Always "Bearer" |
| `expires_in` | number | Seconds until access token expires |

**Error Responses**

| Status | Error | Description |
|--------|-------|-------------|
| 400 | `invalid_state` | State parameter invalid or expired |
| 500 | `oauth_error` | Provider error during token exchange |
| 502 | `oauth_error` | Provider unreachable |

**Security Notes**

- State parameter is validated (10 minute expiry)
- Code can only be used once
- Code expires in 10 minutes

---

### 3. POST /auth/refresh

Refresh an expired access token using a refresh token.

**Request**

```bash
curl -X POST http://localhost:8000/auth/refresh \
  -H "Content-Type: application/json" \
  -d '{
    "refresh_token": "base64_encoded_refresh_token"
  }'
```text

**Request Body**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `refresh_token` | string | Yes | Refresh token from previous login |

**Response (200 OK)**

```json
{
  "access_token": "new_access_token_...",
  "token_type": "Bearer",
  "expires_in": 3600
}
```text

**Response Fields**

| Field | Type | Description |
|-------|------|-------------|
| `access_token` | string | New JWT token for API requests |
| `token_type` | string | Always "Bearer" |
| `expires_in` | number | Seconds until new token expires |

**Error Responses**

| Status | Error | Description |
|--------|-------|-------------|
| 401 | `token_not_found` | Refresh token doesn't exist |
| 401 | `session_revoked` | Session has been revoked |

**Example Usage (JavaScript)**

```javascript
const response = await fetch('/auth/refresh', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    refresh_token: localStorage.getItem('refreshToken')
  })
});

const { access_token, expires_in } = await response.json();
localStorage.setItem('accessToken', access_token);
// Schedule refresh before expiry
setTimeout(() => refreshToken(), expires_in * 1000 - 300000);
```text

---

### 4. POST /auth/logout

Logout and revoke the session.

**Request**

```bash
curl -X POST http://localhost:8000/auth/logout \
  -H "Content-Type: application/json" \
  -d '{
    "refresh_token": "base64_encoded_refresh_token"
  }'
```text

**Request Body**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `refresh_token` | string | No | Token to revoke (if not provided, revokes all) |

**Response (204 No Content)**

No response body.

**Error Responses**

| Status | Error | Description |
|--------|-------|-------------|
| 401 | `session_error` | Session not found |

---

## Authentication

### Bearer Token

Include the access token in the `Authorization` header:

```bash
curl -X POST http://localhost:8000/graphql \
  -H "Authorization: Bearer access_token_..." \
  -H "Content-Type: application/json" \
  -d '{"query": "{ user { id } }"}'
```text

### Optional Authentication

Some endpoints support optional authentication. Missing auth returns results for public data only.

### Token Format

Access tokens are JWT tokens containing:

```json
{
  "sub": "user123",              // User ID from provider
  "iat": 1234567890,             // Issued at timestamp
  "exp": 1234571490,             // Expiration timestamp
  "iss": "https://provider.com", // Issuer URL
  "aud": ["api"],                // Audience
  "email": "user@example.com",   // Custom claims from provider
  "name": "User Name"
}
```text

---

## Error Handling

All errors follow a consistent format:

```json
{
  "errors": [
    {
      "message": "Authentication token has expired",
      "extensions": {
        "code": "token_expired"
      }
    }
  ]
}
```text

### Error Codes

| Code | HTTP Status | Description |
|------|-------------|-------------|
| `token_expired` | 401 | Access token expired |
| `invalid_signature` | 401 | Token signature invalid |
| `invalid_token` | 401 | Token format invalid |
| `token_not_found` | 401 | Refresh token not found |
| `session_revoked` | 401 | Session was revoked |
| `invalid_state` | 400 | CSRF state validation failed |
| `oauth_error` | 500 | OAuth provider error |
| `auth_error` | 500 | Internal authentication error |

---

## Configuration

### Environment Variables

```bash
# OAuth Provider
GOOGLE_CLIENT_ID=...
GOOGLE_CLIENT_SECRET=...
KEYCLOAK_CLIENT_ID=...
KEYCLOAK_CLIENT_SECRET=...
AUTH0_CLIENT_ID=...
AUTH0_CLIENT_SECRET=...

# URLs
OAUTH_REDIRECT_URI=http://localhost:8000/auth/callback
KEYCLOAK_URL=http://localhost:8080
AUTH0_DOMAIN=your-domain.auth0.com

# JWT
JWT_ISSUER=https://provider.com
JWT_ALGORITHM=RS256

# Database
DATABASE_URL=postgres://user:pass@localhost/fraiseql
```text

### Configuration File

Optionally, use a configuration file:

```toml
[auth.google]
issuer = "https://accounts.google.com"
client_id_env = "GOOGLE_CLIENT_ID"
client_secret_env = "GOOGLE_CLIENT_SECRET"
redirect_uri = "http://localhost:8000/auth/callback"

[auth.keycloak]
issuer = "http://localhost:8080/realms/fraiseql"
client_id_env = "KEYCLOAK_CLIENT_ID"
client_secret_env = "KEYCLOAK_CLIENT_SECRET"
redirect_uri = "http://localhost:8000/auth/callback"
```text

---

## Examples

### Complete Login Flow (JavaScript)

```javascript
// 1. Start login
const startResponse = await fetch('/auth/start', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ provider: 'google' })
});

const { authorization_url } = await startResponse.json();

// 2. Redirect user
window.location.href = authorization_url;

// 3. Handle callback (provider redirects to your app)
// Extract tokens from URL or session
const tokens = getTokensFromCallback();
localStorage.setItem('accessToken', tokens.access_token);
localStorage.setItem('refreshToken', tokens.refresh_token);

// 4. Use access token for API requests
const graphqlResponse = await fetch('/graphql', {
  method: 'POST',
  headers: {
    'Authorization': `Bearer ${localStorage.getItem('accessToken')}`,
    'Content-Type': 'application/json'
  },
  body: JSON.stringify({
    query: '{ user { id email name } }'
  })
});

const data = await graphqlResponse.json();
```text

### Token Refresh (JavaScript)

```javascript
async function refreshAccessToken() {
  const refreshToken = localStorage.getItem('refreshToken');

  const response = await fetch('/auth/refresh', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ refresh_token: refreshToken })
  });

  if (response.ok) {
    const { access_token, expires_in } = await response.json();
    localStorage.setItem('accessToken', access_token);

    // Schedule next refresh
    setTimeout(refreshAccessToken, (expires_in - 300) * 1000);
  } else {
    // Refresh failed, require re-login
    logout();
  }
}
```text

### Logout (JavaScript)

```javascript
async function logout() {
  const refreshToken = localStorage.getItem('refreshToken');

  await fetch('/auth/logout', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ refresh_token: refreshToken })
  });

  localStorage.removeItem('accessToken');
  localStorage.removeItem('refreshToken');
  window.location.href = '/';
}
```text

### With Axios

```javascript
import axios from 'axios';

const api = axios.create({
  baseURL: 'http://localhost:8000'
});

// Add token to requests
api.interceptors.request.use(config => {
  const token = localStorage.getItem('accessToken');
  if (token) {
    config.headers.Authorization = `Bearer ${token}`;
  }
  return config;
});

// Handle token expiry
api.interceptors.response.use(
  response => response,
  async error => {
    if (error.response?.status === 401) {
      const refreshed = await refreshAccessToken();
      if (refreshed) {
        // Retry request with new token
        return api.request(error.config);
      } else {
        logout();
      }
    }
    return Promise.reject(error);
  }
);

// Usage
const user = await api.post('/graphql', {
  query: '{ user { id } }'
});
```text

---

## Rate Limiting

Currently no built-in rate limiting. For production, add rate limiting middleware:

```rust
// Example with tower rate limiter
use tower_governor::governor::RateLimiter;

let rate_limiter = RateLimiter::direct(Quota::per_second(100));
let layer = GovernorLayer {
  limiter: rate_limiter,
  error_handler: default_error_handler,
};

app = app.layer(layer);
```text

---

## Security Best Practices

1. **Always use HTTPS** in production
2. **Never expose client secrets** in client code
3. **Store tokens securely**:
   - Use HTTP-only cookies (more secure than localStorage)
   - Or localStorage with content security policy
4. **Handle token expiry** gracefully
5. **Validate redirects** in your app
6. **Log authentication events** for audit trail
7. **Use PKCE** for mobile/native apps (FraiseQL supports this)

---

## Supported Providers

- ✅ Google
- ✅ Keycloak
- ✅ Auth0
- ✅ Any OIDC-compliant provider

To add custom provider, implement `OAuthProvider` trait.

---

## See Also

- [Google OAuth Setup](./SETUP-GOOGLE-OAUTH.md)
- [Keycloak Setup](./SETUP-KEYCLOAK.md)
- [Auth0 Setup](./SETUP-AUTH0.md)
- [Custom SessionStore Implementation](./IMPLEMENT-SESSION-STORE.md)

---

**Last Updated**: 2026-01-21
**Version**: 1.0
