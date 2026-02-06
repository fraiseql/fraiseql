<!-- Skip to main content -->
---

title: Auth0 OAuth 2.0 / OIDC Setup Guide
description: This guide walks you through setting up Auth0 authentication with FraiseQL.
keywords: ["framework", "sdk", "monitoring", "database", "authentication"]
tags: ["documentation", "reference"]
---

# Auth0 OAuth 2.0 / OIDC Setup Guide

This guide walks you through setting up Auth0 authentication with FraiseQL.

## Why Auth0?

- **Managed service**: No infrastructure to maintain
- **Enterprise-grade**: Proven by thousands of companies
- **Fast setup**: Minutes to configure
- **Rich features**: MFA, social login, passwordless auth
- **Scalability**: Handles millions of authentications

## Prerequisites

**Required Knowledge:**

- OAuth 2.0 and OIDC fundamentals (client credentials flow, authorization code flow, ID tokens, access tokens)
- JWT token structure and RS256 signature verification
- HTTP/REST APIs and callback handling
- Auth0 tenant management and application concepts
- Basic networking and domain/URL understanding

**Required Software:**

- FraiseQL v2.0.0-alpha.1 or later
- curl or Postman (for API testing)
- A code editor for configuration files
- Bash or similar shell for environment variables
- Docker (optional, for local testing with ngrok)

**Required Infrastructure:**

