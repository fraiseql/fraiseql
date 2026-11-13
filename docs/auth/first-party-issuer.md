# The first-party issuer

Every sign-in FraiseQL serves itself (password, email OTP, MFA, anonymous, social and SAML)
mints a session the server signs. `[auth_issuer]` names the issuer those sessions carry and
the key that seals the signing keys at rest:

```toml
# server.toml
[auth_issuer]
url      = "https://api.example.com"   # the `iss` of every session this server mints
audience = "my-api"                     # their `aud`
# key_encryption_key_env = "FRAISEQL_AUTH_SIGNING_KEK"   # the default
```

```toml
# fraiseql.toml
[auth.session]
ttl_secs = 900

[auth.database]
url_env = "AUTH_DATABASE_URL"
```

`[auth.local]`, `[auth.social]` and `[saml]` refuse to boot without `[auth_issuer]`. It in turn
refuses to boot without:

- `[auth.session] ttl_secs`: how long an access token lives, and so how long a retired key
  must stay published for the tokens it signed to keep validating;
- `[auth.database]`: the keys are rows in `auth.signing_key`;
- the key-encryption key: 64 hex digits (32 random bytes) in the variable
  `key_encryption_key_env` names, for example `openssl rand -hex 32`.

`url` must be `https`, or `http` on a loopback host for local development, with no query
or fragment. `[auth_issuer]` and `[auth_hs256]` cannot be configured together.

## Keys

Sessions are ES256 JWTs with `typ: at+jwt`, naming the signing key's `kid`. The private halves
are stored sealed under the key-encryption key: a database dump alone does not sign. On its
first boot the server creates a key. Every hour it rotates the active key once it is 30 days
old, removes retired keys whose tokens have all expired, and re-reads the table, so a key
another replica rotated in is picked up. A token naming a `kid` the server has not seen makes
it re-read the table, at most once every 30 seconds.

A replica signs with the keys it last read, so after another one rotates it may sign with the
retired key until its own next read. A retired key therefore stays published for as long as a
session it signed can validate, plus two hours, so every session a replica signed with it
validates until it expires. A replica that has not read its keys successfully for two hours refuses to sign:
sign-ins fail, and the server logs why, until it reads them again.

## Rotating by hand

`fraiseql auth rotate-signing-key` retires the active key and activates a new one, whatever
their ages. It reads what the server reads, and takes no URL or secret on its command line:

```sh
FRAISEQL_CONFIG=server.toml fraiseql auth rotate-signing-key fraiseql.toml
```

- `[auth_issuer]` from the server configuration file (`--config`, else `FRAISEQL_CONFIG`);
- `[auth.database]` from the project, and the auth database URL from the variable its
  `url_env` names;
- the key-encryption key from the variable `key_encryption_key_env` names.

It prints both keys' `kid`s. The retired key stays published until the sessions it signed
have expired. Running servers sign with the new key after their next hourly re-read, or at
once after `POST /admin/v1/auth/refresh-jwks` on each. The command refuses to rotate under a
key-encryption key that does not open the active key, since no server could open a key it
sealed, and never creates the table or the first key: a server with `[auth_issuer]` does
when it boots. It needs the CLI built with `run-server`.

Rotating does not withdraw the retired key: sessions it signed stay valid until they expire.
An immediate withdrawal of a key is not offered.

## Publishing

The server publishes its keys, so a service beside it verifies its sessions as it would any
OIDC provider's:

- `GET /.well-known/openid-configuration`: the `issuer`, the `jwks_uri` and `ES256`;
- `GET /.well-known/jwks.json`: the active key and every retired key whose tokens may still be
  valid, public halves only. It answers `503` until the server has read its keys.

`POST /admin/v1/auth/refresh-jwks` (admin token) makes the server re-read its keys at once, as
it does an external provider's.

## Beside an external provider

With `[auth]` (an external OIDC provider) as well, `/graphql` and the other data routes accept
tokens from both issuers. The routes that act on the caller's own account (MFA enrollment,
confirmation and removal, and email verification) accept only the sessions this server minted.

The `__Host-access_token` cookie: the provider's sign-in sets it, so with `[auth]` every route
that accepts the cookie reads it, as without `[auth_issuer]`. The server puts one of its own
sessions in it only in cookie transport (`[auth.local] session_transport = "cookie"`). In
`json` transport, a session of its own arriving in the cookie is refused on every route, and
without `[auth]` the cookie is not read at all.

## Revoke-all

`POST /auth/revoke-all` ends every session this server minted for the user, so their refresh
tokens no longer refresh, and then refuses every access token issued to them before the call.

That refusal is kept for `[security.token_revocation] revoke_all_ttl_secs`, which has to cover
every session this server signs:

| `revoke_all_ttl_secs` | The refusal is kept for |
|---|---|
| unset | as long as a session this server signs is accepted, and at least 86400 seconds |
| set, at least that long | the value set |
| set, shorter | nothing: the server refuses to boot, and the error names the value to set |

A session is accepted a little longer than `[auth.session] ttl_secs`, or than seven days when
anonymous sign-in is on and `ttl_secs` is shorter. Tokens from another issuer are not counted:
when `[auth]` names one, set `revoke_all_ttl_secs` above the longest its tokens are valid.
