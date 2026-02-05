<!-- Skip to main content -->
---
title: Keycloak OAuth 2.0 / OIDC Setup Guide
description: This guide walks you through setting up Keycloak authentication with FraiseQL.
keywords: ["framework", "sdk", "monitoring", "database", "authentication"]
tags: ["documentation", "reference"]
---

# Keycloak OAuth 2.0 / OIDC Setup Guide

This guide walks you through setting up Keycloak authentication with FraiseQL.

## Why Keycloak?

- **Self-hosted**: Full control over authentication infrastructure
- **Open source**: No vendor lock-in
- **Multi-protocol**: OAuth 2.0, OIDC, SAML, LDAP
- **Enterprise features**: Role-based access, user federation, realms
- **Docker**: Easy to run locally or in production

## Prerequisites

**Required Knowledge:**

- OAuth 2.0 and OIDC fundamentals (authorization code flow with PKCE, ID tokens, access tokens, refresh tokens)
- JWT token structure and RS256 signature verification
- Keycloak concepts (realms, clients, scopes, user roles)
- Docker and Docker Compose basics
- HTTP/REST APIs and callback URLs
- Basic networking and DNS resolution

**Required Software:**

- FraiseQL v2.0.0-alpha.1 or later
- Docker 20.10+ and Docker Compose 1.29+ (for local Keycloak)
  - OR: Keycloak 20+ server (if self-hosted separately)
- curl or Postman (for API testing)
- A code editor for configuration files
- Bash or similar shell for environment variables
- PostgreSQL 14+ (included in Docker Compose, or separate instance)

**Required Infrastructure:**

*For Local Development (Docker):*

- Docker daemon running
- ~2GB available disk space for images and volumes
- Port 8080 available for Keycloak UI
- Port 5432 available for PostgreSQL (or modify docker-compose)

*For Production:*

- Keycloak server instance (self-hosted or cloud-hosted)
- PostgreSQL 14+ database for Keycloak state
- PostgreSQL database for FraiseQL session storage
- FraiseQL server instance
- Publicly accessible URL for OAuth callbacks
- Load balancer (optional, for HA)
- TLS/HTTPS certificate

**Optional but Recommended:**

- Keycloak Themes for branding
- Custom Keycloak User Federation for integrating with LDAP/Active Directory
- Keycloak Realm Backup (for production recovery)
- Nginx reverse proxy with SSL (for production)

**Time Estimate:** 25-45 minutes (15 min for Docker setup + 10-30 min for client/realm configuration and testing)

## Option 1: Running Keycloak Locally (Docker)

### Step 1: Start Keycloak with Docker

Create `docker-compose.yml`:

```yaml
<!-- Code example in YAML -->
version: '3.8'
services:
  keycloak:
    image: quay.io/keycloak/keycloak:latest
    environment:
      KEYCLOAK_ADMIN: admin
      KEYCLOAK_ADMIN_PASSWORD: admin123
      KC_DB: postgres
      KC_DB_URL: jdbc:postgresql://postgres:5432/keycloak
      KC_DB_USERNAME: keycloak
      KC_DB_PASSWORD: keycloak123
    ports:
      - "8080:8080"
    command:
      - start-dev
    depends_on:
      - postgres

  postgres:
    image: postgres:15
    environment:
      POSTGRES_DB: keycloak
      POSTGRES_USER: keycloak
      POSTGRES_PASSWORD: keycloak123
    volumes:
      - postgres_data:/var/lib/postgresql/data

volumes:
  postgres_data:
```text
<!-- Code example in TEXT -->

Start it:

```bash
<!-- Code example in BASH -->
docker-compose up -d
```text
<!-- Code example in TEXT -->

Access Keycloak at `http://localhost:8080`

### Step 2: Create Realm

1. Go to `http://localhost:8080`
2. Click "Administration Console"
3. Login with `admin` / `admin123`
4. Hover over "Master" (top left) → Click "Create Realm"
5. Enter realm name: `FraiseQL`
6. Click "Create"

### Step 3: Create Client

