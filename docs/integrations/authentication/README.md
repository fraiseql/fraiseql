# FraiseQL Authentication System - Complete Documentation

Welcome to FraiseQL's comprehensive OAuth 2.0 / OIDC authentication system. This directory contains complete documentation for implementing, deploying, and maintaining authentication in FraiseQL.

## Prerequisites

**Required Knowledge:**

- OAuth 2.0 and OIDC fundamentals
- JWT token structure and validation
- HTTP/REST APIs
- Basic networking and DNS
- Your chosen auth provider's console/admin panel

**Required Tools:**

- FraiseQL v2.0.0-alpha.1 or later
- curl or Postman (for API testing)
- Configured database (PostgreSQL 14+)
- Node.js or Python (for SDK examples)

**For Each Provider:**

**Google OAuth:**

- Google Cloud Console account with project
- OAuth 2.0 credentials (Client ID and Secret)
- Registered redirect URIs

**Auth0:**

- Auth0 account (free tier available)
- Auth0 application created
- Auth0 API configured

**Keycloak:**

- Keycloak server deployed (self-hosted)
- Realm and client created
- Admin access

**SCRAM:**

- FraiseQL configured with SCRAM support
- Username/password credentials
- TLS enabled for security

**Time Estimate:** 30 minutes to 2 hours depending on provider choice

---

## 📚 Documentation Structure

### Quick Start

1. **First Time Setup?** → Choose your provider:
   - [Google OAuth Setup](./SETUP-GOOGLE-OAUTH.md) - Recommended for quick testing
   - [Keycloak Setup](./SETUP-KEYCLOAK.md) - Self-hosted option
   - [Auth0 Setup](./SETUP-AUTH0.md) - Managed service option

2. **Need API Details?** → [API Reference](./API-REFERENCE.md)
   - Complete endpoint documentation
   - Request/response formats
   - Error codes and handling
   - JavaScript/Axios examples

### Implementation

- **[API Reference](./API-REFERENCE.md)** - Complete endpoint documentation
  - All HTTP endpoints with examples
  - Error handling patterns
  - JavaScript integration examples

- **[Custom SessionStore Implementation](./IMPLEMENT-SESSION-STORE.md)** - Build your own backend
  - Redis implementation (full code)
  - DynamoDB implementation (full code)
  - MongoDB implementation (full code)
  - Best practices and testing

### Deployment & Operations

- **[Deployment Guide](./DEPLOYMENT.md)** - Production deployment
  - Docker Compose setup
  - Kubernetes manifests
  - Nginx reverse proxy
  - SSL/TLS with Let's Encrypt
  - Database setup and backups
  - Scaling and high availability

- **[Monitoring Guide](./MONITORING.md)** - Observability
  - Structured logging (JSON)
  - Prometheus metrics
  - Grafana dashboards
  - Health checks
  - Alert rules

- **[Troubleshooting Guide](./TROUBLESHOOTING.md)** - Common issues
  - Login flow problems
  - Token issues
  - Database connectivity
  - Performance optimization
  - OAuth provider debugging

### Security & Compliance

- **[Security Checklist](./SECURITY-CHECKLIST.md)** - Production security
  - 100+ point security audit
  - OAuth provider specifics
  - GDPR / SOC2 / PCI-DSS
  - Incident response
  - Sign-off templates

---

## 🚀 Getting Started (5 minutes)

### Step 1: Choose OAuth Provider

```bash
# Option 1: Google (easiest for testing)
# https://console.cloud.google.com
# Create OAuth app, get credentials

# Option 2: Keycloak (self-hosted)
# docker-compose up keycloak

# Option 3: Auth0 (managed service)
# https://manage.auth0.com
```text

### Step 2: Configure FraiseQL

```bash
# .env file
GOOGLE_CLIENT_ID=...
GOOGLE_CLIENT_SECRET=...
OAUTH_REDIRECT_URI=http://localhost:8000/auth/callback
JWT_ISSUER=https://accounts.google.com
DATABASE_URL=postgres://user:pass@localhost/fraiseql
```text

### Step 3: Register Endpoints

```rust
use fraiseql_server::auth::{auth_start, auth_callback, auth_refresh, auth_logout};

let auth_routes = Router::new()
    .route("/auth/start", post(auth_start))
    .route("/auth/callback", get(auth_callback))
    .route("/auth/refresh", post(auth_refresh))
    .route("/auth/logout", post(auth_logout))
    .with_state(auth_state);
```text

### Step 4: Test the Flow

```bash
# Start login
curl -X POST http://localhost:8000/auth/start \
  -H "Content-Type: application/json" \
  -d '{"provider": "google"}' | jq .authorization_url

# Visit the URL in browser, authenticate
# Tokens returned in callback response
```text

---

## 📖 Documentation Guide

### By Role

**Developers**

