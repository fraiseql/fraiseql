# Security Configuration Guide

This guide covers security features and configuration options available in FraiseQL v2.0.0-alpha.1.

## Overview of Security Features

FraiseQL adopts a **fail-secure** approach to security configuration, prioritizing production safety through secure defaults and explicit opt-in for public features:

| Feature | Default | Why |
|---------|---------|-----|
| Playground | **Disabled** | Schema exposure protection |
| Introspection | **Disabled** | Schema exposure protection |
| Admin API | **Disabled** | Critical operation protection |
| JWT Audience | **Required** | Token confusion prevention |
| CORS Origins | **Validated in production** | CSRF protection |
| Rate Limiting | **Enabled** | DoS/abuse protection |

## Configuration Requirements

### JWT Audience Validation (Required)

**When**: If you're using OIDC authentication.

**Why**: This prevents token confusion attacks where tokens from one service can be used in another.

**Configuration**:

```toml
[auth]
issuer = "https://tenant.auth0.com/"
audience = "https://api.example.com"  # Required for OIDC
```text

**Migration Steps**:

1. Identify your API identifier (typically a URL like `https://api.example.com` or a simple ID like `my-api`)
2. Add `audience = "YOUR_API_IDENTIFIER"` to your `[auth]` section
3. Restart your server - it will fail with a clear error if missing

**Error you might see**:

```text
OIDC audience is REQUIRED for security. Set 'audience' in auth config
to your API identifier. This prevents token confusion attacks...
```text

### Admin Endpoints Configuration

**Status**: Disabled by default. Enable only when needed.

**Available admin endpoints**:

- `/api/v1/admin/reload-schema` - Reload GraphQL schema
- `/api/v1/admin/cache/clear` - Clear caches
- `/api/v1/admin/config` - View server configuration

**To enable admin endpoints**:

If you need admin endpoints:

```toml
[server]
admin_api_enabled = true
admin_token = "your-secure-token-32-characters-minimum!!"
```text

Or via environment variables:

```bash
FRAISEQL_ADMIN_API_ENABLED=true
FRAISEQL_ADMIN_TOKEN="your-secure-token-32-characters-minimum!!"
```text

Then authenticate requests:

```bash
curl -H "Authorization: Bearer your-secure-token-32-characters-minimum!!" \
     http://localhost:8000/api/v1/admin/config
```text

### Introspection Configuration

**Status**: Disabled by default. Enable based on environment needs.

**Options:**

- **Disabled** (default, recommended for production): Introspection endpoint not available
- **Enabled without auth** (development): Schema available to all requests
- **Enabled with auth** (production): Requires OIDC authentication

**For Development** (public schema):

```toml
[server]
introspection_enabled = true
introspection_require_auth = false  # Schema public (dev only!)
```text

**For Production** (enable with auth):

```toml
[server]
introspection_enabled = true
introspection_require_auth = true   # Requires OIDC auth
```text

Environment variables:

```bash
FRAISEQL_INTROSPECTION_ENABLED=true
FRAISEQL_INTROSPECTION_REQUIRE_AUTH=true  # In production
```text

**Note**: Schema export endpoints (`/api/v1/schema.graphql`, `/api/v1/schema.json`) follow the same configuration as introspection.

### GraphQL Playground Configuration

**Status**: Disabled by default. Development-only feature.

**When to enable**: Local development environments only.

**To enable** (development only):

```toml
[server]
playground_enabled = true
```text

Or environment variable:

```bash
FRAISEQL_PLAYGROUND_ENABLED=true
```text

**Migration**: If you're using playground, explicitly enable it in your dev configs.

### CORS Configuration

**Behavior**:

- **Development mode** (FRAISEQL_ENV=development): CORS validation relaxed
- **Production mode** (default): CORS must be explicitly configured

**Configuration**:

```toml
[server]
cors_enabled = true
cors_origins = ["https://app.example.com", "https://api.example.com"]
```text

**Important**: In production mode, you must explicitly set allowed origins. Empty origins would allow requests from ANY origin, which is a security risk.

## Optional Features

### Design API Authentication

