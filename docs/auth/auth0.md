# Auth0 with FraiseQL

Auth0 is an OIDC-compliant provider that works with FraiseQL's existing PKCE
authentication flow **without any code changes**. This guide covers configuration.

## Prerequisites

1. An Auth0 tenant (e.g. `your-tenant.auth0.com`)
2. A **Regular Web Application** created in the Auth0 dashboard
3. FraiseQL compiled with `[security.pkce]` and `[auth]` configured

## fraiseql.toml

```toml
[auth]
discovery_url       = "https://your-tenant.auth0.com/"
client_id           = "your-auth0-client-id"
client_secret_env   = "AUTH0_CLIENT_SECRET"
server_redirect_uri = "https://api.yourdomain.com/auth/callback"

[security.pkce]
enabled = true
code_challenge_method = "S256"
state_ttl_secs = 600

[security.state_encryption]
enabled = true
algorithm = "chacha20-poly1305"
key_env = "STATE_ENCRYPTION_KEY"
```

## Environment variables

```bash
# Auth0 client secret (from Auth0 dashboard → Applications → Settings)
AUTH0_CLIENT_SECRET=your-client-secret

# 32-byte hex key for PKCE state encryption
# Generate: openssl rand -hex 32
STATE_ENCRYPTION_KEY=<64-hex-chars>
```

## Auth0 dashboard configuration

In your Auth0 application settings:

| Setting | Value |
|---------|-------|
| **Application Type** | Regular Web Application |
| **Allowed Callback URLs** | `https://api.yourdomain.com/auth/callback` |
| **Allowed Logout URLs** | `https://yourdomain.com` |
| **Token Endpoint Authentication Method** | POST |

## Custom claims mapping

If you use Auth0 Actions or Rules to add custom claims (e.g. roles, permissions),
map them in the JWT validation configuration:

```toml
# Example: Auth0 namespaced custom claims
# Auth0 requires custom claims to use a URL namespace to avoid collision
# with standard OIDC claims.
```

Auth0 standard claims (`sub`, `email`, `name`) are mapped automatically.
Custom claims added via Auth0 Actions appear in the JWT payload and are
available in `SecurityContext.attributes`.

### Example Auth0 Action (Login flow)

```javascript
exports.onExecutePostLogin = async (event, api) => {
  const namespace = 'https://yourdomain.com';
  api.idToken.setCustomClaim(`${namespace}/roles`, event.authorization?.roles || []);
  api.accessToken.setCustomClaim(`${namespace}/roles`, event.authorization?.roles || []);
};
```

These claims are accessible in FraiseQL RLS policies and `inject` parameters
via the JWT `attributes` map.

## API key authentication (optional)

For service-to-service communication that bypasses Auth0, configure API keys:

```toml
[security.api_keys]
enabled = true
header = "X-API-Key"
hash_algorithm = "sha256"
storage = "env"

[[security.api_keys.static]]
key_hash = "sha256:<hex-encoded-sha256-of-your-key>"
scopes = ["read:*", "write:*"]
name = "backend-service"
```

Generate a key hash:
```bash
echo -n "your-api-key" | sha256sum | cut -d' ' -f1
```

## Testing the flow

```bash
# 1. Start the auth flow
curl -v "https://api.yourdomain.com/auth/start?redirect_uri=https://app.yourdomain.com/callback"
# → 302 redirect to Auth0 login page

# 2. After login, Auth0 redirects to /auth/callback with code + state
# → FraiseQL exchanges code for tokens and returns them as JSON

# 3. Use the access token for GraphQL queries
curl -H "Authorization: Bearer <access-token>" \
     -H "Content-Type: application/json" \
     -d '{"query": "{ users { id name } }"}' \
     https://api.yourdomain.com/graphql
```

## Multi-tenant Auth0

For multi-tenant setups using Auth0 Organizations:

1. Enable Organizations in Auth0
2. Auth0 adds the `org_id` claim to tokens
3. Map `org_id` to `tenant_id` using FraiseQL's `inject` feature:

```python
@fraiseql.query(inject={"tenant_id": "jwt:org_id"})
def get_users(tenant_id: str) -> list[User]:
    ...
```

## Troubleshooting

| Issue | Solution |
|-------|----------|
| `400` on `/auth/callback` | Check Allowed Callback URLs in Auth0 matches `server_redirect_uri` |
| `502` on token exchange | Verify `AUTH0_CLIENT_SECRET` is correct |
| Missing claims in `SecurityContext` | Check Auth0 Action is attached to the Login flow |
| `state` validation fails | Ensure `STATE_ENCRYPTION_KEY` is the same across replicas (or use Redis PKCE store) |
