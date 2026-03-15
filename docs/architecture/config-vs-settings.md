# Config vs Settings

FraiseQL uses 150+ `*Config` types and 6 `*Settings` types. The naming is intentional and encodes a semantic distinction.

## Definitions

| Suffix | Source | Lifecycle | Example |
|--------|--------|-----------|---------|
| `*Config` | `fraiseql.toml`, env vars, CLI flags | Mutable during development; baked at compile time | `ServerConfig`, `OidcConfig`, `RateLimitConfig` |
| `*Settings` | `schema.compiled.json` | Immutable after server start | `SecuritySettings`, `RateLimitingSettings` |

## Flow

```
fraiseql.toml                    fraiseql-cli compile           fraiseql-server start
  [fraiseql.security]     --->     SecuritySettings      --->    loaded from compiled
  rate_limiting = {...}            (validated, embedded          schema; immutable for
  audit_logging = {...}             in compiled schema)          server lifetime
```

### `*Config` (Developer-Facing)

Config types represent **developer intent** expressed in TOML or environment variables. They may contain raw paths, URLs, or feature toggles that need validation before use.

- Defined in: `fraiseql-server/src/server_config/`, `fraiseql-core/src/config/`
- Loaded from: `fraiseql.toml`, environment variables, CLI flags
- Validated: at load time (parse errors) and at startup (`validate()`)
- May change between deployments

### `*Settings` (Runtime-Immutable)

Settings types represent **validated, compiled configuration** embedded in `schema.compiled.json` by the CLI compiler. They are loaded once at server startup and never change during the server's lifetime.

- Defined in: `fraiseql-cli/src/config/`, `fraiseql-auth/src/security_config.rs`
- Loaded from: `schema.compiled.json` (the `"security"` key)
- Validated: at compile time by `fraiseql-cli compile`
- Immutable after `Server::new()`

## The 6 Settings Types

| Type | Crate | Purpose |
|------|-------|---------|
| `FraiseQLSettings` | `fraiseql-cli` | Top-level compiled settings container |
| `SecuritySettings` | `fraiseql-cli` | Security subsystem aggregate (contains the 4 below) |
| `AuditLoggingSettings` | `fraiseql-auth` | Audit log level, enabled flag |
| `ErrorSanitizationSettings` | `fraiseql-auth` | Error message stripping for production |
| `RateLimitingSettings` | `fraiseql-auth` | Auth endpoint rate limits (compiled from TOML) |
| `StateEncryptionSettings` | `fraiseql-auth` | PKCE state encryption key config |

## When to Use Which

- **Adding a new server knob** (bind address, pool size, feature toggle) -> `*Config` in `server_config/`
- **Adding a new security policy** that gets baked into the compiled schema -> `*Settings` in `fraiseql-auth` or `fraiseql-cli`
- **Adding runtime-only middleware config** (rate limiting per IP, CORS origins) -> `*Config` in `server_config/`

## Environment Variable Overrides

Settings compiled into `schema.compiled.json` can be overridden at runtime via environment variables. The server checks env vars after loading compiled settings:

```
Compiled value (schema.compiled.json)
    |
    v  env var override (e.g. FRAISEQL_RATE_LIMIT_MAX_REQUESTS)
Final runtime value
```

This allows the same compiled schema to be deployed across environments (staging/production) with different operational parameters.
