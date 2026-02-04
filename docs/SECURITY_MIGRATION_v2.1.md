# Security Migration Guide: v2.0 → v2.1

This guide helps you migrate from FraiseQL v2.0 to v2.1, which introduces significant security improvements.

## Overview of Security Changes

FraiseQL v2.1 adopts a **fail-secure** approach to security configuration, changing several defaults to prioritize production safety:

| Feature | v2.0 Default | v2.1 Default | Impact |
|---------|-------------|-------------|--------|
| Playground | Enabled | **Disabled** | Schema exposure protection |
| Introspection | Always enabled | **Disabled** | Schema exposure protection |
| Admin API | Always enabled | **Disabled** | Critical operation protection |
| JWT Audience | Optional | **Required** | Token confusion prevention |
| CORS Origins | Not validated | **Validated in production** | CSRF protection |

## Breaking Changes

### CRITICAL: JWT Audience Validation Now Required

**Impact**: If you're using OIDC authentication, you MUST configure the audience field.

**Why**: This prevents token confusion attacks where tokens from one service can be used in another.

**Before** (v2.0):
```toml
[auth]
issuer = "https://tenant.auth0.com/"
# audience was optional - NOT RECOMMENDED
```

**After** (v2.1):
```toml
[auth]
issuer = "https://tenant.auth0.com/"
audience = "https://api.example.com"  # NOW REQUIRED
```

**Migration Steps**:
1. Identify your API identifier (typically a URL like `https://api.example.com` or a simple ID like `my-api`)
2. Add `audience = "YOUR_API_IDENTIFIER"` to your `[auth]` section
3. Restart your server - it will fail with a clear error if missing

**Error you might see**:
```
OIDC audience is REQUIRED for security. Set 'audience' in auth config
to your API identifier. This prevents token confusion attacks...
```

### Admin Endpoints Now Opt-In

**Impact**: Admin endpoints (`/api/v1/admin/*`) are disabled by default.

**What are admin endpoints?**
- `/api/v1/admin/reload-schema` - Reload GraphQL schema
- `/api/v1/admin/cache/clear` - Clear caches
- `/api/v1/admin/config` - View server configuration

**Before** (v2.0):
```
Admin endpoints were always enabled and accessible.
⚠️ SECURITY RISK: No authentication required
```

**After** (v2.1):
```
Admin endpoints are disabled by default.
To enable: Set admin_api_enabled=true AND admin_token=<strong-token>
```

**Migration for v2.1**:

If you need admin endpoints:
```toml
[server]
admin_api_enabled = true
admin_token = "your-secure-token-32-characters-minimum!!"
```

Or via environment variables:
```bash
FRAISEQL_ADMIN_API_ENABLED=true
FRAISEQL_ADMIN_TOKEN="your-secure-token-32-characters-minimum!!"
```

Then authenticate requests:
```bash
curl -H "Authorization: Bearer your-secure-token-32-characters-minimum!!" \
     http://localhost:8000/api/v1/admin/config
```

### Introspection Now Opt-In

**Impact**: GraphQL introspection endpoint (`/introspection`) is disabled by default.

**Before** (v2.0):
```
Introspection was always enabled and public.
⚠️ SECURITY RISK: Schema exposed to everyone
```

**After** (v2.1):
```
Introspection is disabled by default.
Optionally enable with optional authentication.
```

**For Development** (enable without auth):
```toml
[server]
introspection_enabled = true
introspection_require_auth = false  # Schema public (dev only!)
```

**For Production** (enable with auth):
```toml
[server]
introspection_enabled = true
introspection_require_auth = true   # Requires OIDC auth
```

Environment variables:
```bash
FRAISEQL_INTROSPECTION_ENABLED=true
FRAISEQL_INTROSPECTION_REQUIRE_AUTH=true  # In production
```

**Note**: Schema export endpoints (`/api/v1/schema.graphql`, `/api/v1/schema.json`) follow the same configuration as introspection.

### Playground Disabled by Default

**Impact**: GraphQL playground (`/playground`) is disabled by default.

**Before** (v2.0):
```
Playground was enabled by default.
⚠️ SECURITY RISK: IDE exposing schema in production
```

**After** (v2.1):
```
Playground is disabled by default.
```

**To Enable** (development only):
```toml
[server]
playground_enabled = true
```

Or environment variable:
```bash
FRAISEQL_PLAYGROUND_ENABLED=true
```

