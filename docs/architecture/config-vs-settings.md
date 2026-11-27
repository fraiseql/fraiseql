# Config vs Settings

FraiseQL uses 150+ `*Config` types and a handful of `*Settings` types. The suffix says
where a value comes from, but the split is not strict: the compiler parses some compiled
sections into `*Config` types, and the server has a few `*Settings` types that are never
compiled. Read the source column below, not the suffix alone.

## Definitions

| Suffix | Source | Lifecycle | Example |
|--------|--------|-----------|---------|
| `*Config` | Project `fraiseql.toml` (read by `fraiseql compile`), the server's `--config` TOML, env vars, CLI flags | Project config is compiled into `schema.compiled.json`; server config is read at each boot | `ServerConfig`, `OidcConfig`, `RateLimitConfig` |
| `*Settings` | Project `fraiseql.toml`, read by `fraiseql-cli` | Compiled into `schema.compiled.json` | `SecuritySettings`, `FunctionsSettings` |

## Flow

```text
fraiseql.toml                    fraiseql-cli compile               fraiseql-server start
  [security]     --->   fraiseql_cli::config::      --->    CompiledSchema.security:
  rate_limiting = {...}          SecuritySettings                     fraiseql_core::schema::
  pkce = {...}                   (validated, embedded under the       SecurityConfig, read once
                                  compiled "security" key)            at boot
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

- Defined in: `fraiseql-cli/src/config/toml_schema/` (`SecuritySettings`,
  `FunctionsSettings`)
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
| `SecuritySettings` | `fraiseql-cli` | The `[security]` table of every `fraiseql.toml` (`TomlSchema`), lowered with `[tenancy]` into the schema's `"security"` key |
| `FunctionsSettings` | `fraiseql-cli` | The `[functions]` table (module directory, dead-letter store) |

The server reads the compiled `"security"` key as `fraiseql_core::schema::SecurityConfig`
(`CompiledSchema.security`). Many subsystems read it: the error sanitizer, state
encryption, rate limiting, API keys, service accounts, token revocation, tenancy and
`RuntimeConfig`, among others.

## Runtime Sections in a Project `fraiseql.toml`

A project that authors its schema with an SDK keeps a `fraiseql.toml` beside the
`schema.json` the SDK emits. Every `fraiseql.toml` is read by one type, `TomlSchema`, so
that file takes the settings a TOML schema takes, in the same spelling, and they compile
to the same thing in both workflows:

```toml
# fraiseql.toml
[project]
name = "orders"

[mcp]
enabled = true

[rest]
enabled = true
path = "/api"

