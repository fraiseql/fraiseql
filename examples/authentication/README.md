# Authentication

The server accepts a bearer token, validates it, and turns the claims into the
identity that row-level security and field-level authorization see. This example
does that half in isolation: no server, no database, no network — just
`fraiseql-auth`'s validator against tokens the process signs itself.

## Run it

```bash
./run.sh
```

No database and no compiled schema.

## What to read

The happy path is four lines. The interesting part is the five rejections at the
end — a validator that accepts any of them is a validator that lets one service's
tokens be replayed against another, or lets an expired session keep working:

| token | must fail because |
|---|---|
| expired an hour ago | `exp` is in the past |
| addressed to another service | `aud` is not this API |
| signed by someone else | the signature does not verify |
| minted by a different issuer | `iss` is not the configured issuer |
| not a JWT at all | it does not parse |

The example exits non-zero if any of them is accepted, so it is a test as well as
a demonstration.

Note `.with_audiences(&[AUDIENCE])`. Pinning only the issuer accepts every token
that issuer ever minted, including tokens addressed to a different service —
`JwtValidator::new` requires `aud` to be *present*, but only `with_audiences`
requires it to be *yours*.

An HS256 shared secret is used here because it fits in one file. Production uses
RS256 against the provider's JWKS, where the server holds only a public key; the
validation posture is the same.

This crate depends on `jsonwebtoken` for one import, `Algorithm`, because
`fraiseql-auth` does not re-export it —
[#1198](https://github.com/fraiseql/fraiseql/issues/1198).