- Auth0 account (free tier available at <https://auth0.com/signup>)
- Auth0 tenant/domain (created automatically with account)
- FraiseQL server instance (local or deployed)
- Publicly accessible URL or ngrok tunnel for OAuth callbacks
- PostgreSQL database for session storage (if using custom SessionStore)
- Auth0 Application (to be created in Step 1)
- Auth0 API definition (to be created in Step 3)

**Optional but Recommended:**

- ngrok or Cloudflare Tunnel (for local development without deployment)
- Auth0 Rules/Actions for custom authorization logic
- Auth0 Logs page for debugging authentication issues
- HTTPS certificate for production deployment

**Time Estimate:** 20-40 minutes for complete setup and first user authentication test

## Step 1: Create Auth0 Application

1. Go to [Auth0 Dashboard](https://manage.auth0.com)
2. Click "Applications" (left sidebar)
3. Click "Create Application"
4. Enter name: "FraiseQL Server"
5. Choose application type: **Regular Web Application**
6. Click "Create"

## Step 2: Configure Application Settings

1. In the application settings, go to "Settings" tab
2. Find these important values:
   - **Domain**: `your-domain.auth0.com`
   - **Client ID**: (copy this)
   - **Client Secret**: (copy this)

3. Scroll down to "Allowed Callback URLs" and add:

   ```text
<!-- Code example in TEXT -->
   http://localhost:8000/auth/callback
   https://yourdomain.com/auth/callback

   ```text
<!-- Code example in TEXT -->

4. Scroll to "Allowed Logout URLs" and add:

   ```text
<!-- Code example in TEXT -->
   http://localhost:3000
   https://yourdomain.com
   ```text
<!-- Code example in TEXT -->

5. Scroll to "Allowed Web Origins" and add:

   ```text
<!-- Code example in TEXT -->
   http://localhost:3000
   http://localhost:8000
   https://yourdomain.com
   ```text
<!-- Code example in TEXT -->

6. Click "Save Changes"

## Step 3: Create API (for access tokens)

1. Click "Applications" → "APIs" (left sidebar)
2. Click "Create API"
3. Enter name: "FraiseQL API"
4. Identifier: `https://api.FraiseQL.example.com`
5. Signing algorithm: **RS256** (default)
6. Click "Create"

## Step 4: Configure FraiseQL

Create `.env` file:

```bash
<!-- Code example in BASH -->
# Auth0 Configuration
AUTH0_DOMAIN=your-domain.auth0.com
AUTH0_CLIENT_ID=YOUR_CLIENT_ID
AUTH0_CLIENT_SECRET=YOUR_CLIENT_SECRET
OAUTH_REDIRECT_URI=http://localhost:8000/auth/callback

# JWT Configuration (Auth0 uses RS256 automatically)
JWT_ISSUER=https://your-domain.auth0.com/
JWT_ALGORITHM=RS256

# Database Configuration
DATABASE_URL=postgres://user:password@localhost/FraiseQL
```text
<!-- Code example in TEXT -->

## Step 5: Configure FraiseQL Server

```rust
<!-- Code example in RUST -->
use fraiseql_server::auth::OidcProvider;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    let domain = std::env::var("AUTH0_DOMAIN")?;
    let issuer_url = format!("https://{}/", domain);

    let oauth_provider = Arc::new(
        OidcProvider::new(
            "auth0",
            &issuer_url,
            &std::env::var("AUTH0_CLIENT_ID")?,
            &std::env::var("AUTH0_CLIENT_SECRET")?,
            &std::env::var("OAUTH_REDIRECT_URI")?,
        )
        .await?
    );

    // Register auth endpoints...
    Ok(())
}
```text
<!-- Code example in TEXT -->

## Step 6: Register Auth Endpoints

```rust
<!-- Code example in RUST -->
use axum::Router;
use fraiseql_server::auth::{
    auth_start, auth_callback, auth_refresh, auth_logout, AuthState
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

## Testing

### 1. Start Login Flow

```bash
<!-- Code example in BASH -->
curl -X POST http://localhost:8000/auth/start \
  -H "Content-Type: application/json" \
  -d '{"provider": "auth0"}'
```text
<!-- Code example in TEXT -->

### 2. Complete Authentication

Visit the returned authorization URL and log in with your Auth0 account.

### 3. Use Tokens

```bash
<!-- Code example in BASH -->
curl -X POST http://localhost:8000/graphql \
  -H "Authorization: Bearer <access_token>" \
  -H "Content-Type: application/json" \
  -d '{"query": "{ user { id } }"}'
```text
<!-- Code example in TEXT -->

## Advanced: User Management

Auth0 provides user management APIs. Access them via:

```rust
<!-- Code example in RUST -->
// Get user profile from Auth0 user info endpoint
// This is automatically done by OidcProvider::user_info()

let user_info = oauth_provider.user_info(&access_token).await?;
println!("User ID: {}", user_info.id);
println!("Email: {}", user_info.email);
```text
<!-- Code example in TEXT -->

## Advanced: Rules and Actions

Auth0 Rules (legacy) or Actions allow custom logic:

1. Go to "Auth Pipeline" → "Actions"
2. Click "Create Action"
3. Name: "Add Custom Claims"
4. Trigger: "Post-Login"
5. Add code:

   ```javascript
<!-- Code example in JAVASCRIPT -->
   exports.onExecutePostLogin = async (event, api) => {
     const namespace = 'https://FraiseQL.example.com';
     if (event.authorization) {
       api.idToken.setCustomClaim(`${namespace}/roles`, event.user.roles);
       api.accessToken.setCustomClaim(`${namespace}/org_id`, event.user.org_id);
     }
   };
   ```text
<!-- Code example in TEXT -->

6. Click "Save" → "Deploy"

Then access in FraiseQL:

```rust
<!-- Code example in RUST -->
let user = auth::AuthenticatedUser { /* ... */ };
let roles = user.get_custom_claim("https://FraiseQL.example.com/roles");
let org_id = user.get_custom_claim("https://FraiseQL.example.com/org_id");
```text
<!-- Code example in TEXT -->

## Advanced: Social Login

Auth0 supports social login (Google, GitHub, etc.). To enable:

1. Go to "Authentication" → "Social"
2. Enable desired providers (Google, GitHub, LinkedIn, etc.)
3. Provide credentials for each provider
4. Auth0 handles the OAuth flow automatically

Your users can now log in with their social accounts!

## Advanced: Multi-Factor Authentication (MFA)

To require MFA:

1. Go to "Security" → "Multi-factor Authentication"
2. Enable desired factors (SMS, Email, Google Authenticator)
3. Configure enrollment
4. Auth0 will prompt for MFA during login

## Advanced: Roles and Permissions

Set up role-based access control:

1. Go to "User Management" → "Roles"
2. Click "Create Role"
3. Name: `admin`
4. Description: "Administrator role"
5. Add permissions:
   - `read:data`
   - `write:data`
   - `delete:data`
6. Assign role to user:
   - Go to "Users"
   - Click user
   - Go to "Roles" tab
   - Add roles

In FraiseQL, check roles:

```rust
<!-- Code example in RUST -->
let user = auth::AuthenticatedUser { /* ... */ };
if user.has_role("admin") {
    // Admin logic
}
```text
<!-- Code example in TEXT -->

## Troubleshooting

### Error: "Invalid Client"

**Cause**: Client ID or secret is incorrect.

**Solution**:

- Copy from Auth0 dashboard exactly
- Verify environment variables
- Check no spaces or special characters

### Error: "Redirect URI Mismatch"

**Cause**: Callback URL doesn't match Auth0 configuration.

**Solution**:

- Update "Allowed Callback URLs" in settings
- Check for `http://` vs `https://`
- Verify port number matches
- Check for trailing slashes

### Error: "Invalid Scope"

**Cause**: Requested scope not available.

**Solution**:

- Auth0 by default supports: `openid profile email`
- For custom scopes, define in API settings
- Make sure API is connected to application

### Users Can't Log In

**Cause**: Auth0 application not accessible or database connection failed.

**Solution**:

```bash
<!-- Code example in BASH -->
# Test Auth0 connectivity
curl https://your-domain.auth0.com/.well-known/openid-configuration

# Check FraiseQL logs for errors
# Verify DATABASE_URL is correct
```text
<!-- Code example in TEXT -->

## Production Deployment

### Environment Configuration

```bash
<!-- Code example in BASH -->
# .env.prod
AUTH0_DOMAIN=your-domain.auth0.com
AUTH0_CLIENT_ID=<prod-client-id>
AUTH0_CLIENT_SECRET=<strong-secret>
OAUTH_REDIRECT_URI=https://api.example.com/auth/callback

JWT_ISSUER=https://your-domain.auth0.com/
JWT_ALGORITHM=RS256

DATABASE_URL=postgres://user:pass@prod-db/FraiseQL
```text
<!-- Code example in TEXT -->

### Auth0 Tenant Configuration

1. Go to "Tenant Settings"
2. Update friendly name for production
3. Enable "Allow Impersonation" (optional)
4. Configure session timeout
5. Set custom domain (optional but recommended):
   - Use custom domain like `auth.example.com` instead of `your-domain.auth0.com`
   - Reduces vendor lock-in risk

### Monitoring

Auth0 provides logs:

1. Go to "Monitoring" → "Logs"
2. View all authentication events
3. Set up webhooks for real-time notifications
4. Export logs to your analytics system

### Backup and Disaster Recovery

Auth0 manages backups, but you should:

1. Regularly export user data
2. Keep credentials in secure vault
3. Document configurations
4. Test restore procedures

## Cost

Auth0 pricing:

- **Free tier**: Up to 7,000 active users
- **Pro**: Pay-as-you-go, starts around $13/month
- **Enterprise**: Custom pricing

Most applications fit in the free tier initially.

## See Also

- [Auth0 Documentation](https://auth0.com/docs)
- [Auth0 API Reference](https://auth0.com/docs/api)
- [Auth0 Rules & Actions](https://auth0.com/docs/get-started/actions)
- [FraiseQL API Reference](./api-reference.md)

---

**Next Step**: See [API Reference](./api-reference.md) for complete endpoint documentation.