[validation]
max_query_depth = 12
```

| Sections | Beside a `schema.json` |
|---|---|
| `[project]`, `[security]`, `[tenancy]`, `[session_variables]`, `[naming]`, `[cost_weights]`, `[mutation_defaults]`, `[mcp]`, `[rest]`, `[grpc]`, `[debug]`, `[validation]`, `[subscriptions]`, `[federation]`, `[auth]`, `[changelog]`, `[hierarchies]`, `[query_defaults]`, `[caching]`, `[observers]`, `[functions]`, `[locale]`, `[inject_defaults]` | Honoured. |
| `[types]`, `[queries]`, `[mutations]`, `[includes]`, `[domain_discovery]`, `[crud]`, `[analytics]` | Refused by name: they author the schema, which the schema document does here. |

A retired spelling is refused by name in both roles, with its replacement: the
`[fraiseql]` prefix (`[fraiseql.security]` is `[security]`), `[schema]` (`[project]`),
`naming_convention` (`[naming] convention`), and `[observability]` (metrics and tracing
are keys of the server's own configuration file).

A schema document may carry its own copy of a section (`mcp_config`, `rest_config`, …).
One source alone applies. Two that agree compile. Two that differ are refused, and the
error names both sources and the first key they differ on: neither file overrides the
other. A table that only says `enabled = false` is still a declaration, and disagrees with
a copy that says `true`. The same holds between a TOML schema and the types document it is
compiled with (`--types`): there too, a table is declared when the file writes it.

`[[caching.rules]]` has no copy to reconcile: each rule lowers onto the query it names. A
rule for a query whose TTL the schema document already declares is refused, for the same
reason.

`[naming] convention` is the one setting that is not reconciled: an SDK writes its
own convention into every document, so the project config decides when it writes the key,
and the document's value stands when it does not.

## When to Use Which

- **Adding a new server knob** (bind address, pool size, feature toggle) -> `*Config` in `server_config/`
- **Adding a new security policy** that gets baked into the compiled schema -> a field on the `fraiseql-cli` config type for its section, lowered into `fraiseql_core::schema::SecurityConfig`
- **Adding runtime-only middleware config** (rate limiting per IP, CORS origins) -> `*Config` in `server_config/`

## `${VAR}` Placeholders

Both files expand `${VAR}` from the environment before they are parsed, with one expander,
but at different times, and the difference decides where a value ends up:

- **`fraiseql.toml` is expanded by `fraiseql compile`.** The value is pasted into the file's
  text, and whatever key it sits under is compiled as written: the value itself lands in
  `schema.compiled.json`, an artifact that travels (container images, artifact stores, CI
  caches, config maps). A placeholder whose variable is not set where you compile stays in
  the artifact as written, `${NAME}` and all, and is not expanded again at runtime.
- **The server's configuration file is expanded when the server starts**, where it is read;
  nothing it holds is written anywhere.

A secret therefore does not belong in a `${VAR}` placeholder in `fraiseql.toml`. Each key
that takes a credential or a credential-bearing URL has an `*_env` twin naming the variable
that holds it: the compiled schema carries the variable's name, and the server reads its
value at startup.

| Section | Key |
|---------|-----|
| `[auth]` | `client_secret_env` |
| `[auth.database]` | `url_env` |
| `[auth.social.google]`, `.github`, `.discord`, `.facebook` | `client_secret_env` |
| `[auth.social.apple]` | `private_key_env` |
| `[[security.service_accounts]]` | `secret_env` |
| `[security.state_encryption]` | `key_env` |
| `[security.rate_limiting]`, `[security.pkce]`, `[security.token_revocation]` | `redis_url_env` |
| `[observers]` | `nats_url_env` |
| an observer action in the schema document | `url_env`, `webhook_url_env` |

`[database] url` is the exception that needs none: the compiler reads it to check the
database and does not compile it, so `url = "${DATABASE_URL}"` is safe there.

The expansion itself is held to the same rules in both files. A value must be printable UTF-8 text: one holding a control character (a tab
included), an invisible format character (Unicode Cf) or a separator other than the plain
space (Unicode Zs, Zl, Zp) is refused wherever its placeholder sits, naming the variable and
the class, as is one that is not UTF-8. A value holding a TOML metacharacter (`"`, `'`, `\`,
`[`, `]`, `{`, `}`) is refused too.

A placeholder written outside a quoted string (`timeout = ${T}`) puts its value into the
file as bare TOML, where `#` would end the line early and `,` or `=` would add keys to the
inline table it sits in. There a value holding `#`, `,`, `=` or a space is refused too,
naming the variable and not the value; quote the placeholder (`name = "${NAME}"`) to take
such a value as text. A placeholder inside a string or a comment is text either way.

The two files differ on a variable that is not set:

- `fraiseql.toml` keeps the placeholder as written: the file is compiled where the
  deployment's variables may not exist.
- The server's configuration file (`--config`, `FRAISEQL_CONFIG`) is refused, naming the
  variable: it is read where the deployment runs, and a placeholder kept as a value would
  become that value, an `admin_token` of `${ADMIN_TOKEN}`.

A parse error in either file whose variables were expanded gives the line and the reason,
not the line's text, which may hold a secret. Every quoted span of the reason is shown as
`<value>` (`invalid type: string <value>, expected u32`), and a reason that still holds an
expanded value is replaced whole. A file with nothing expanded keeps the whole error.

## Refused Configuration

A configuration `fraiseql` refuses exits with code 2, as a schema that does not validate
does. Under `--json` the error carries a `diagnostic`, so a tool can tell which kind of
refusal it is and find the key without reading the message:

```json
{"error": {"message": "Failed to parse fraiseql.toml; …", "code": 2,
           "diagnostic": {"code": "E-CONFIG-RETIRED", "file": "fraiseql.toml",
                          "table": "schema", "key": null, "replacement": "`[project]`",
                          "belongs_in": null, "also_in": null}}}
```