- Start: [API Reference](./API-REFERENCE.md)
- Then: [Setup Guide](./SETUP-GOOGLE-OAUTH.md) for your provider
- Reference: [Custom SessionStore](./IMPLEMENT-SESSION-STORE.md) if needed

**DevOps / Site Reliability Engineers**

- Start: [Deployment Guide](./DEPLOYMENT.md)
- Then: [Monitoring Guide](./MONITORING.md)
- Reference: [Troubleshooting](./TROUBLESHOOTING.md)

**Security / Compliance Teams**

- Start: [Security Checklist](./SECURITY-CHECKLIST.md)
- Reference: [Deployment Guide](./DEPLOYMENT.md) for architecture

**Support / Operations**

- Start: [Troubleshooting Guide](./TROUBLESHOOTING.md)
- Reference: [Monitoring Guide](./MONITORING.md) for dashboards

### By Task

| Task | Document |
|------|----------|
| Set up OAuth with Google | [SETUP-GOOGLE-OAUTH.md](./SETUP-GOOGLE-OAUTH.md) |
| Set up Keycloak | [SETUP-KEYCLOAK.md](./SETUP-KEYCLOAK.md) |
| Set up Auth0 | [SETUP-AUTH0.md](./SETUP-AUTH0.md) |
| Call auth endpoints | [API-REFERENCE.md](./API-REFERENCE.md) |
| Build custom session store | [IMPLEMENT-SESSION-STORE.md](./IMPLEMENT-SESSION-STORE.md) |
| Deploy to production | [DEPLOYMENT.md](./DEPLOYMENT.md) |
| Set up monitoring | [MONITORING.md](./MONITORING.md) |
| Debug issues | [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) |
| Pass security audit | [SECURITY-CHECKLIST.md](./SECURITY-CHECKLIST.md) |

---

## 🔑 Key Features

✅ **Multiple OAuth Providers**

- Google OAuth 2.0 / OIDC
- Keycloak
- Auth0
- Any OIDC-compliant provider

✅ **Security First**

- PKCE for authorization code flow
- CSRF protection via state parameter
- JWT signature verification
- Token hashing in storage
- Session revocation

✅ **Production Ready**

- Structured logging (JSON)
- Prometheus metrics
- Health checks
- Kubernetes deployment
- Docker Compose setup
- Database backups

✅ **Extensible**

- Pluggable `SessionStore` trait
- Pluggable `OAuthProvider` trait
- Custom provider implementations
- Multiple session backends (PostgreSQL, Redis, DynamoDB, MongoDB)

✅ **Well Documented**

- Setup guides for all major providers
- Complete API reference
- Implementation examples
- Deployment guide
- Security best practices
- Troubleshooting guide

---

## 📊 Architecture Overview

```text
┌─ Client (Browser/App)
│
├─ POST /auth/start
│  └─ Returns authorization URL
│
├─ Redirects to OAuth Provider
│  └─ User authenticates
│
├─ GET /auth/callback?code=...&state=...
│  ├─ Validates state (CSRF)
│  ├─ Exchanges code for tokens
│  ├─ Gets user info
│  ├─ Creates session
│  └─ Returns access & refresh tokens
│
├─ Subsequent API Requests
│  ├─ Authorization: Bearer <access_token>
│  ├─ JWT Validator verifies signature
│  ├─ Session Manager validates session
│  └─ Request proceeds with authenticated user
│
├─ POST /auth/refresh
│  ├─ Validates refresh token
│  ├─ Creates new access token
│  └─ Returns new token
│
└─ POST /auth/logout
   ├─ Revokes session
   └─ User logged out
```text

---

## 🧪 Testing

### Manual Testing

```bash
# Start login flow
curl -X POST http://localhost:8000/auth/start \
  -H "Content-Type: application/json" \
  -d '{"provider": "google"}'

# Get authorization URL, visit in browser
# Complete OAuth flow
# Receive tokens in response

# Use access token
curl -X POST http://localhost:8000/graphql \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"query": "{ user { id } }"}'
```text

### Automated Testing

```bash
# Run all auth tests
cargo test -p fraiseql-server auth:: --lib

# Run with logging
RUST_LOG=debug cargo test -p fraiseql-server auth:: --lib -- --nocapture
```text

---

## 🚨 Common Issues

### "Invalid Redirect URI"

