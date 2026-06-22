# Compiled Schema Lifecycle

`schema.compiled.json` is the central deployment artifact of FraiseQL. This document
describes what it contains, how to treat it, and how to move it from development to production.

---

## What It Contains

`schema.compiled.json` contains:

- GraphQL type definitions
- Pre-compiled SQL templates for each query and mutation
- Security configuration (rate limits, error sanitization, RLS anchors)
- Cache TTL overrides per query
- Observer configuration (NATS URL, Redis URL)
- Federation circuit breaker thresholds

It does **not** contain database passwords, API keys, or user data. Connection strings are
resolved from environment variables at runtime, not embedded in the compiled schema.

---

## Sensitivity Classification

**Treat as sensitive but not secret.**

The SQL templates reveal your database schema structure (view names, column names, JOIN
relationships). Protect access similarly to how you protect database schema migrations.

| Property | Detail |
|----------|--------|
| Contains credentials? | No |
| Contains user data? | No |
| Reveals schema structure? | Yes |
| Safe for public repository? | No |
| Safe for private S3 / artifact store? | Yes |
| Should be encrypted at rest? | Recommended for high-security environments |

---

## Recommended CI/CD Deployment Flows

### Option A: Compile in CI, deploy as artifact (recommended)

```
Developer pushes schema changes
     ↓
CI: fraiseql compile fraiseql.toml
     ↓
CI: fraiseql lint schema.compiled.json      (validate structure)
     ↓
CI: Upload schema.compiled.json to S3 / artifact store
     ↓
Deployment: Server downloads from S3 at startup
```

**Pros**: Single source of truth, auditable, schema changes are decoupled from server deploys.

**Cons**: Requires S3 or artifact storage infrastructure.

### Option B: Compile into Docker image

```dockerfile
FROM rust:1.78 AS builder
COPY fraiseql.toml types.json ./
RUN fraiseql compile fraiseql.toml

FROM debian:bookworm-slim
COPY --from=builder schema.compiled.json .
COPY --from=builder fraiseql-server .
ENTRYPOINT ["./fraiseql-server", "--schema", "schema.compiled.json"]
```

**Pros**: Self-contained image, no external storage required.

**Cons**: Schema baked into image; schema changes require a full image rebuild and redeploy.

### Option C: Compile at container startup (development only)

```bash
#!/bin/sh
# entrypoint.sh — do NOT use in production
fraiseql compile fraiseql.toml
exec fraiseql-server --schema schema.compiled.json
```

**Pros**: Simple local development flow.

**Cons**: Compilation errors cause deployment failures at startup, not at build time.
Only appropriate for development and staging environments.

---

## Validation Before Deployment

Always validate the compiled schema in CI before deployment:

```bash
# Validate structure and embedded SQL
fraiseql lint schema.compiled.json
```

The `lint` command checks:

- All SQL templates parse as valid SQL
- Security configuration is internally consistent
- No deprecated fields are present
- Required fields for enabled features are populated

> **Known gap**: `fraiseql validate-compiled --check-views-exist` (which connects to
> the database and verifies all SQL views exist at deploy time) is planned but not yet
> implemented. Until then, view existence is verified at server startup.

---

## Server Startup Behaviour

The server loads `schema.compiled.json` at startup. Configuration priority (highest to lowest):

1. **Environment variables** — `DATABASE_URL`, `REDIS_URL`, `NATS_URL`, etc.
2. **Compiled schema values** — security config, cache TTLs, observer settings
3. **Built-in defaults**

The server serves one compiled schema at any instant, but it **can** be swapped without a
restart: send `SIGUSR1` (auto-installed when a schema path is configured) or call
`POST /api/v1/admin/reload-schema`, and the server re-reads + validates the file and
atomically swaps the executor — in-flight requests finish on the old schema, new requests get
the new one, and a failed reload keeps the previous schema. Rolling deployments also work
correctly: new pods start with the new schema while old pods drain existing connections. See
[zero-downtime-deploys.md](zero-downtime-deploys.md) for the full deploy story and
[../runbooks/13-schema-hot-reload-failure.md](../runbooks/13-schema-hot-reload-failure.md) for
the reload trigger commands and failure diagnosis.

---

## Schema Versioning

`schema.compiled.json` carries an integer `schema_format_version` describing the compiled-schema
binary format. At startup the server checks it against the version it expects:

- **Version mismatch**: Server refuses to start and logs a fatal error.
- **Field absent**: Server starts and emits a `WARN` log (legacy schema, format assumed current).

Always compile with a `fraiseql-cli` whose `schema_format_version` matches the server you are
deploying — in practice, keep the CLI and server versions in lockstep.

---

## Who Owns the Compile Step?

| Team | Responsibility |
|------|---------------|
| Developer | Modifies Python/TypeScript schema decorators |
| CI pipeline | Runs `fraiseql compile` and `fraiseql lint` on every PR |
| DevOps / platform | Stores compiled artifact; injects into deployment |
| Server process | Loads compiled artifact at startup |

The compile step is a build-time concern, not a runtime concern.

---

## Gitignore

`schema.compiled.json` should be in `.gitignore`. The compiled artifact belongs in your
artifact store, not version control:

```gitignore
# FraiseQL compiled schema — store in CI artifact store, not git
schema.compiled.json
```

The source files (`fraiseql.toml`, Python/TypeScript decorators) are the version-controlled
inputs. The compiled schema is a derived output.
