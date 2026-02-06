<!-- Skip to main content -->
---

title: Google OAuth 2.0 Setup Guide
description: This guide walks you through setting up Google OAuth authentication with FraiseQL.
keywords: ["framework", "sdk", "monitoring", "database", "authentication"]
tags: ["documentation", "reference"]
---

# Google OAuth 2.0 Setup Guide

This guide walks you through setting up Google OAuth authentication with FraiseQL.

## Prerequisites

**Required Knowledge:**

- OAuth 2.0 and OIDC fundamentals (authorization code flow, ID tokens, access tokens)
- JWT token structure and claims
- HTTP/REST APIs and callbacks
- Basic networking and DNS (understanding of redirect URIs)
- Google Cloud Console navigation and project management

**Required Software:**

- FraiseQL v2.0.0-alpha.1 or later
- curl or Postman (for testing endpoints)
- A code editor for configuration files
- Bash or similar shell for environment variable setup
- Docker (optional, for testing with ngrok tunneling)

**Required Infrastructure:**

- Active Google Cloud account (free tier available)
- Google Cloud Project (to be created in Step 1)
- FraiseQL server instance (running locally or deployed)
- Publicly accessible URL or ngrok tunnel for OAuth callbacks
- PostgreSQL database for session storage (if using custom SessionStore)

**Optional but Recommended:**

- ngrok or similar tunneling service (for local testing without deployment)
- HTTPS certificate for production (Let's Encrypt or your certificate authority)

**Time Estimate:** 15-30 minutes for complete setup and testing

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
   - User support email: <your-email@example.com>
   - Developer contact: <your-email@example.com>
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
<!-- Code example in BASH -->
# Google OAuth Configuration
GOOGLE_CLIENT_ID=YOUR_CLIENT_ID.apps.googleusercontent.com
GOOGLE_CLIENT_SECRET=YOUR_CLIENT_SECRET
OAUTH_REDIRECT_URI=http://localhost:8000/auth/callback

# JWT Configuration
JWT_ISSUER=https://accounts.google.com
JWT_ALGORITHM=RS256

# Session Configuration
DATABASE_URL=postgres://user:password@localhost/FraiseQL
```text
<!-- Code example in TEXT -->

## Step 5: Update Server Configuration

In your Rust code, configure the OIDC provider:

```rust
<!-- Code example in RUST -->
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
```text
<!-- Code example in TEXT -->

## Step 6: Register Auth Endpoints

Add these routes to your Axum application:

```rust
<!-- Code example in RUST -->
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
```text
<!-- Code example in TEXT -->

## Step 7: Test the Flow

### 1. Start Login Flow

```bash
<!-- Code example in BASH -->
curl -X POST http://localhost:8000/auth/start \
  -H "Content-Type: application/json" \
  -d '{"provider": "google"}'
```text
<!-- Code example in TEXT -->

Response:

```json
<!-- Code example in JSON -->
{
  "authorization_url": "https://accounts.google.com/o/oauth2/v2/auth?client_id=...&state=..."
}
```text
<!-- Code example in TEXT -->

### 2. Visit the Authorization URL

Open the URL in a browser. You'll see Google's login page.

### 3. Complete Login

After authentication, Google will redirect to:

```text
<!-- Code example in TEXT -->
http://localhost:8000/auth/callback?code=...&state=...
```text
<!-- Code example in TEXT -->

This endpoint will:

- Validate the state (CSRF protection)
- Exchange the code for tokens
- Get user info from Google
- Create a session
- Return tokens

Response:

```json
<!-- Code example in JSON -->
{
  "access_token": "access_token_...",
  "refresh_token": "refresh_token_...",
  "token_type": "Bearer",
  "expires_in": 3600
}
```text
<!-- Code example in TEXT -->

### 4. Use the Access Token

```bash
<!-- Code example in BASH -->
curl -X POST http://localhost:8000/graphql \
  -H "Authorization: Bearer access_token_..." \
  -H "Content-Type: application/json" \
  -d '{"query": "{ user { id name } }"}'
```text
<!-- Code example in TEXT -->

### 5. Refresh Token

```bash
<!-- Code example in BASH -->
curl -X POST http://localhost:8000/auth/refresh \
  -H "Content-Type: application/json" \
  -d '{"refresh_token": "refresh_token_..."}'
```text
<!-- Code example in TEXT -->

### 6. Logout

```bash
<!-- Code example in BASH -->
curl -X POST http://localhost:8000/auth/logout \
  -H "Content-Type: application/json" \
  -d '{"refresh_token": "refresh_token_..."}'
```text
<!-- Code example in TEXT -->

## Frontend Integration

### JavaScript/TypeScript

```typescript
<!-- Code example in TypeScript -->
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
```text
<!-- Code example in TEXT -->

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
- [FraiseQL Auth API Reference](./api-reference.md)

## Production Deployment

For production:

1. Update redirect URIs in Google Cloud Console to use `https://yourdomain.com`
2. Set environment variables in your deployment:

   ```bash
<!-- Code example in BASH -->
   GOOGLE_CLIENT_ID=prod.apps.googleusercontent.com
   GOOGLE_CLIENT_SECRET=<secret>
   OAUTH_REDIRECT_URI=https://yourdomain.com/auth/callback
   ```text
<!-- Code example in TEXT -->

3. Ensure database is backed up
4. Enable HTTPS with valid certificate
5. Set secure cookie flags if using cookies
6. Monitor error logs for failed authentications

## Testing with Multiple Accounts

Google allows multiple accounts in development:

```bash
<!-- Code example in BASH -->
# Test with different Google accounts by:
# 1. Using incognito mode for each account
# 2. Or explicitly signing out before each test
# 3. Or using multiple browsers
```text
<!-- Code example in TEXT -->

## Rate Limiting

Google applies rate limits:

- 100,000 requests per day per project
- Most apps won't hit this limit
- If needed, request quota increase in Google Cloud Console

---

**Next Step**: See [API Reference](./api-reference.md) for complete endpoint documentation.