**Migration**: If you're using playground, explicitly enable it in your dev configs.

### CORS Validation in Production

**Impact**: CORS origins are now validated in production mode.

**New Behavior**:
- **Development mode** (FRAISEQL_ENV=development): CORS origin validation relaxed
- **Production mode** (default): CORS must be explicitly configured

**Before** (v2.0):
```
cors_origins = []  # Allowed ALL origins (security risk!)
```

**After** (v2.1):
```toml
[server]
cors_enabled = true
cors_origins = ["https://app.example.com", "https://api.example.com"]
```

**Error in production** if you try to use empty origins:
```
cors_enabled is true but cors_origins is empty in production mode.
This allows requests from ANY origin, which is a security risk.
Set your allowed domains explicitly.
```

## Non-Breaking Changes

### Design API Optional Authentication

Design audit endpoints (`/api/v1/design/*`) now support optional authentication.

**Default** (v2.1): Public (no auth required)

**To require auth**:
```toml
[server]
design_api_require_auth = true  # Requires OIDC auth
```

This doesn't affect existing deployments - the endpoints remain public unless you explicitly enable auth.

## Migration Checklist

### Phase 1: Pre-Migration (Before Upgrading to v2.1)

- [ ] Review your current FraiseQL v2.0 configuration
- [ ] Document which endpoints you use (admin, introspection, playground)
- [ ] Identify your API identifier for the audience field
- [ ] Test v2.1 in a staging environment

### Phase 2: Update Configuration

- [ ] Add `audience` field to your `[auth]` section (CRITICAL)
- [ ] If you use admin endpoints: add `admin_api_enabled` and `admin_token`
- [ ] If you use introspection: add `introspection_enabled` and `introspection_require_auth`
- [ ] If you use playground: add `playground_enabled = true`
- [ ] Set explicit `cors_origins` if using CORS

### Phase 3: Update Deployment

