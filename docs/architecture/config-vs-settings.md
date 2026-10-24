# Config vs Settings

FraiseQL uses 150+ `*Config` types and a handful of `*Settings` types. The suffix says
where a value comes from, but the split is not strict: the compiler parses some compiled
sections into `*Config` types, and the server has a few `*Settings` types that are never
compiled. Read the source column below, not the suffix alone.

## Definitions

| Suffix | Source | Lifecycle | Example |
|--------|--------|-----------|---------|
| `*Config` | Project `fraiseql.toml` (read by `fraiseql compile`), the server's `--config` TOML, env vars, CLI flags | Project config is compiled into `schema.compiled.json`; server config is read at each boot | `ServerConfig`, `OidcConfig`, `RateLimitConfig` |
| `*Settings` | Project `fraiseql.toml`, read by `fraiseql-cli` | Compiled into `schema.compiled.json` | `FraiseQLSettings`, `SecuritySettings`, `FunctionsSettings` |

## Flow

```text
fraiseql.toml                    fraiseql-cli compile               fraiseql-server start
  [fraiseql.security]     --->   fraiseql_cli::config::      --->    CompiledSchema.security:
  rate_limiting = {...}          SecurityConfig, or                   fraiseql_core::schema::
  audit_logging = {...}          SecuritySettings for [security]      SecurityConfig, read once
                                 (validated, embedded under the       at boot
                                  compiled "security" key)
```

### `*Config` (Developer-Facing)

Config types represent **developer intent** expressed in TOML or environment variables. They may contain raw paths, URLs, or feature toggles that need validation before use.

- Defined in: `fraiseql-server/src/server_config/`, `fraiseql-server/src/config/`,
  `fraiseql-cli/src/config/`, and next to the subsystem that uses them (for example
  `fraiseql-server/src/middleware/rate_limit/config.rs`)
- Loaded from: the server's `--config` TOML, the project `fraiseql.toml`, environment
  variables, CLI flags
- Validated: at load time (parse errors, unknown keys) and at startup
- May change between deployments

### `*Settings` (Compiled)

Settings types are read from the project `fraiseql.toml` by the compiler and lowered into
`schema.compiled.json`. The server never deserializes them: it reads the compiled result
through `fraiseql_core` schema types.

- Defined in: `fraiseql-cli/src/config/` (`FraiseQLSettings` in `mod.rs`,
  `SecuritySettings` and `FunctionsSettings` in `toml_schema/`)
- Loaded from: project `fraiseql.toml`
- Validated: at compile time by `fraiseql-cli compile`
- Fixed for the server's lifetime once loaded: a schema hot-reload that changes a
  section a subsystem read at boot (such as `security`) is refused until a restart
  (`fraiseql-server/src/routes/graphql/reload_gate.rs`, #782)

`ObserverRuntimeSettings`, `LedgerSettings` and `SendSettings` in `fraiseql-server` are
exceptions: they are server runtime configuration, not compiled.

## The Compiled Settings Types

| Type | Crate | Purpose |
|------|-------|---------|
| `FraiseQLSettings` | `fraiseql-cli` | The `[fraiseql]` table of a project `fraiseql.toml` (`TomlProjectConfig`). Its `security` field is `fraiseql_cli::config::SecurityConfig` |
| `SecuritySettings` | `fraiseql-cli` | The `[security]` table of a TOML-schema project (`TomlSchema`), compiled into the schema's `"security"` key |
| `FunctionsSettings` | `fraiseql-cli` | The `[functions]` table (module directory, dead-letter store) |

The server reads the compiled `"security"` key as `fraiseql_core::schema::SecurityConfig`
(`CompiledSchema.security`). Many subsystems read it: the error sanitizer, state
encryption, rate limiting, API keys, service accounts, token revocation, tenancy and
`RuntimeConfig`, among others.

## When to Use Which

- **Adding a new server knob** (bind address, pool size, feature toggle) -> `*Config` in `server_config/`
- **Adding a new security policy** that gets baked into the compiled schema -> a field on the `fraiseql-cli` config type for its section, lowered into `fraiseql_core::schema::SecurityConfig`
- **Adding runtime-only middleware config** (rate limiting per IP, CORS origins) -> `*Config` in `server_config/`

## Environment Variable Overrides

Most compiled settings are **immutable at runtime** — that is the point of compiling them.
There is no generic `FRAISEQL_*` prefix engine that maps every setting onto a variable; the
server reads each override explicitly, and the complete list of compiled settings with a
runtime override is:

| Variable | Overrides |
|----------|-----------|
| `FRAISEQL_MAX_PAGE_SIZE` | The compiled page-size ceiling (#421) |
| `FRAISEQL_MAX_OFFSET` | The compiled offset ceiling, `[validation] max_offset` (#1306); `0`/`none` lifts it |
| `FRAISEQL_CHANGELOG_ENABLED` | The compiled change-log toggle (composes AND with the compiled value) |
| `FRAISEQL_FUNCTIONS_MODULE_DIR` | The compiled `[functions] module_dir`, where function modules load from (#1715) |
| `FRAISEQL_FUNCTIONS_DLQ_STORE` | The compiled `[functions] dlq_store` backend |
| `FRAISEQL_FUNCTIONS_DLQ_MAX_SIZE` | The compiled functions DLQ size cap |
| `FRAISEQL_FUNCTIONS_RETRY_MAX_ATTEMPTS` / `_INITIAL_DELAY_MS` / `_MAX_DELAY_MS` | The compiled functions retry policy |
| `FRAISEQL_FUNCTIONS_BEFORE_MUTATION_BUDGET_MS` | The `before:mutation` chain's wall-clock ceiling (default 500 ms; `0` disables it, logged loudly at startup) |
| `FRAISEQL_FUNCTIONS_REQUEST_QUERY_BUDGET_MS` | The default wall-clock ceiling on a `request:query` invocation that declares no `timeout_ms` (default 5 000 ms; `0` disables it, logged loudly at startup). A function's own `timeout_ms` wins over it — see [functions.md](./functions.md#answering-a-request-from-a-function-1329) |
| `FRAISEQL_SOURCES_ENABLED` | Whether the source scheduler runs at all |
| `FRAISEQL_EGRESS_ALLOWED_DOMAINS` | `[egress] allowed_domains`, the hosts functions and source connectors may call |
| `FRAISEQL_SOURCES_ALLOWED_ENV_VARS` | The sources env-var allowlist |

Rate limiting is the special case, and its precedence is implemented and pinned by tests
(`server::tests::rate_limit_boot_guard_tests`). Resolution order, **lowest to highest**,
each layer applied over the last:

```text
server [rate_limiting] table (the TOML file passed via --config)
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