See: [Troubleshooting](./TROUBLESHOOTING.md#invalid-redirect-uri-error)

### "Invalid State"

See: [Troubleshooting](./TROUBLESHOOTING.md#invalid-state-error)

### "Token Expired"

See: [Troubleshooting](./TROUBLESHOOTING.md#token-expired-on-valid-token)

### Database Connection Issues

See: [Troubleshooting](./TROUBLESHOOTING.md#database-issues)

---

## 📋 Checklist for Deployment

- [ ] Review [Security Checklist](./SECURITY-CHECKLIST.md)
- [ ] Set up OAuth provider (Google/Keycloak/Auth0)
- [ ] Configure environment variables
- [ ] Set up PostgreSQL database
- [ ] Run database migrations
- [ ] Configure HTTPS/TLS
- [ ] Set up monitoring and alerts
- [ ] Test complete OAuth flow
- [ ] Load test authentication endpoints
- [ ] Review logs for errors
- [ ] Get security sign-off
- [ ] Deploy to production

---

## 🔧 Configuration Reference

### Environment Variables

```bash
# OAuth Provider Credentials (all required)
GOOGLE_CLIENT_ID=...                    # From Google Cloud Console
GOOGLE_CLIENT_SECRET=...                # From Google Cloud Console
OAUTH_REDIRECT_URI=https://...          # Exact match with provider

# JWT Configuration
JWT_ISSUER=https://accounts.google.com  # From OAuth provider
JWT_ALGORITHM=RS256                     # Usually RS256

# Database
DATABASE_URL=postgres://user:pass@host/db
DATABASE_POOL_SIZE=20

# Logging
RUST_LOG=info,fraiseql_server::auth=debug
```text

### Docker Environment

See [Deployment Guide](./DEPLOYMENT.md#docker-deployment) for complete setup

### Kubernetes Configuration

See [Deployment Guide](./DEPLOYMENT.md#kubernetes-deployment) for manifests

---

## 🆘 Getting Help

1. **Check the docs** - Most questions covered in documentation
2. **Check troubleshooting** - [Troubleshooting Guide](./TROUBLESHOOTING.md)
3. **Enable debug logging** - `RUST_LOG=debug`
4. **Create GitHub issue** - <https://github.com/fraiseql/fraiseql/issues>
   - Include error message (no secrets)
   - Steps to reproduce
   - Environment details
   - Logs with debug enabled

---

## 📈 Performance Characteristics

| Operation | Latency | Alert Threshold |
|-----------|---------|-----------------|
| JWT Validation | 1-5ms | >10ms |
| Session Lookup | 5-50ms | >100ms |
| OAuth Token Exchange | 200-500ms | >1000ms |
| User Info Retrieval | 100-300ms | >500ms |

---

## 🔐 Security Considerations

- **Always use HTTPS** in production
- **Never expose secrets** in client code
- **Store tokens securely** (HTTP-only cookies)
- **Handle token expiry** gracefully
- **Validate redirects** in your app
- **Monitor authentication** events
- **Rotate secrets** regularly
- **Use strong passwords** for database

See [Security Checklist](./SECURITY-CHECKLIST.md) for complete list.

---

## 📚 Additional Resources

- [OAuth 2.0 Specification](https://tools.ietf.org/html/rfc6749)
- [OpenID Connect Specification](https://openid.net/specs/openid-connect-core-1_0.html)
- [PKCE (RFC 7636)](https://tools.ietf.org/html/rfc7636)
- [JWT Best Practices (RFC 8725)](https://tools.ietf.org/html/rfc8725)

---

## 📝 Document Index

| Document | Length | Purpose |
|----------|--------|---------|
| SETUP-GOOGLE-OAUTH.md | 400 lines | Google OAuth setup |
| SETUP-KEYCLOAK.md | 350 lines | Self-hosted OIDC |
| SETUP-AUTH0.md | 400 lines | Managed OIDC |
| API-REFERENCE.md | 500 lines | Complete API docs |
| IMPLEMENT-SESSION-STORE.md | 600 lines | Custom backends (3 examples) |
| DEPLOYMENT.md | 500 lines | Production deployment |
| MONITORING.md | 350 lines | Observability setup |
| SECURITY-CHECKLIST.md | 450 lines | Security audit |
| TROUBLESHOOTING.md | 450 lines | Common issues |
| README.md | This file | Overview |

**Total: 3,000+ lines of documentation**

---

## 🎯 Next Steps

1. **Choose your OAuth provider** (Google recommended for quick start)
2. **Follow the setup guide** for that provider
3. **Set up database** and run migrations
4. **Test the complete flow** manually
5. **Deploy to production** following deployment guide
6. **Set up monitoring** for operations
7. **Run through security checklist** before going live

---

**Version**: 1.0
**Last Updated**: 2026-01-21
**Status**: Production Ready ✅

---

See [Security Checklist](./SECURITY-CHECKLIST.md) for deployment sign-off template.

---

## See Also

- **[Federation Guide](../federation/guide.md)** - Multi-subgraph authentication coordination
- **[Production Deployment](../../guides/production-deployment.md)** - Deploying authentication in production
- **[Security Model](../../architecture/security/security-model.md)** - Authentication architecture deep dive
- **[Observability](./MONITORING.md)** - Monitoring authentication events
- **[Troubleshooting](./TROUBLESHOOTING.md)** - Common authentication issues
