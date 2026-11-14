# Authentication

The server accepts a bearer token, validates it, and turns the claims into the
identity that row-level security and field-level authorization see. This example
does that half in isolation: no server, no database — just the validator the server
uses, `OidcValidator`, against tokens from an ES256 issuer the process runs on
loopback.

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

The validator is configured with both `issuer` and `audience`. Pinning only the
issuer accepts every token that issuer ever minted, including tokens addressed to a
different service.

The issuer generates its key at startup and publishes the public half as a JWKS,
which is what an OIDC provider does, and what the server's own issuer
(`[auth_issuer]`) does for the sessions it mints. The validator holds only public
keys.
