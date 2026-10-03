# Trusting more than one token issuer

A deployment can accept tokens from its OIDC identity provider **and** from other
issuers side by side. A typical case is a first-party token-exchange service that mints
short-lived [RFC 8693](https://www.rfc-editor.org/rfc/rfc8693) delegation tokens (an `act`
claim, one tenant, narrow scopes) for the same API, signed with its own key and published
at its own JWKS URL.

`[auth]` describes the primary issuer. Each further issuer is a
`[[auth.additional_issuers]]` table with its own keys, audience and algorithms.

## How a token is routed

1. The server reads the token's `iss` claim **before** checking its signature, and only
   to choose which issuer's keys apply.
2. It verifies the token with that issuer's keys, audience, algorithms and clock skew
   **alone**. `iss` is then checked again by the validation itself.
3. A token whose `iss` names no configured issuer, or that carries no `iss`, is refused
   without contacting any key endpoint. It is never tried against every key set: that
   would make `iss` meaningless, and a key published by one issuer could vouch for a
   token claiming to be another.

The issuer that vouched for a request is recorded as the principal's issuer
(`SecurityContext::issuer`), and `$iss` is available to identity-enrichment queries.

## Configuration (`server.toml`)

```toml
# server.toml
[auth]
issuer   = "https://idp.example.com"
audience = "https://api.example.com"

[[auth.additional_issuers]]
issuer             = "https://exchange.example.com"
audience           = "https://api.example.com"
jwks_uri           = "https://exchange.example.com/.well-known/jwks.json"
allowed_algorithms = ["ES256"]
require_jti        = true
```

Each `[[auth.additional_issuers]]` table accepts:

| Field | Default | Meaning |
|-------|---------|---------|
| `issuer` | (required) | Matched exactly against a token's `iss` |
| `audience`, `additional_audiences` | — | Accepted `aud` values; at least one is required |
| `jwks_uri` | discovered from `issuer` | The issuer's key endpoint |
| `allowed_algorithms` | `["RS256"]` | Signing algorithms accepted for this issuer |
| `clock_skew_secs` | `60` | Expiry tolerance (capped, as for `[auth]`) |
| `scope_claim` | `"scope"` | Claim scopes are read from |
| `require_jti` | `false` | Refuse tokens without a `jti` |

`required`, `jwks_cache_ttl_secs` and `[auth.me]` are shared and set on `[auth]` only.

## Rules the server enforces at startup

- `[auth] issuer` must be set whenever any additional issuer is: every trusted issuer has
  to be named, because tokens are routed by name. Issuer-less mode
  ([issuer-less-jwt.md](issuer-less-jwt.md)) is single-issuer only.
- Each issuer appears once.
- Each additional issuer is held to the same rules as `[auth]`: HTTPS (loopback `http`
  allowed for development), an audience, at least one algorithm. The error names the
  issuer at fault.

## Delegation tokens

Tokens from a token-exchange service usually carry `act`. The acting party's class is read
from `act.actor_type`; see [Delegated requests](../operations/actor-policies.md#delegated-requests).
Tenant and claim resolution are the same for every issuer: the request's tenant is the
schema's `tenant_claim`.