- [ ] Set `FRAISEQL_ENV=production` (or don't set it - production is default)
- [ ] For dev environments, optionally set `FRAISEQL_ENV=development`
- [ ] Ensure strong tokens for `admin_token` (32+ characters)
- [ ] Update any automation that accesses admin endpoints with new auth

### Phase 4: Verify

- [ ] Test GraphQL queries (no auth changes required)
- [ ] Test authentication flow (audience validation now in place)
- [ ] Verify admin endpoints return 404 if not enabled
- [ ] Verify introspection returns 404 if not enabled
- [ ] Check logs for security warnings

## Configuration Examples

### Development Environment

```toml
# .toml file or environment variables for local development
[server]
playground_enabled = true
introspection_enabled = true
introspection_require_auth = false
cors_enabled = true
cors_origins = ["http://localhost:3000", "http://localhost:5173"]

[auth]
issuer = "https://your-tenant.auth0.com/"
audience = "localhost"  # Use "localhost" for local dev
```

Environment variables for dev:
```bash
export FRAISEQL_ENV=development
export FRAISEQL_PLAYGROUND_ENABLED=true
export FRAISEQL_INTROSPECTION_ENABLED=true
```

### Production Environment

```toml
# production.toml
[server]
playground_enabled = false
introspection_enabled = true          # Optional
introspection_require_auth = true     # If enabled
admin_api_enabled = true              # If you need it
admin_token = "your-secure-32+-char-token"
cors_enabled = true
cors_origins = ["https://app.example.com", "https://another-app.example.com"]

[auth]
issuer = "https://your-tenant.auth0.com/"
audience = "https://api.example.com"  # Your API identifier
```

Environment variables for production:
```bash
export FRAISEQL_ENV=production  # Or don't set (production is default)
export FRAISEQL_ADMIN_TOKEN="your-secure-32+-char-token"
export FRAISEQL_ADMIN_API_ENABLED=true
```

## Troubleshooting

### "OIDC audience is REQUIRED for security"

**Solution**: Add the `audience` field to your `[auth]` section:
```toml
[auth]
audience = "your-api-identifier"
```

### "playground_enabled is true in production mode"

**Solution**: Either disable playground or set `FRAISEQL_ENV=development`:
```bash
# Option 1: Disable
playground_enabled = false

# Option 2: Development mode
export FRAISEQL_ENV=development
```

### "cors_origins is empty in production mode"

**Solution**: Explicitly configure CORS origins:
```toml
[server]
cors_origins = ["https://app.example.com"]
```

### Admin endpoints return 404

**Solution**: Enable admin API:
```toml
[server]
admin_api_enabled = true
admin_token = "your-secure-token-32-char-minimum!!"
```

### Introspection endpoint returns 404

**Solution**: Enable introspection:
```toml
[server]
introspection_enabled = true
introspection_require_auth = false  # For dev; true for production
```

## Rate Limiting

FraiseQL v2.1 includes built-in rate limiting to protect GraphQL endpoints from abuse and denial-of-service attacks.

### Overview

Rate limiting is **enabled by default** with sensible per-IP and per-user limits using a token bucket algorithm:

| Setting | Default | Description |
|---------|---------|-------------|
| `enabled` | `true` | Rate limiting enabled for security |
| `rps_per_ip` | 100 | Requests per second per IP address |
| `rps_per_user` | 1000 | Requests per second per authenticated user |
| `burst_size` | 500 | Maximum tokens to accumulate |

### Configuration

**In TOML file** (`fraiseql.toml`):
```toml
[rate_limiting]
enabled = true
rps_per_ip = 100         # Adjust per your traffic patterns
rps_per_user = 1000      # Higher limit for authenticated users
burst_size = 500         # Allow temporary traffic spikes
cleanup_interval_secs = 300  # Cleanup stale entries every 5 minutes
```

**Via environment variables**:
```bash
# Enable/disable rate limiting
export FRAISEQL_RATE_LIMITING_ENABLED=true

# Adjust limits
export FRAISEQL_RATE_LIMIT_RPS_PER_IP=100
export FRAISEQL_RATE_LIMIT_RPS_PER_USER=1000
export FRAISEQL_RATE_LIMIT_BURST_SIZE=500
```

### Response Headers

When rate limiting is active, responses include:

```
X-RateLimit-Limit: 100        # Max requests allowed per second
X-RateLimit-Remaining: 42     # Requests remaining in current window
Retry-After: 60               # Seconds to wait before retrying (on 429)
```

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
```

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
```

**Production (Standard)**:
```toml
[rate_limiting]
enabled = true
rps_per_ip = 100     # Reasonable limit for public endpoints
rps_per_user = 1000  # Higher for authenticated users
```

**Production (High Traffic)**:
```toml
[rate_limiting]
enabled = true
rps_per_ip = 500     # Increase for high-traffic services
rps_per_user = 5000
burst_size = 2000
```

### Disabling Rate Limiting

To disable (only recommended for internal/trusted environments):

```bash
export FRAISEQL_RATE_LIMITING_ENABLED=false
```

Or in config:
```toml
[rate_limiting]
enabled = false
```

### Separate from Auth Rate Limiting

Note: FraiseQL also includes separate rate limiting for OAuth/OIDC auth flows to prevent brute-force attacks. This is independent of general rate limiting and has its own configuration under security settings.

## Getting Help

- Check the error messages - they now include actionable guidance
- Review the [ARCHITECTURE_PRINCIPLES.md](./ARCHITECTURE_PRINCIPLES.md) for security best practices
- Consult the [Security Configuration](./ARCHITECTURE_PRINCIPLES.md#security-configuration-best-practices) section

## Security Improvements Summary

| Issue | v2.0 | v2.1 | Fix |
|-------|------|------|-----|
| Schema exposure in production | ❌ | ✅ | Introspection/Playground disabled by default |
| Admin operations unprotected | ❌ | ✅ | Bearer token required |
| Token confusion attacks | ❌ | ✅ | Audience validation required |
| CORS allows all origins | ❌ | ✅ | Explicit origin validation in production |
| Design API public | ❌ | ✅ | Optional auth support |
| DoS/abuse attacks on GraphQL | ❌ | ✅ | Rate limiting enabled by default |

## Version Compatibility

- **Upgrading FROM**: v2.0.x → v2.1.x
- **Downgrading TO**: v2.1.x → v2.0.x requires removing configuration options (audience, admin_token, etc.)

## Support & Questions

If you encounter issues during migration:
1. Check this guide's troubleshooting section
2. Review error messages - they're now more descriptive
3. Check the [ARCHITECTURE_PRINCIPLES.md](./ARCHITECTURE_PRINCIPLES.md) documentation
4. Report issues with detailed reproduction steps

---

**Last Updated**: 2026-02-03
**Version**: v2.1.0
