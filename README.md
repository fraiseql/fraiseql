# FraiseQL

[![Rust](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Crates.io](https://img.shields.io/crates/v/fraiseql.svg)](https://crates.io/crates/fraiseql)
[![Test Coverage](https://img.shields.io/badge/tests-10000%2B-brightgreen.svg)](./crates/fraiseql-core/tests/)
[![preflight](https://github.com/fraiseql/fraiseql/actions/workflows/dagger-preflight.yml/badge.svg?branch=dev)](https://github.com/fraiseql/fraiseql/actions/workflows/dagger-preflight.yml)
[![security](https://github.com/fraiseql/fraiseql/actions/workflows/dagger-security.yml/badge.svg?branch=dev)](https://github.com/fraiseql/fraiseql/actions/workflows/dagger-security.yml)

**Compiled GraphQL execution engine.** Define schemas in Python or TypeScript, compile them into a validated schema bound to PostgreSQL views, and serve it from a Rust server.

Compilation validates the schema and binds each query to the PostgreSQL view it reads. At request time the server builds the SQL for the requested selection and runs it as a prepared statement (#1557).

## Quick Start

```python
# 1. Define schema (Python)
import fraiseql

@fraiseql.type
class User:
    id: int
    name: str
    email: str

@fraiseql.query
def users(limit: int = 10) -> list[User]:
    return fraiseql.config(sql_source="v_user", returns_list=True)

fraiseql.export_schema("schema.json")
```

```bash
# 2. Compile
fraiseql-cli compile schema.json -o schema.compiled.json

# 3. Run (binds 127.0.0.1:8000 unless --bind-addr or a --config file says otherwise)
DATABASE_URL=postgres://localhost/mydb fraiseql-server --schema-path schema.compiled.json

# 4. Query
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users(limit: 5) { id name email } }"}'
```

## Why FraiseQL?

1. **Schema checked before deploy.** `fraiseql compile` validates the schema and binds each query to the view or relation it reads. With `--database`, it also checks those relations against the live database and records native columns, foreign-key join columns and pagination order. SQL is not generated at compile time: each request builds its SQL from the selection set and runs it as a prepared statement (#1636, #1557).

2. **Schema-as-code authoring.** Define schemas in Python or TypeScript with decorators, compile to optimized JSON. No runtime language bridge, no FFI.

3. **PostgreSQL, used properly.** SQL is generated for PostgreSQL specifically — JSONB projection, native RLS, `LISTEN/NOTIFY` subscriptions, WAL-based change capture — not translated through a lowest-common-denominator ORM layer.

## Performance

```text
Request:  Parse query -> Match against compiled schema -> Build SQL for the selection
          -> Execute as a prepared statement -> Serialize
```

Benchmarks: `crates/fraiseql-core/benches/` (Criterion, reproducible).

## Architecture

```text
Authoring (Python/TS)     Compilation (Rust)        Runtime (Rust)
      |                         |                        |
  schema.json    +    fraiseql.toml    ->    schema.compiled.json    ->    Server
  (types)             (config)               (types + config + bindings)  (builds SQL per request)
```

Python and TypeScript are authoring languages only. The runtime is pure Rust with zero language bridge overhead.

## Database Support

**PostgreSQL 18+ only.** An older PostgreSQL server is refused at connect time.
MySQL, SQLite and SQL Server adapters were removed in v2.15.0 — they had never
been exercised against a real database and failed on the primary query shape. A `mysql://`, `sqlite://` or `sqlserver://` URL is
refused at startup with an explanatory error.

See [docs/database-compatibility.md](docs/database-compatibility.md) for what was
removed and why.

## Wire Protocol

`fraiseql-wire` is a separate read-only Rust crate for streaming bulk reads directly from PostgreSQL views. It is not part of the FraiseQL server. Mutations go through the GraphQL HTTP endpoint.

## Schema Authoring SDKs

Eleven SDKs live in `sdks/official/`. Three are published to a package registry and
are versioned in lockstep with the engine; the other eight are **source-only** — they
are tested on every push and score on the conformance suite, but no release publishes
them, so use them by vendoring the directory. This table is enforced by
`make lint-sdk-publication-claims`, which fails if it disagrees with what
`tools/release.sh` bumps or with what the workflows can actually publish.

<!-- sdk-table:start -->

| SDK | Language | Registry | Install |
|-----|----------|----------|---------|
| `fraiseql-python` | Python 3.11+ | PyPI | `pip install fraiseql` |
| `fraiseql-typescript` | TypeScript / Node.js | npm | `npm install fraiseql` |
| `fraiseql-rust` | Rust | crates.io | `cargo add fraiseql-rust` |
| `fraiseql-csharp` | C# / .NET 8+ | — | vendor `sdks/official/fraiseql-csharp` |
| `fraiseql-dart` | Dart / Flutter | — | vendor `sdks/official/fraiseql-dart` |
| `fraiseql-elixir` | Elixir | — | vendor `sdks/official/fraiseql-elixir` |
| `fraiseql-fsharp` | F# / .NET 8+ | — | vendor `sdks/official/fraiseql-fsharp` |
| `fraiseql-go` | Go 1.23+ | — | vendor `sdks/official/fraiseql-go` |
| `fraiseql-java` | Java 21+ | — | vendor `sdks/official/fraiseql-java` |
| `fraiseql-php` | PHP 8.2+ | — | vendor `sdks/official/fraiseql-php` |
| `fraiseql-ruby` | Ruby 3.2+ | — | vendor `sdks/official/fraiseql-ruby` |

<!-- sdk-table:end -->

Source-only is a statement about distribution, not quality: conformance scores are in
[`sdks/official/README.md`](sdks/official/README.md), and most source-only SDKs score
18/19 or 19/19. See [ADR-0019](docs/adr/0019-sdk-publication-boundary.md).

> **Rust SDK note.** `fraiseql-rust` first reaches crates.io at 2.16.0. It is a distinct
> package from the engine crate `fraiseql`: the SDK authors schemas, the engine executes
> them. Until 2.15.0 its only publisher fired on a `rust-sdk/v*` tag no release creates,
> and 2.15.0's release-tag publisher failed before uploading (#1488), so the row above
> named a registry that answered 404.

> **Go note.** `sdks/official/fraiseql-go/go.mod` declares
> `module github.com/fraiseql/fraiseql-go`, a repository that does not exist, so
> `go get` cannot fetch it by that path or any other (#1224). Vendor the directory.

## Installation

**Rust applications:**

```toml
# Cargo.toml
[dependencies]
fraiseql = { version = "2.19.0", features = ["server"] }
```

**Schema authoring:**

```bash
pip install fraiseql        # Python
npm install fraiseql        # TypeScript
cargo add fraiseql          # Rust
```

The other eight SDKs are source-only — see the table above.

**Feature flags:**

The `fraiseql` crate has no default features; PostgreSQL support is always compiled in.
The full list is in [docs/features-compatibility-matrix.md](docs/features-compatibility-matrix.md).

| Feature | Use Case |
|---------|----------|
| `server` | HTTP GraphQL server |
| `cli` | The `fraiseql` command-line binary |
| `observers` | Post-mutation event hooks |
| `arrow` | Apache Arrow Flight for analytics |
| `wire` | The `fraiseql-wire` streaming client and its read-only adapter |
| `full` | `server`, `cli`, `observers`, `observers-cache`, `arrow`, `wire`, `federation` |
| `release-full` | The feature set of the published `-full` release binary |

## Security

Queries pass user input to SQL as bind parameters. No hand-written unsafe code. Additional enterprise features:

- OAuth2/OIDC authentication (7+ providers)
- Field-level authorization
- Auth event logging (login attempts) via `fraiseql-auth`
- Rate limiting on auth endpoints
- Error sanitization (no implementation details leaked)
- Constant-time token comparison

### APQ Cache RLS Dependency

Automatic Persisted Query (APQ) caching isolates results per user via Row-Level Security. Different users must generate different WHERE clauses through their RLS policies. If RLS is disabled or generates an empty WHERE clause, two users with the same query and variables will receive the same cached response. Always verify RLS is active in multi-tenant deployments with caching enabled.

See [Security Checklist](docs/guides/production-security-checklist.md) for production hardening.

## Documentation

- [Getting Started](docs/guides/getting-started.md) -- 5-minute quick start
- [Architecture Documentation](docs/architecture/README.md) -- System design, compiler internals, security model
- [Value Proposition](docs/value-proposition.md) -- What FraiseQL does and does not do
- [Roadmap](roadmap.md) -- Prioritized next steps
- [Changelog](CHANGELOG.md) -- User-facing changes per version
- [Operational Runbooks](docs/runbooks/) -- Incident response procedures
- [Security Checklist](docs/guides/production-security-checklist.md) -- Production hardening
- [Migration from v1](docs/guides/v1-to-v2-migration.md) -- Upgrade path

## Quality

- 10,000+ tests (unit, integration, E2E, property-based, fuzz)
- Cross-SDK parity suite: 5 authoring SDKs (Python, TypeScript, Go and PHP for the full schema, Rust for types) produce identical schema JSON; `make test-parity` and the SDK Schema Parity workflow run it
- Golden fixture regression guards for every field in the compiled schema contract (protects against issue-#53-class bugs)
- No hand-written unsafe code (workspace lint `unsafe_code = "deny"`; only `wasmtime`-generated bindings are allowed it)
- Clippy pedantic as deny with justified suppressions
- Load testing infrastructure (k6)
- 15 operational runbooks

## Repository Layout

```text
crates/               # Rust engine crates (fraiseql-core, fraiseql-server, fraiseql-cli, …)
sdks/official/        # Official authoring SDKs (Python, TypeScript, Java, Go, Rust, PHP, …)
sdks/community/       # Community-maintained SDKs
docs/                 # Architecture docs, guides, runbooks
vendor/               # Vendored Rust patch dependencies ([patch.crates-io])
tutorial/             # Interactive tutorial platform — separate product, co-located for convenience
```

**Fraisier** (deployment orchestration tool) has been moved to its own repository at
[`github.com/fraiseql/fraisier`](https://github.com/fraiseql/fraisier).

See [`sdks/official/README.md`](sdks/official/README.md) for the full SDK inventory.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

Dual-licensed under MIT or Apache 2.0. See [LICENSE](LICENSE).