**Status**: Public by default. Optional authentication available.

Design audit endpoints (`/api/v1/design/*`) support optional authentication.

**Default**: Public (no auth required)

**To require authentication**:

```toml
[server]
design_api_require_auth = true  # Requires OIDC authentication
```text

The endpoints remain public unless you explicitly enable authentication.

## Configuration Steps

### Step 1: Assess Your Needs

- [ ] Do you need introspection for tooling? (Apollo Studio, etc.)
- [ ] Do you need admin endpoints? (schema reloading, cache management)
- [ ] Do you need playground? (development only)
- [ ] What are your CORS origins? (production deployment targets)

### Step 2: Configure Security

- [ ] Add `audience` field to your `[auth]` section (required for OIDC)
- [ ] If you use admin endpoints: add `admin_api_enabled` and `admin_token`
- [ ] If you use introspection: add `introspection_enabled` and `introspection_require_auth`
- [ ] If you use playground: add `playground_enabled = true` (dev only)
- [ ] Set explicit `cors_origins` for your deployment environment

### Step 3: Deployment Setup

- [ ] Production: Set `FRAISEQL_ENV=production` or leave unset (default)
- [ ] Development: Optionally set `FRAISEQL_ENV=development`
- [ ] Use strong tokens for `admin_token` (32+ characters minimum)
- [ ] Store sensitive tokens in environment variables, not config files

### Step 4: Verify Configuration

- [ ] Test GraphQL queries to endpoint
- [ ] Test authentication flow with audience validation
- [ ] Verify admin endpoints return 404 if not enabled
- [ ] Verify introspection returns 404 if not enabled
- [ ] Check logs for security configuration warnings

## Configuration Examples

### Development Environment

Best practices for local development:

```toml
# FraiseQL.toml (or export as environment variables)
[server]
playground_enabled = true
introspection_enabled = true
introspection_require_auth = false
cors_enabled = true
cors_origins = ["http://localhost:3000", "http://localhost:5173"]

[auth]
issuer = "https://your-tenant.auth0.com/"
audience = "localhost"  # Use "localhost" for local dev
```text

Or set environment variables:

```bash
export FRAISEQL_ENV=development
export FRAISEQL_PLAYGROUND_ENABLED=true
export FRAISEQL_INTROSPECTION_ENABLED=true
```text

### Production Environment

Secure defaults for production deployments:

```toml
# production.toml
[server]
playground_enabled = false
introspection_enabled = true          # Optional: set to false if not needed
introspection_require_auth = true     # If introspection is enabled
admin_api_enabled = true              # Only if needed for your operations
admin_token = "your-secure-32+-char-token-minimum!!"
cors_enabled = true
cors_origins = ["https://app.example.com", "https://another-app.example.com"]

[auth]
issuer = "https://your-tenant.auth0.com/"
audience = "https://api.example.com"  # Your API identifier
```text

Or set environment variables:

```bash
export FRAISEQL_ENV=production  # Or don't set (production is default)
export FRAISEQL_ADMIN_TOKEN="your-secure-32+-char-token"
export FRAISEQL_ADMIN_API_ENABLED=true
```text

## Troubleshooting

### "OIDC audience is REQUIRED for security"

**Solution**: Add the `audience` field to your `[auth]` section:

```toml
[auth]
audience = "your-api-identifier"
```text

### "playground_enabled is true in production mode"

**Solution**: Either disable playground or set `FRAISEQL_ENV=development`:

```bash
# Option 1: Disable
playground_enabled = false

# Option 2: Development mode
export FRAISEQL_ENV=development
```text

### "cors_origins is empty in production mode"

**Solution**: Explicitly configure CORS origins:

```toml
[server]
cors_origins = ["https://app.example.com"]
```text

### Admin endpoints return 404

**Solution**: Enable admin API:

```toml
[server]
admin_api_enabled = true
admin_token = "your-secure-token-32-char-minimum!!"
```text

### Introspection endpoint returns 404

**Solution**: Enable introspection:

```toml
[server]
introspection_enabled = true
introspection_require_auth = false  # For dev; true for production
```text

## Rate Limiting

