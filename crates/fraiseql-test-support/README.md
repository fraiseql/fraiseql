# fraiseql-test-support

The single source of truth for how a FraiseQL integration test obtains its backing
service (database, cache, message bus, secret store).

One policy, applied per service:

1. If the service's env URL is set (e.g. `DATABASE_URL`), use it.
2. Otherwise, with the `local-testcontainers` feature, spawn an ephemeral container
   on the local Docker daemon (Ryuk reaper on) and use that.
3. Otherwise return `None` — the caller skips.

CI never enables `local-testcontainers`: Dagger provisions the services and injects
the URLs, identically to a local `dagger call test-integration`. The testcontainers
code path is therefore not compiled into CI binaries, so the container-leak class is
impossible there by construction.

```rust
#[tokio::test]
async fn needs_postgres() {
    let Some(pg) = fraiseql_test_support::postgres().await else {
        eprintln!("SKIP: no postgres (set DATABASE_URL or enable local-testcontainers)");
        return;
    };
    let _url = pg.url();
}
```

This crate is intentionally dependency-light (no workspace crates) so every crate can
dev-depend on it without a cycle.

## Test issuer (`test-issuer` feature)

`issuer::TestIssuer` is an in-process ES256 OIDC issuer. It generates its P-256
keys at runtime, serves a JWKS and a discovery document on a loopback port, and
mints tokens with chosen claims. Point the real validator at `jwks_uri()`, or at
`issuer()` for discovery, so the production path runs.

The defaults are strict. Every token carries `iss`, `aud`, `sub`, `iat`, `nbf`,
`exp` and a unique `jti`, and an `at+jwt` header naming its `kid`. A test that
needs a claim missing or changed calls `Claims::without` or `Claims::set`.

The issuer also provides:

- `rotate()`, which adds a key and makes it the signing key;
- `retire(kid)`, which stops publishing a key;
- `foreign_token(claims)`, which names the active `kid` but is signed by a key
  the issuer never publishes.
