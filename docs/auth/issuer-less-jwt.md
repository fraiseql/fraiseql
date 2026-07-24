# JWT validation for IdPs that omit `iss` (issuer-less mode)

Some identity providers issue signed access tokens **without an `iss` (issuer)
claim** — for example self-hosted [Hanko](https://www.hanko.io/) 2.x, whose
access tokens carry `sub`, `aud`, `exp`, `iat` (plus `email`, `session_id`) but
no `iss`. FraiseQL's OIDC `[auth]` supports these providers: the `issuer` field
is **optional**, symmetric with `audience`.

## How it works

`[auth]` (the server's `OidcConfig`) treats `issuer` as optional:

| `issuer` | Behaviour |
|----------|-----------|
| **set** | `iss` **must be present and equal** the configured issuer. When `jwks_uri` is not pinned, `issuer` is also used for OIDC discovery. |
| **unset** | `iss` is **not validated**. Discovery is impossible without an issuer, so `jwks_uri` **must be pinned**. |

In both modes the token is still gated by:

- **signature** — verified against the keys fetched from the configured
  `jwks_uri` (or the discovered JWKS endpoint), and
- **audience** — `audience` is mandatory and checked against the token's `aud`.

## Configuration (`server.toml`)

Pin the IdP's JWKS endpoint and set the audience to your relying-party
identifier; omit `issuer`:

```toml
[auth]
# issuer intentionally omitted — Hanko access tokens carry no `iss` claim.
jwks_uri = "https://hanko.example.com/.well-known/jwks.json"
audience = "<your-relying-party-id>"   # matched against the token's `aud`
allowed_algorithms = ["RS256"]          # or your IdP's signing algorithm
```

If you instead set `issuer`, `iss` becomes required and validated (and
`jwks_uri` may be omitted to use OIDC discovery):

```toml
[auth]
issuer   = "https://accounts.google.com"
audience = "<your-client-id>"
```

## Why dropping `iss` is safe here

With a **pinned `jwks_uri`**, the signing keys come from the endpoint you
configured — not from a URL discovered inside the token's own `iss`. Forgery is
prevented by the signature (only your IdP holds the private key), and
cross-service token confusion is prevented by the mandatory `audience` check.
`iss` validation is defence-in-depth against *key confusion* when multiple
issuers share one JWKS — which does not apply to a single-IdP, direct-`jwks_uri`
deployment.

Because `issuer` is unset, FraiseQL requires `jwks_uri` to be pinned: without an
issuer there is no way to *discover* the JWKS endpoint, so it must be provided
explicitly. Starting the server with neither `issuer` nor `jwks_uri` is a
configuration error.

## The CLI accepts the same block

Runtime JWT validation is configured entirely by the server reading `[auth]`
from its own config file into `OidcConfig` — auth is **not** carried in
`schema.compiled.json`. When you keep one config file for both `fraiseql compile`
and the server, the CLI's `[auth]` schema (`OidcClientConfig`) validates the
*same* block structurally, so it accepts issuer-less mode too: `jwks_uri` is a
recognised field and `issuer` is optional when `jwks_uri` is pinned. A unified
config like the one above therefore passes `fraiseql compile`/lint **and** runs
on the server.

> Note: advanced `OidcConfig` fields (e.g. `allowed_algorithms`,
> `additional_audiences`, `jwks_cache_ttl_secs`) are not yet mirrored in the
> CLI's `[auth]` schema, so a config that sets them will be rejected by
> `fraiseql compile` even though the server accepts them. Omit them (the RS256
> default suits Hanko) or configure `[auth]` only in a server-only config file.
