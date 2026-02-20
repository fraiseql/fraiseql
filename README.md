# FraiseQL

[![Rust](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Crates.io](https://img.shields.io/crates/v/fraiseql.svg)](https://crates.io/crates/fraiseql)
[![Test Coverage](https://img.shields.io/badge/tests-4773%2B-brightgreen.svg)](./crates/fraiseql-core/tests/)
![Build](https://github.com/fraiseql/fraiseql/actions/workflows/ci.yml/badge.svg)

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

1. **Compile-time SQL generation.** Zero runtime overhead for deterministic queries. Your schema is analyzed once at build; queries execute without interpretation.

2. **Schema-as-code authoring.** Define schemas in Python or TypeScript with decorators, compile to optimized JSON. No runtime language bridge, no FFI.

3. **Multi-database from one schema.** PostgreSQL, MySQL, SQLite, SQL Server from a single compiled schema. Per-database SQL generation, not ORM translation.

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

## Supported Databases

| Database | Status | Feature Flag |
|----------|--------|-------------|
| PostgreSQL | Primary | `postgres` (default) |
| MySQL | Supported | `mysql` |
| SQLite | Supported | `sqlite` |
| SQL Server | Supported | `sqlserver` |

## Schema Authoring SDKs

| Tier | Languages |
|------|-----------|
| Tier 1 (Supported) | Python, TypeScript, Java, Go |
| Tier 2 (Maintained) | PHP, Rust |

## Installation

**Rust applications:**

```toml
[dependencies]
fraiseql = { version = "2.0.0-beta.3", features = ["server"] }
```

**Schema authoring:**

```bash
pip install fraiseql        # Python
npm install fraiseql        # TypeScript
```

**Feature flags:**

| Feature | Use Case |
|---------|----------|
| `postgres` (default) | PostgreSQL only |
| `mysql`, `sqlite`, `sqlserver` | Additional databases |
| `server` | HTTP GraphQL server |
| `observers` | Post-mutation event hooks |
| `arrow` | Apache Arrow Flight for analytics |
| `wire` | Streaming JSON over PostgreSQL wire protocol |
| `full` | All features |

## Security

All queries are parameterized at compile time. Zero unsafe code (forbidden). Additional enterprise features:

- OAuth2/OIDC authentication (7+ providers)
- Field-level authorization and encryption-at-rest
- Audit logging (file, PostgreSQL, Syslog)
- Rate limiting on auth endpoints
- Error sanitization (no implementation details leaked)
- Constant-time token comparison

See [Security Checklist](docs/guides/production-security-checklist.md) for production hardening.

## Documentation

- [Getting Started](docs/guides/getting-started.md) -- 5-minute quick start
- [Architecture Principles](ARCHITECTURE_PRINCIPLES.md) -- System design
- [Value Proposition](docs/VALUE_PROPOSITION.md) -- What FraiseQL does and does not do
- [Roadmap](ROADMAP.md) -- Prioritized next steps
- [Changelog](CHANGELOG.md) -- User-facing changes per version
- [SLA/SLO Targets](docs/sla.md) -- Availability and latency objectives
- [Operational Runbooks](docs/runbooks/) -- Incident response procedures
- [Security Checklist](docs/guides/production-security-checklist.md) -- Production hardening
- [Migration from v1](docs/guides/v1-to-v2-migration.md) -- Upgrade path

## Quality

- 4,773+ tests (unit, integration, E2E, property-based, fuzz)
- Zero unsafe code (forbidden at compile time)
- Clippy pedantic as deny with justified suppressions
- Load testing infrastructure (k6)
- 12 operational runbooks

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

Dual-licensed under MIT or Apache 2.0. See [LICENSE](LICENSE).
