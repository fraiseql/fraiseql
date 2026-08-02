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

Most compiled settings are **immutable at runtime** — that is the point of compiling them.
There is no generic `FRAISEQL_*` prefix engine that maps every setting onto a variable; the
server reads each override explicitly, and the complete list of compiled settings with a
runtime override is:

| Variable | Overrides |
|----------|-----------|
| `FRAISEQL_MAX_PAGE_SIZE` | The compiled page-size ceiling (#421) |
| `FRAISEQL_CHANGELOG_ENABLED` | The compiled change-log toggle (composes AND with the compiled value) |
| `FRAISEQL_FUNCTIONS_DLQ_STORE` | The compiled `[functions] dlq_store` backend |
| `FRAISEQL_FUNCTIONS_DLQ_MAX_SIZE` | The compiled functions DLQ size cap |
| `FRAISEQL_FUNCTIONS_RETRY_MAX_ATTEMPTS` / `_INITIAL_DELAY_MS` / `_MAX_DELAY_MS` | The compiled functions retry policy |
| `FRAISEQL_SOURCES_ENABLED` | Whether the source scheduler runs at all |
| `FRAISEQL_SOURCES_ALLOWED_DOMAINS` / `FRAISEQL_SOURCES_ALLOWED_ENV_VARS` | The sources egress/env allowlists |

Rate limiting is the special case, and its precedence is implemented and pinned by tests
(`server::tests::rate_limit_boot_guard_tests`). Resolution order, **lowest to highest**,
each layer applied over the last:

```
server [rate_limiting] table (fraiseql.toml passed via --config)
    < compiled [security.rate_limiting] (schema.compiled.json)
    < CLI flags / env vars (FRAISEQL_RATE_LIMITING_ENABLED,
      FRAISEQL_RATE_LIMIT_RPS_PER_IP, FRAISEQL_RATE_LIMIT_RPS_PER_USER,
      FRAISEQL_RATE_LIMIT_BURST_SIZE — each per-field)
```

The proxy-trust and zero-budget boot guards run once on whatever configuration comes out
of that resolution, so no source can smuggle an unguarded limiter past them.

Every other server knob (bind address, pool sizing, timeouts, …) is `*Config`, not a
compiled setting: it comes from the `--config` TOML file plus the explicit CLI/env
overrides documented in `fraiseql-server --help` (`ServerArgs`). If a variable is not in
`--help` output or the table above, the server does not read it.
