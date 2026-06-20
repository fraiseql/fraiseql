# SAML / Enterprise SSO

FraiseQL supports SAML 2.0 SSO in two ways:

1. **Native SAML Service Provider** (opt-in `auth-saml` build feature) — FraiseQL
   itself terminates SP-initiated login and the Assertion Consumer Service. Use this
   when you want SAML handled in-process without an extra hop. See
   [Native SAML Service Provider](#native-saml-service-provider-auth-saml) below.
2. **Identity proxy** (zero code, default build) — an external proxy (Keycloak, Dex,
   Auth0) accepts SAML assertions and issues JWKS-signed JWTs to FraiseQL. This is the
   industry-standard pattern for microservices and needs no build features. See
   [Identity proxy](#identity-proxy).

## Native SAML Service Provider (`auth-saml`)

> **Status (#381):** this ships SP-initiated SSO + ACS for a single IdP. Multi-IdP
> discovery, per-tenant SAML config storage, and SCIM provisioning remain on the #381
> umbrella and are not yet implemented.

Enabled by the non-default Cargo feature `auth-saml`, which pulls in
[`samael`](https://crates.io/crates/samael) and its `xmlsec` backend. The default build
stays free of the XML/crypto C stack.

### Build requirements (C libraries)

Signature verification uses the reference **xmlsec1** library (libxml2 + openssl). These
system packages are required to build with `--features auth-saml`, and the requirement is
identical for local development and CI:

| Environment | Install |
|-------------|---------|
| Arch (local) | `sudo pacman -S xmlsec` |
| Debian/Ubuntu (CI) | `apt-get install -y libxml2-dev libxmlsec1-dev libssl-dev pkg-config` |

The dedicated Dagger `integration: saml` suite installs the Debian packages; keep the two
columns in sync if either changes.

### Endpoints

| Route | Purpose |
|-------|---------|
| `GET /auth/saml/login?idp=<name>` | SP-initiated SSO — builds a signed-relay-state `AuthnRequest` and 302-redirects to the IdP. |
| `POST /auth/saml/acs` | Assertion Consumer Service — verifies the `SAMLResponse` and creates a session. |

### Security model

Every assertion is verified fail-closed before a session is issued:

- **Signature** — verified against the configured IdP certificate, restricted to a
  SHA-256+ signature/digest **algorithm allow-list** (SHA-1 is rejected).
- **XML Signature Wrapping** — the document is *reduced to the bytes covered by the
  verified signature* and only that is parsed; forged/duplicate/nested assertions are
  rejected.
- **XXE / entity expansion** — any `DOCTYPE`/entity declaration is rejected before parsing.
- **Audience / Recipient / Destination** — must match the SP `entity_id` / ACS URL.
- **Conditions** — `NotBefore` / `NotOnOrAfter` enforced with bounded clock skew.
- **Request binding** — the assertion's `InResponseTo` must match an in-flight
  `AuthnRequest` ID bound to the single-use `RelayState`.
- **Replay** — each assertion `ID` is single-use for the lifetime of its validity window.

### Account linking and the email-trust policy

A verified assertion maps to a local user via the same account store as the OIDC/social
path, keyed on `("saml:<idp>", NameID)`. By default a SAML identity **never** auto-merges
with another provider's account on a shared email.

Per-IdP opt-in `trust_asserted_email` allows a verified SAML email to link across
providers, but only when the merge is provably bounded to a single tenant. If an IdP is
configured with a `tenant_id` (multi-tenant intent), the policy **fails closed** — because
the single-tenant account store cannot scope a global email merge to one tenant, honoring
it could merge a verified assertion into another tenant's account (the nOAuth class). SAML
trust is never added to the global trusted-provider set; it is computed per IdP.

### Minimal configuration

```rust
use fraiseql_auth::{SamlAuthState, SamlIdpConfig};

let idp = SamlIdpConfig::builder(
        "okta",                                  // logical IdP name → "saml:okta"
        "https://api.example.com/saml/metadata", // this SP's entity_id
        "https://api.example.com/auth/saml/acs", // ACS URL
    )
    .idp_metadata_xml(&okta_metadata_xml)?       // IdP signing cert + SSO endpoint
    // .trust_asserted_email(true)               // opt-in cross-provider email linking
    .build()?;

let saml_state = SamlAuthState::new(state_store, session_store)
    .with_idp(idp)
    .with_user_store(account_store);
// mount fraiseql_auth::saml::saml_routes(saml_state)
```

## Identity proxy

The proxy pattern needs no build features and is the right choice when you already run an
IdP broker or want SAML handled out-of-process.

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