1. In the `FraiseQL` realm, go to "Clients" (left sidebar)
2. Click "Create client"
3. Client ID: `FraiseQL-server`
4. Client Protocol: `openid-connect`
5. Click "Next"
6. Enable:
   - ✅ Client authentication
   - ✅ Authorization
7. Click "Next"
8. Root URL: `http://localhost:8000`
9. Valid redirect URIs:
   - `http://localhost:8000/auth/callback`
   - `http://localhost:3000/*` (if frontend on different port)
10. Valid post logout redirect URIs:
    - `http://localhost:3000`
11. Click "Save"

### Step 4: Get Client Secret

1. In the client settings, go to "Credentials" tab
2. Copy the **Client Secret** (you'll need this)

### Step 5: Create Test User (Optional)

1. Go to "Users" (left sidebar)
2. Click "Add user"
3. Username: `testuser`
4. Email: `test@example.com`
5. Click "Create"
6. Go to "Credentials" tab
7. Set password: (click "Set password")
8. Enter password and confirm

## Option 2: Using Managed Keycloak (Production)

If using a hosted Keycloak service:

1. Create realm in your managed Keycloak
2. Create client with your production URLs
3. Note the issuer URL: `https://your-keycloak.example.com/realms/FraiseQL`
4. Follow client setup steps above

## Configure FraiseQL

Create `.env` file:

```bash
<!-- Code example in BASH -->
# Keycloak Configuration
KEYCLOAK_URL=http://localhost:8080
KEYCLOAK_REALM=FraiseQL
KEYCLOAK_CLIENT_ID=FraiseQL-server
KEYCLOAK_CLIENT_SECRET=<copy-from-credentials>
OAUTH_REDIRECT_URI=http://localhost:8000/auth/callback

# JWT Configuration
JWT_ISSUER=http://localhost:8080/realms/FraiseQL
JWT_ALGORITHM=RS256

# Database Configuration
DATABASE_URL=postgres://user:password@localhost/FraiseQL
```text
<!-- Code example in TEXT -->

## Configure FraiseQL Server

```rust
<!-- Code example in RUST -->
use fraiseql_server::auth::OidcProvider;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    let issuer_url = format!(
        "{}/realms/{}",
        std::env::var("KEYCLOAK_URL")?,
        std::env::var("KEYCLOAK_REALM")?
    );

    let oauth_provider = Arc::new(
        OidcProvider::new(
            "keycloak",
            &issuer_url,
            &std::env::var("KEYCLOAK_CLIENT_ID")?,
            &std::env::var("KEYCLOAK_CLIENT_SECRET")?,
            &std::env::var("OAUTH_REDIRECT_URI")?,
        )
        .await?
    );

    // Register auth endpoints...
    Ok(())
}
```text
<!-- Code example in TEXT -->

## Testing the Flow

### 1. Start Login

```bash
<!-- Code example in BASH -->
curl -X POST http://localhost:8000/auth/start \
  -H "Content-Type: application/json" \
  -d '{"provider": "keycloak"}'
```text
<!-- Code example in TEXT -->

### 2. Complete Authentication

Visit the returned authorization URL and log in with your Keycloak account.

### 3. Use Tokens

```bash
<!-- Code example in BASH -->
curl -X POST http://localhost:8000/graphql \
  -H "Authorization: Bearer <access_token>" \
  -H "Content-Type: application/json" \
  -d '{"query": "{ user { id } }"}'
```text
<!-- Code example in TEXT -->

## Advanced: User Federation

Keycloak can federate users from:

- LDAP/Active Directory
- Database (custom providers)
- Other identity providers

To set up LDAP federation:

1. Go to "User Federation" (left sidebar)
2. Click "Add provider" → "ldap"
3. Configure LDAP connection:
   - Vendor: `Active Directory` or `LDAP`
   - Connection URL: `ldap://your-ldap-server`
   - Bind DN: `cn=admin,dc=example,dc=com`
   - Bind credential: (password)
4. Configure user mapping
5. Click "Save"

## Advanced: Custom Roles

Create custom roles for RBAC:

1. Go to "Roles" (left sidebar)
2. Click "Create role"
3. Role name: `api-admin`
4. Click "Create"
5. Assign role to user:
   - Go to "Users"
   - Click user
   - Go to "Role mapping"
   - Assign roles

In your code, check roles:

```rust
<!-- Code example in RUST -->
let user = auth::AuthenticatedUser { /* ... */ };
if user.has_role("api-admin") {
    // Admin access
}
```text
<!-- Code example in TEXT -->

## Troubleshooting

### Error: "Realm not found"

**Cause**: Realm doesn't exist or wrong URL.

**Solution**:

- Verify realm name in Keycloak
- Check `KEYCLOAK_REALM` environment variable
- Try accessing `http://localhost:8080/realms/FraiseQL/.well-known/openid-configuration`

### Error: "Invalid Client"

**Cause**: Client ID or secret is wrong.

**Solution**:

- Verify client ID in Keycloak
- Copy client secret from "Credentials" tab
- Check environment variables match exactly

### Error: "Redirect URI mismatch"

**Cause**: Callback URL doesn't match Keycloak configuration.

**Solution**:

- Update "Valid redirect URIs" in client settings
- Include all redirect URLs (dev, staging, production)
- Check for trailing slashes and protocol (http vs https)

### Keycloak Container Won't Start

**Solution**:

```bash
<!-- Code example in BASH -->
# Check logs
docker-compose logs keycloak

# Restart
docker-compose restart keycloak

# Recreate if needed
docker-compose down
docker-compose up -d
```text
<!-- Code example in TEXT -->

## Production Deployment

For production Keycloak:

1. Use PostgreSQL (not H2)
2. Enable HTTPS with valid certificates
3. Use strong passwords
4. Configure backup and restore procedures
5. Set up monitoring and alerting
6. Use environment-specific realms
7. Enable audit logging

Example production environment:

```bash
<!-- Code example in BASH -->
# .env.prod
KEYCLOAK_URL=https://keycloak.example.com
KEYCLOAK_REALM=production
KEYCLOAK_CLIENT_ID=FraiseQL-prod
KEYCLOAK_CLIENT_SECRET=<strong-secret>
OAUTH_REDIRECT_URI=https://api.example.com/auth/callback

JWT_ISSUER=https://keycloak.example.com/realms/production
JWT_ALGORITHM=RS256

DATABASE_URL=postgres://user:strong-pass@db.internal/FraiseQL
```text
<!-- Code example in TEXT -->

## Using with Docker Compose in Production

Production `docker-compose.yml`:

```yaml
<!-- Code example in YAML -->
version: '3.8'
services:
  keycloak:
    image: quay.io/keycloak/keycloak:latest
    environment:
      KEYCLOAK_ADMIN: ${KEYCLOAK_ADMIN_USER}
      KEYCLOAK_ADMIN_PASSWORD: ${KEYCLOAK_ADMIN_PASSWORD}
      KC_DB: postgres
      KC_DB_URL: jdbc:postgresql://${KC_DB_HOST}:5432/${KC_DB_NAME}
      KC_DB_USERNAME: ${KC_DB_USER}
      KC_DB_PASSWORD: ${KC_DB_PASSWORD}
      KC_PROXY: edge
    ports:
      - "8080:8080"
    command:
      - start
    restart: always
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 30s
      timeout: 10s
      retries: 3

  FraiseQL:
    image: FraiseQL/server:latest
    environment:
      KEYCLOAK_URL: http://keycloak:8080
      # ... other env vars
    depends_on:
      keycloak:
        condition: service_healthy
    ports:
      - "8000:8000"
    restart: always
```text
<!-- Code example in TEXT -->

## Multi-Realm Setup

For different environments, create separate realms:

```text
<!-- Code example in TEXT -->
Keycloak
├── development (uses test users)
├── staging (mirrors production)
└── production (uses enterprise LDAP)
```text
<!-- Code example in TEXT -->

Each realm has:

- Separate clients with different credentials
- Different OIDC configurations
- Environment-specific roles and policies

## See Also

- [Keycloak Documentation](https://www.keycloak.org/documentation)
- [Keycloak Admin Guide](https://www.keycloak.org/docs/latest/server_admin/)
- [FraiseQL API Reference](./API-REFERENCE.md)

---

**Next Step**: See [API Reference](./API-REFERENCE.md) for complete endpoint documentation.