FraiseQL includes built-in rate limiting to protect GraphQL endpoints from abuse and denial-of-service attacks.

### Overview

Rate limiting is **enabled by default** with sensible per-IP and per-user limits using a token bucket algorithm:

| Setting | Default | Description |
|---------|---------|-------------|
| `enabled` | `true` | Rate limiting enabled for security |
| `rps_per_ip` | 100 | Requests per second per IP address |
| `rps_per_user` | 1000 | Requests per second per authenticated user |
| `burst_size` | 500 | Maximum tokens to accumulate |

### Configuration

**In TOML file** (`FraiseQL.toml`):

```toml
[rate_limiting]
enabled = true
rps_per_ip = 100         # Adjust per your traffic patterns
rps_per_user = 1000      # Higher limit for authenticated users
burst_size = 500         # Allow temporary traffic spikes
cleanup_interval_secs = 300  # Cleanup stale entries every 5 minutes
```text

**Via environment variables**:

```bash
# Enable/disable rate limiting
export FRAISEQL_RATE_LIMITING_ENABLED=true

# Adjust limits
export FRAISEQL_RATE_LIMIT_RPS_PER_IP=100
export FRAISEQL_RATE_LIMIT_RPS_PER_USER=1000
export FRAISEQL_RATE_LIMIT_BURST_SIZE=500
```text

### Response Headers

When rate limiting is active, responses include:

```text
X-RateLimit-Limit: 100        # Max requests allowed per second
X-RateLimit-Remaining: 42     # Requests remaining in current window
Retry-After: 60               # Seconds to wait before retrying (on 429)
```text

### 429 Too Many Requests Response

When rate limit is exceeded, the server responds with HTTP 429:

```json
{
  "errors": [
    {
      "message": "Rate limit exceeded. Please retry after 60 seconds."
    }
  ]
}
```text

### Best Practices

1. **Monitor Rate Limit Responses**: Track 429 responses to identify abuse patterns
2. **Adjust Per Endpoint**: If design audit endpoints show high load, consider increasing their limits
3. **Set Per Client**: For high-traffic clients, issue API tokens (users) instead of identifying by IP
4. **Test Before Production**: Run load tests with your expected traffic to tune limits
5. **Log Rate Limit Events**: Enable logging to track which IPs/users hit the limit

### Recommended Limits by Use Case

**Development**:

```toml
[rate_limiting]
enabled = false  # Or set very high limits
rps_per_ip = 10000
rps_per_user = 50000
```text

**Production (Standard)**:

```toml
[rate_limiting]
enabled = true
rps_per_ip = 100     # Reasonable limit for public endpoints
rps_per_user = 1000  # Higher for authenticated users
```text

**Production (High Traffic)**:

```toml
[rate_limiting]
enabled = true
rps_per_ip = 500     # Increase for high-traffic services
rps_per_user = 5000
burst_size = 2000
```text

### Disabling Rate Limiting

To disable (only recommended for internal/trusted environments):

```bash
export FRAISEQL_RATE_LIMITING_ENABLED=false
```text

Or in config:

```toml
[rate_limiting]
enabled = false
```text

### Separate from Auth Rate Limiting

Note: FraiseQL also includes separate rate limiting for OAuth/OIDC auth flows to prevent brute-force attacks. This is independent of general rate limiting and has its own configuration under security settings.

## Security Features Summary

Built-in protection for common GraphQL security issues:

| Issue | Protection |
|-------|-----------|
| Schema exposure in production | ✅ Introspection/Playground disabled by default |
| Admin operations unprotected | ✅ Bearer token required |
| Token confusion attacks | ✅ Audience validation enforced |
| CORS-based attacks | ✅ Explicit origin validation in production |
| DoS/abuse attacks | ✅ Rate limiting enabled by default |

## Support & Questions

- Check the troubleshooting section in this guide
- Review error messages - they include actionable guidance
- Consult [Architecture]../architecture/README.md) for security best practices
- Report issues with detailed reproduction steps

---

**Last Updated**: 2026-02-05
**Applies to**: v2.0.0-alpha.1
**Type**: Configuration Guide (Current)
