# Google OAuth 2.0 Setup Guide

This guide walks you through setting up Google OAuth authentication with FraiseQL.

## Prerequisites

- A Google Cloud Project
- Access to Google Cloud Console
- FraiseQL server running on a publicly accessible URL (or ngrok for development)

## Step 1: Create a Google Cloud Project

1. Go to [Google Cloud Console](https://console.cloud.google.com)
2. Click the project dropdown at the top
3. Click "NEW PROJECT"
4. Enter project name: "FraiseQL Auth"
5. Click "CREATE"

## Step 2: Enable Google+ API

1. In the Cloud Console, search for "Google+ API"
2. Click on "Google+ API"
3. Click "ENABLE"

## Step 3: Create OAuth Credentials

1. In the Cloud Console, go to "Credentials" (left sidebar)
2. Click "Create Credentials" → "OAuth client ID"
3. If prompted, configure the OAuth consent screen first:
   - User Type: **External**
   - App name: "FraiseQL"
   - User support email: your-email@example.com
   - Developer contact: your-email@example.com
   - Click "SAVE AND CONTINUE" through all screens
4. Back to OAuth client ID creation:
   - Application type: **Web application**
   - Name: "FraiseQL Server"
   - Authorized JavaScript origins:
     - `http://localhost:3000` (for local development)
     - `https://yourdomain.com` (for production)
   - Authorized redirect URIs:
     - `http://localhost:8000/auth/callback` (dev server)
     - `https://yourdomain.com/auth/callback` (production)
   - Click "CREATE"

5. You'll see your credentials. Note:
   - **Client ID**: `YOUR_CLIENT_ID.apps.googleusercontent.com`
   - **Client Secret**: `YOUR_CLIENT_SECRET`

## Step 4: Configure FraiseQL

Create a `.env` file in your FraiseQL server directory:

```bash
# Google OAuth Configuration
GOOGLE_CLIENT_ID=YOUR_CLIENT_ID.apps.googleusercontent.com
GOOGLE_CLIENT_SECRET=YOUR_CLIENT_SECRET
OAUTH_REDIRECT_URI=http://localhost:8000/auth/callback

# JWT Configuration
JWT_ISSUER=https://accounts.google.com
JWT_ALGORITHM=RS256

# Session Configuration
DATABASE_URL=postgres://user:password@localhost/fraiseql
```

## Step 5: Update Server Configuration

In your Rust code, configure the OIDC provider:

```rust
use fraiseql_server::auth::OidcProvider;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    let oauth_provider = Arc::new(
        OidcProvider::new(
            "google",
            "https://accounts.google.com",
            &std::env::var("GOOGLE_CLIENT_ID")?,
            &std::env::var("GOOGLE_CLIENT_SECRET")?,
            &std::env::var("OAUTH_REDIRECT_URI")?,
        )
        .await?
    );

    // Register auth endpoints with your Axum router
    // ... rest of setup

    Ok(())
}
```

## Step 6: Register Auth Endpoints

Add these routes to your Axum application:

```rust
use axum::{
    routing::{get, post},
    Router,
};
use fraiseql_server::auth::{
    auth_start, auth_callback, auth_refresh, auth_logout,
    AuthState, OidcProvider, PostgresSessionStore,
};

let auth_state = AuthState {
    oauth_provider,
    session_store: Arc::new(PostgresSessionStore::new(db_pool)),
    state_store: Arc::new(dashmap::DashMap::new()),
};

let auth_routes = Router::new()
    .route("/auth/start", post(auth_start))
    .route("/auth/callback", get(auth_callback))
    .route("/auth/refresh", post(auth_refresh))
    .route("/auth/logout", post(auth_logout))
    .with_state(auth_state);

let app = Router::new()
    .merge(auth_routes)
    // ... other routes
```

## Step 7: Test the Flow

### 1. Start Login Flow

```bash
curl -X POST http://localhost:8000/auth/start \
  -H "Content-Type: application/json" \
  -d '{"provider": "google"}'
```

Response:
```json
{
  "authorization_url": "https://accounts.google.com/o/oauth2/v2/auth?client_id=...&state=..."
}
```

### 2. Visit the Authorization URL

Open the URL in a browser. You'll see Google's login page.

### 3. Complete Login

After authentication, Google will redirect to:
```
http://localhost:8000/auth/callback?code=...&state=...
```

This endpoint will:
- Validate the state (CSRF protection)
- Exchange the code for tokens
- Get user info from Google
- Create a session
- Return tokens

Response:
```json
{
  "access_token": "access_token_...",
  "refresh_token": "refresh_token_...",
  "token_type": "Bearer",
  "expires_in": 3600
}
```

### 4. Use the Access Token

```bash
curl -X POST http://localhost:8000/graphql \
  -H "Authorization: Bearer access_token_..." \
  -H "Content-Type: application/json" \
  -d '{"query": "{ user { id name } }"}'
```

### 5. Refresh Token

```bash
curl -X POST http://localhost:8000/auth/refresh \
  -H "Content-Type: application/json" \
  -d '{"refresh_token": "refresh_token_..."}'
```

### 6. Logout

```bash
curl -X POST http://localhost:8000/auth/logout \
  -H "Content-Type: application/json" \
  -d '{"refresh_token": "refresh_token_..."}'
```

## Frontend Integration

### JavaScript/TypeScript

```typescript
// Start login flow
const response = await fetch('http://localhost:8000/auth/start', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ provider: 'google' })
});

const { authorization_url } = await response.json();

// Redirect user to Google
window.location.href = authorization_url;

// After user authenticates, Google redirects to your callback
// Extract tokens from the response and store them
const tokens = getTokensFromResponse(); // from redirect
localStorage.setItem('accessToken', tokens.access_token);
localStorage.setItem('refreshToken', tokens.refresh_token);

// Use tokens for API requests
const graphqlResponse = await fetch('http://localhost:8000/graphql', {
  method: 'POST',
  headers: {
    'Authorization': `Bearer ${localStorage.getItem('accessToken')}`,
    'Content-Type': 'application/json'
  },
  body: JSON.stringify({ query: '{ user { id } }' })
});
```

## Troubleshooting

### Error: "Invalid Redirect URI"

**Cause**: The redirect URI in your request doesn't match the configured URI.

**Solution**:
- Check the exact URL registered in Google Cloud Console
- Ensure `http://` vs `https://` match
- Ensure port number matches
- Check for trailing slashes

### Error: "Client ID mismatch"

**Cause**: Client ID from the request doesn't match the one in Google Cloud.

**Solution**:
- Verify the `GOOGLE_CLIENT_ID` environment variable is set correctly
- Copy-paste from Google Cloud Console (don't type manually)

### Error: "Invalid State"

**Cause**: CSRF protection failed - state parameter doesn't match.

**Solution**:
- This usually means the state cache expired (10 minutes)
- User took too long to authenticate
- Restart the login flow

### Error: "Invalid Code"

**Cause**: Authorization code already used or expired.

**Solution**:
- Authorization codes expire in 10 minutes
- Can only be used once
- Start the login flow again

## Security Considerations

1. **Client Secret**: Never expose in client-side code. Only use on server.
2. **HTTPS**: Always use HTTPS in production. HTTP only for localhost.
3. **Redirect URI**: Only register intended redirect URIs in Google Console.
4. **Token Storage**: Store refresh tokens securely (encrypted database).
5. **Token Expiry**: Access tokens expire in 1 hour by default. Refresh before use.

## Additional Resources

- [Google OAuth 2.0 Documentation](https://developers.google.com/identity/protocols/oauth2)
- [OpenID Connect with Google](https://developers.google.com/identity/openid-connect)
- [FraiseQL Auth API Reference](./API-REFERENCE.md)

## Production Deployment

For production:

1. Update redirect URIs in Google Cloud Console to use `https://yourdomain.com`
2. Set environment variables in your deployment:
   ```bash
   GOOGLE_CLIENT_ID=prod.apps.googleusercontent.com
   GOOGLE_CLIENT_SECRET=<secret>
   OAUTH_REDIRECT_URI=https://yourdomain.com/auth/callback
   ```
3. Ensure database is backed up
4. Enable HTTPS with valid certificate
5. Set secure cookie flags if using cookies
6. Monitor error logs for failed authentications

## Testing with Multiple Accounts

Google allows multiple accounts in development:

```bash
# Test with different Google accounts by:
# 1. Using incognito mode for each account
# 2. Or explicitly signing out before each test
# 3. Or using multiple browsers
```

## Rate Limiting

Google applies rate limits:
- 100,000 requests per day per project
- Most apps won't hit this limit
- If needed, request quota increase in Google Cloud Console

---

**Next Step**: See [API Reference](./API-REFERENCE.md) for complete endpoint documentation.