`table` is dotted (`security.rate_limiting`) and empty for a top-level key; `key` is `null`
when the whole table is refused; `replacement` is what to write instead, when there is
something; `belongs_in` names the file a misplaced setting belongs in, when it is another
file; `also_in` is `{"file": …, "key": …}` for a setting made twice, the other file and the
key as that file spells it, and `null` otherwise. `fraiseql doctor --json` puts the same diagnostic on its config check, and `fraiseql
run` gives it for the server's configuration file too. The standalone `fraiseql-server`
keeps its own exit status.

### Diagnostic Codes

| Code | Refused |
|---|---|
| `E-CONFIG-RETIRED` | A spelling the file no longer takes; `replacement` names the one it takes |
| `E-CONFIG-UNREAD` | A key nothing reads; there is nothing to write instead |
| `E-CONFIG-UNAPPLIED` | A key `fraiseql run` never applied |
| `E-CONFIG-MISPLACED` | A setting written in the wrong file or table |
| `E-CONFIG-DISAGREES` | A setting `fraiseql.toml` and the schema document both declare, differently |
| `E-CONFIG-UNKNOWN-KEY` | A key the table it is written in does not take; the replacement names the nearest accepted spelling, if one is near |
| `E-CONFIG-SET-TWICE` | A setting `fraiseql run` reads from `fraiseql.toml` and from the server's configuration file, made in both, whether or not the two values agree; `also_in` names the server file and its key |

### Exit Codes

| Code | Meaning |
|---|---|
| 0 | Command completed successfully |
| 1 | Command failed with an error |
| 2 | Input refused: the schema, a document or the configuration did not validate |

A command may document outcome codes of its own: `fraiseql functions invoke` exits 3 and 4
(see [functions.md](./functions.md)).

## Editor Support

`fraiseql.toml` has a JSON Schema, generated from the grammar the compiler reads:

```text
https://raw.githubusercontent.com/fraiseql/fraiseql/v2.19.1/docs/schemas/fraiseql.toml.schema.json
```

The URL is the schema committed at the release's tag, so a file pointing at it is checked
against the grammar of the compiler it was written for. It returns 404 until that release
is tagged; a build from an unreleased checkout has its schema at
`docs/schemas/fraiseql.toml.schema.json` in that checkout.

Taplo (and the Even Better TOML extension, which runs it) reads the schema from a
`#:schema` line at the top of the file. `fraiseql init` writes that line for its own
version:

```toml
# fraiseql.toml
#:schema https://raw.githubusercontent.com/fraiseql/fraiseql/v2.19.1/docs/schemas/fraiseql.toml.schema.json

[project]
name = "my-app"
```

Or associate every `fraiseql.toml` of a repository in its `.taplo.toml`:

```toml
# .taplo.toml
[[rule]]
include = ["**/fraiseql.toml"]

[rule.schema]
path = "https://raw.githubusercontent.com/fraiseql/fraiseql/v2.19.1/docs/schemas/fraiseql.toml.schema.json"
```

