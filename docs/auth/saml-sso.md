# SAML / Enterprise SSO

FraiseQL does not implement a native SAML Service Provider. Instead, use an
**identity proxy** that accepts SAML assertions from your IdP and issues
JWKS-signed JWTs to FraiseQL. This is the industry-standard pattern for
microservices and requires zero code changes.

## Architecture

```
Browser → SAML IdP (Okta, Azure AD, ADFS)
              ↓
        Identity Proxy (Keycloak / Dex / Auth0)
              ↓
        JWT (signed with JWKS)
              ↓
        FraiseQL (validates JWT via OIDC discovery)
```

The identity proxy:
1. Handles SAML assertion validation and XML signature verification
2. Maps SAML attributes to JWT claims
3. Exposes an OIDC-compatible discovery endpoint (`.well-known/openid-configuration`)

FraiseQL points its `[auth]` configuration at the proxy's OIDC endpoint.

## Recommended proxies

| Proxy | Hosting | Best for |
|-------|---------|----------|
| **Keycloak** | Self-hosted (Docker, K8s) | Full control, multi-protocol support |
| **Dex** | Self-hosted (lightweight) | Kubernetes-native, minimal footprint |
| **Auth0** | Managed SaaS | Quick setup, enterprise SAML federation built-in |
| **Okta** | Managed SaaS | Enterprise SSO with SAML + OIDC |

## Example: Keycloak as SAML-to-OIDC bridge

### 1. Deploy Keycloak

```bash
docker run -d --name keycloak \
  -p 8180:8080 \
  -e KEYCLOAK_ADMIN=admin \
  -e KEYCLOAK_ADMIN_PASSWORD=admin \
  quay.io/keycloak/keycloak:latest start-dev
```

### 2. Configure SAML Identity Provider in Keycloak

1. Create a realm (e.g. `fraiseql`)
2. Go to Identity Providers → Add SAML v2.0
3. Enter your corporate IdP metadata URL
4. Map SAML attributes to Keycloak user attributes

### 3. Create an OIDC client for FraiseQL

1. Go to Clients → Create
2. Client ID: `fraiseql`
3. Client Protocol: `openid-connect`
4. Access Type: `confidential`
5. Valid Redirect URIs: `https://api.yourdomain.com/auth/callback`

### 4. Configure FraiseQL

```toml
[auth]
discovery_url       = "https://keycloak.yourdomain.com/realms/fraiseql"
client_id           = "fraiseql"
client_secret_env   = "KEYCLOAK_CLIENT_SECRET"
server_redirect_uri = "https://api.yourdomain.com/auth/callback"

[security.pkce]
enabled = true
```

## Example: Dex as SAML-to-OIDC bridge

Dex is lightweight and Kubernetes-native:

```yaml
# dex-config.yaml
issuer: https://dex.yourdomain.com
connectors:
  - type: saml
    id: corporate-idp
    name: Corporate SSO
    config:
      ssoURL: https://idp.corp.example.com/saml/sso
      ca: /etc/dex/saml-ca.pem
      redirectURI: https://dex.yourdomain.com/callback
      usernameAttr: name
      emailAttr: email
      groupsAttr: groups

staticClients:
  - id: fraiseql
    secret: ${DEX_CLIENT_SECRET}
    name: FraiseQL
    redirectURIs:
      - https://api.yourdomain.com/auth/callback
```

Then configure FraiseQL to point at Dex:

```toml
[auth]
discovery_url       = "https://dex.yourdomain.com"
client_id           = "fraiseql"
client_secret_env   = "DEX_CLIENT_SECRET"
server_redirect_uri = "https://api.yourdomain.com/auth/callback"
```

## Attribute mapping

SAML attributes are mapped to JWT claims by the identity proxy. Common mappings:

| SAML Attribute | JWT Claim | FraiseQL Usage |
|---------------|-----------|----------------|
| `NameID` | `sub` | `SecurityContext.user_id` |
| `groups` / `memberOf` | `roles` or custom claim | `SecurityContext.roles` |
| `email` | `email` | `SecurityContext.attributes["email"]` |
| `department` | custom claim | Available via `inject` for RLS |

Configure the mapping in your identity proxy, not in FraiseQL.

## Security considerations

- The identity proxy **must** validate SAML XML signatures
- Use HTTPS between all components
- Rotate the OIDC client secret regularly
- Consider enabling FraiseQL's token revocation (`[security.token_revocation]`)
  for instant session termination when users are deprovisioned in the IdP
