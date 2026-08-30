# FraiseQL

[![Rust](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Crates.io](https://img.shields.io/crates/v/fraiseql.svg)](https://crates.io/crates/fraiseql)
[![Test Coverage](https://img.shields.io/badge/tests-10000%2B-brightgreen.svg)](./crates/fraiseql-core/tests/)
[![preflight](https://github.com/fraiseql/fraiseql/actions/workflows/dagger-preflight.yml/badge.svg?branch=dev)](https://github.com/fraiseql/fraiseql/actions/workflows/dagger-preflight.yml)
[![security](https://github.com/fraiseql/fraiseql/actions/workflows/dagger-security.yml/badge.svg?branch=dev)](https://github.com/fraiseql/fraiseql/actions/workflows/dagger-security.yml)

**Compiled GraphQL execution engine.** Define schemas in Python or TypeScript, compile to optimized SQL at build time, execute with predictable sub-10ms latency.

Where Hasura and PostGraphile interpret GraphQL at request time, FraiseQL generates deterministic SQL templates during compilation, achieving zero runtime query planning overhead for known query patterns.

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

# 3. Run
fraiseql-server --config fraiseql.toml --schema schema.compiled.json

# 4. Query
curl -X POST http://localhost:8080/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users(limit: 5) { id name email } }"}'
```

## Why FraiseQL?

1. **Compile-time SQL generation.** SQL is generated at build time for deterministic queries. Your schema is analyzed once at build; queries execute without interpretation or query-planning overhead.

2. **Schema-as-code authoring.** Define schemas in Python or TypeScript with decorators, compile to optimized JSON. No runtime language bridge, no FFI.

3. **PostgreSQL, used properly.** SQL is generated for PostgreSQL specifically — JSONB projection, native RLS, `LISTEN/NOTIFY` subscriptions, WAL-based change capture — not translated through a lowest-common-denominator ORM layer.

## Performance

```
Traditional GraphQL:  Schema + Query -> Parse -> Plan -> SQL -> Execute -> Serialize
                      ~~~~~~~~~~~~~~ runtime overhead ~~~~~~~~~~~~~~

FraiseQL:             Compiled Schema -> SQL Template -> Execute -> Serialize
                      ~~~~ zero planning overhead ~~~~
```

Benchmarks: `crates/fraiseql-core/benches/` (Criterion, reproducible).

## Architecture

```
Authoring (Python/TS)     Compilation (Rust)        Runtime (Rust)
      |                         |                        |
  schema.json    +    fraiseql.toml    ->    schema.compiled.json    ->    Server
  (types)             (config)               (types + SQL templates)      (execute)
```

Python and TypeScript are authoring languages only. The runtime is pure Rust with zero language bridge overhead.

## Database Support

**PostgreSQL 14+ only.** MySQL, SQLite and SQL Server adapters were removed in
v2.15.0 — they had never been exercised against a real database and failed on
the primary query shape. A `mysql://`, `sqlite://` or `sqlserver://` URL is
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
| `fraiseql-rust` | Rust | crates.io | `cargo add fraiseql` |
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

> **Go note.** `sdks/official/fraiseql-go/go.mod` declares
> `module github.com/fraiseql/fraiseql-go`, a repository that does not exist, so
> `go get` cannot fetch it by that path or any other (#1224). Vendor the directory.

## Installation

**Rust applications:**

```toml
[dependencies]
fraiseql = { version = "2.15.0", features = ["server"] }
```

**Schema authoring:**

```bash
pip install fraiseql        # Python
npm install fraiseql        # TypeScript
cargo add fraiseql          # Rust
```

The other eight SDKs are source-only — see the table above.

**Feature flags:**

| Feature | Use Case |
|---------|----------|
| `postgres` (default) | PostgreSQL only |
| `server` | HTTP GraphQL server |
| `observers` | Post-mutation event hooks |
| `arrow` | Apache Arrow Flight for analytics |
| `wire` | Streaming JSON over PostgreSQL wire protocol |
| `full` | All features |

## Security

All queries are parameterized at compile time. Zero unsafe code (forbidden). Additional enterprise features:

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
- [SLA/SLO Targets](docs/sla.md) -- Availability and latency objectives
- [Operational Runbooks](docs/runbooks/) -- Incident response procedures
- [Security Checklist](docs/guides/production-security-checklist.md) -- Production hardening
- [Migration from v1](docs/guides/v1-to-v2-migration.md) -- Upgrade path

## Quality

- 10,000+ tests (unit, integration, E2E, property-based, fuzz)
- Cross-SDK parity suite: all 9 authoring SDKs (Python, TypeScript, Go, Java, PHP, C#, F#, Elixir, Rust SDK) produce identical schema JSON
- Golden fixture regression guards for every field in the compiled schema contract (protects against issue-#53-class bugs)
- Zero unsafe code (forbidden at compile time)
- Clippy pedantic as deny with justified suppressions
- Load testing infrastructure (k6)
- 15 operational runbooks

## Repository Layout

```
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