An editor then flags what the compiler refuses: a key no table takes (the compiler's
`E-CONFIG-UNKNOWN-KEY`), a key the compiler refuses by name (shown as deprecated, with the
compiler's refusal as its description), and a value of the wrong type. The tables that
author a schema (`[types]`, `[queries]` and the rest) are accepted: a schema authored in
TOML takes them, and the compiler refuses them only beside a schema document.

### The Server's Configuration File

The file `--config` or `FRAISEQL_CONFIG` names has a JSON Schema too, generated from the type
the server reads it into:

```text
https://raw.githubusercontent.com/fraiseql/fraiseql/v2.19.1/docs/schemas/fraiseql-server.schema.json
```

It is pinned to the release's tag the same way. The file has no fixed name, so point
at it with a `#:schema` line, or with a `.taplo.toml` rule naming your file:

```toml
# .taplo.toml
[[rule]]
include = ["deploy/server.toml"]

[rule.schema]
path = "https://raw.githubusercontent.com/fraiseql/fraiseql/v2.19.1/docs/schemas/fraiseql-server.schema.json"
```

The schema is generated with every feature on, so it names every key any build of the
server reads. Beyond the types, it carries what the server does with each key:

- A key no table takes is flagged. The server refuses it as `E-CONFIG-UNKNOWN-KEY`,
  naming the nearest accepted spelling, and so does `fraiseql run`, which reads the file
  with the server's loader.
- A key the server refuses by name is shown as deprecated, with the server's refusal as
  its description. Examples: a grouping table the file never had (`[server]`, `[cache]`),
  a `fraiseql.toml` spelling (`[tls] cert_file`), a retired section (`[auth_hs256]`), a
  renamed key (`[rate_limiting] rps_per_ip`), and a flat `[observers]` tuning key.
- `x-fraiseql-env` names the environment variable that overrides the key, such as
  `FRAISEQL_POOL_MAX_SIZE` on `pool_max_size`. Some variables set no key of the file:
  `FRAISEQL_CONFIG` itself, `FRAISEQL_LOG_FORMAT`, and the `FRAISEQL_RATE_LIMIT*` overrides,
  which apply over the compiled schema's limits.
- `x-fraiseql-feature` names the build feature a key needs, such as `observers` on
  `[observers]`. A binary built without it refuses the key and names the feature.

## Environment Variable Overrides

Most compiled settings are **immutable at runtime** — that is the point of compiling them.
There is no generic `FRAISEQL_*` prefix engine that maps every setting onto a variable; the
server reads each override explicitly, and the complete list of compiled settings with a
runtime override is:

| Variable | Overrides |
|----------|-----------|
| `FRAISEQL_MAX_PAGE_SIZE` | The compiled page-size ceiling (#421); a positive integer, anything else refuses boot |
| `FRAISEQL_MAX_OFFSET` | The compiled offset ceiling, `[validation] max_offset` (#1306); a positive integer, anything else refuses boot |
| `FRAISEQL_CHANGELOG_ENABLED` | The compiled change-log toggle (composes AND with the compiled value) |
| `FRAISEQL_FUNCTIONS_MODULE_DIR` | The compiled `[functions] module_dir`, where function modules load from (#1715) |
| `FRAISEQL_FUNCTIONS_SHARED_DIR` | The compiled `[functions] shared_dir`, where shared modules load from (#1651) |
| `FRAISEQL_FUNCTIONS_DLQ_STORE` | The compiled `[functions] dlq_store` backend; `memory` or `postgres`, anything else refuses boot |
| `FRAISEQL_FUNCTIONS_DLQ_MAX_SIZE` | The compiled functions DLQ size cap |
| `FRAISEQL_FUNCTIONS_RETRY_MAX_ATTEMPTS` / `_INITIAL_DELAY_MS` / `_MAX_DELAY_MS` | The compiled functions retry policy |
| `FRAISEQL_FUNCTIONS_BEFORE_MUTATION_BUDGET_MS` | The `before:mutation` chain's wall-clock ceiling (default 500 ms; `0` disables it, logged loudly at startup) |
| `FRAISEQL_FUNCTIONS_REQUEST_QUERY_BUDGET_MS` | The default wall-clock ceiling on a `request:query` invocation that declares no `timeout_ms` (default 5 000 ms; `0` disables it, logged loudly at startup). A function's own `timeout_ms` wins over it — see [functions.md](./functions.md#answering-a-request-from-a-function-1329) |
| `FRAISEQL_SOURCES_ENABLED` | Whether the source scheduler runs at all |
| `FRAISEQL_EGRESS_ALLOWED_DOMAINS` | `[egress] allowed_domains`, the hosts functions and source connectors may call |
| `FRAISEQL_SOURCES_ALLOWED_ENV_VARS` | The sources env-var allowlist |

Rate limiting is the special case, pinned by tests
(`server::tests::rate_limit_boot_guard_tests`). Its policy is declared in one file: the
compiled `[security.rate_limiting]` (from `fraiseql.toml`) or the server's `[rate_limiting]`
table, which takes the same keys plus `cleanup_interval_secs`. When both carry it the
server refuses to boot: neither replaces the other. The CLI flags and environment
variables (`FRAISEQL_RATE_LIMITING_ENABLED`, `FRAISEQL_RATE_LIMIT_RPS_PER_IP`,
`FRAISEQL_RATE_LIMIT_RPS_PER_USER`, `FRAISEQL_RATE_LIMIT_BURST_SIZE`) then override it,
each per field.

The proxy-trust and zero-budget boot guards run once on whatever configuration comes out
of that resolution, so no source can smuggle an unguarded limiter past them.

Every other server knob (bind address, pool sizing, timeouts, …) is `*Config`, not a
compiled setting: it comes from the `--config` TOML file plus the explicit CLI/env
overrides documented in `fraiseql-server --help` (`ServerArgs`). If a variable is not in
`--help` output or the table above, the server does not read it.
