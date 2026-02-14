# FraiseQL v2

[![Rust](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Crates.io](https://img.shields.io/crates/v/fraiseql.svg)](https://crates.io/crates/fraiseql)
[![Test Coverage](https://img.shields.io/badge/tests-4773%2B-brightgreen.svg)](./crates/fraiseql-core/tests/)
![Build](https://github.com/fraiseql/fraiseql/actions/workflows/ci.yml/badge.svg)

**v2.0.0-alpha.5** | **Alpha** | **Compiled GraphQL Engine for Relational Databases**

**Rust** | **16+ Language SDKs** | **4+ Databases**

---

## 🎯 GraphQL Without The Overhead

**Compile your schema. Execute queries at runtime. Zero interpretation.**

FraiseQL v2 is a compiled GraphQL execution engine that transforms schema definitions into optimized SQL at build time, eliminating runtime overhead entirely.

```rust
// 1. Define schema (Python, TypeScript, Rust, etc.)
// See docs/guides/language-generators.md

// 2. Compile it
// $ fraiseql-cli compile schema.json -o schema.compiled.json

// 3. Run the server
let schema = CompiledSchema::from_file("schema.compiled.json")?;
let db = Arc::new(db::PostgresAdapter::new(&url).await?);
let server = Server::new(config, schema, db, None).await?;
server.serve().await?

// 4. Execute queries
// POST /graphql
// { "query": "{ users(limit: 10) { id name email } }" }
```

---

## 💡 Why FraiseQL v2?

| Benefit | Value | Impact |
|---------|-------|--------|
| **Zero Interpretation** | All schema logic resolved at compile time | 10-20x faster than traditional GraphQL |
| **Multi-Database** | PostgreSQL, MySQL, SQLite, SQL Server support | Write once, deploy anywhere |
| **Type Safe** | Strict type system + Rust's memory safety | Impossible categories of bugs prevented at compile time |
| **Production Ready** | 4,773+ tests, zero unsafe code | Enterprise-grade reliability |
| **No N+1 Queries** | Joins determined at build time | Automatic optimization without code changes |
| **Security by Default** | Parameterized queries, field-level auth | All queries protected from SQL injection |

---

## 🚀 Performance

```
Traditional GraphQL:  PostgreSQL → ORM → Deserialize → Python Objects → Serialize → JSON
                      └─────── Overhead: 60-70% Python runtime ──────┘

FraiseQL v2:          PostgreSQL → Direct SQL → JSONB/Arrow Response
                      └─────── Overhead: 0% (fully compiled) ──────┘

Result:               10-20x faster query execution
                      10x higher throughput
                      Bounded memory usage (streaming support)
```

📊 **Real benchmarks:** See `crates/fraiseql-core/benches/` for reproducible performance data.

---

## 🛠️ Technology Stack

| Component | Technology | Why |
|-----------|-----------|-----|
| **Execution Engine** | Rust | Zero-cost abstractions, memory safe, blazingly fast |
| **Schema Authoring** | 16+ Languages | Python, TypeScript, Go, Java, Kotlin, Rust, Scala, + 9 more |
| **Databases** | Native Drivers | PostgreSQL (primary), MySQL, SQLite, SQL Server |
| **HTTP Server** | Axum | Modern async Rust, minimal overhead |
| **Data Formats** | JSON, Arrow Flight | JSON for APIs, Arrow for analytics + warehouses |

---

## 📋 What's Included

### Core Engine ✅
- [x] GraphQL execution (queries, mutations, types, interfaces, unions)
- [x] Automatic WHERE type generation (150+ operators for PostgreSQL)
- [x] Apollo Federation v2 with SAGA transactions
- [x] Automatic Persisted Queries (APQ) with allowlisting
- [x] Query result caching with automatic invalidation
- [x] Multi-tenant isolation with per-tenant data scoping

### Enterprise Security ✅
- [x] Audit logging (file, PostgreSQL, Syslog backends)
- [x] Rate limiting on auth endpoints
- [x] Field-level authorization via directives
- [x] Field-level encryption-at-rest
- [x] Credential rotation automation
- [x] OAuth2/OIDC (7+ providers: GitHub, Google, Auth0, Azure AD, Keycloak, Okta, + extensible)
- [x] Constant-time token comparison (timing attack prevention)
- [x] Error sanitization (no implementation details leaked)

### Data Integration ✅
- [x] Webhooks (Discord, Slack, GitHub, Stripe + custom)
- [x] Change Data Capture at database layer
- [x] NATS JetStream messaging
- [x] Backup & disaster recovery
- [x] Stream large result sets via fraiseql-wire (PostgreSQL)
- [x] Apache Arrow Flight for analytics tools

### Testing ✅
- [x] 4,773+ tests (unit, integration, E2E, chaos engineering)
- [x] Zero unsafe code (forbidden at compile time)
- [x] All Clippy warnings are errors
- [x] Chaos engineering validated transactions

---

## ⚡ Quick Start

### 1. Create Schema (Pick Your Language)

**Python:**
```python
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

**TypeScript:**
```typescript
import { type, query, export_schema } from "fraiseql";

@type()
class User {
  id!: number;
  name!: string;
  email!: string;
}

@query()
async function users(limit: number = 10): Promise<User[]> {
  return config({ sql_source: "v_user", returns_list: true });
}

export_schema("schema.json");
```

**Other languages:** [See language guide](docs/guides/language-generators.md) (16+ supported)

### 2. Compile

```bash
fraiseql-cli compile schema.json -o schema.compiled.json
```

### 3. Configure & Run

```toml
[server]
bind_addr = "0.0.0.0:8080"
database_url = "postgresql://localhost/mydb"

[security.rate_limiting]
enabled = true
auth_start_max_requests = 100
```

```bash
fraiseql-server --config fraiseql.toml --schema schema.compiled.json
```

### 4. Query

```bash
curl -X POST http://localhost:8080/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users(limit: 5) { id name email } }"}'
```

---

## 🗂️ Architecture

![Architecture Pipeline](docs/diagrams/architecture-pipeline.d2)

```
Authoring (Python/TS/Go/...)
    ↓
Schema Definition → Compiler (fraiseql-cli)
    ↓
Compiled Schema (schema.compiled.json)
    ↓
Server (fraiseql-server) + Database
    ↓
GraphQL Queries → SQL Execution → JSON/Arrow Response
```

**Key insight:** Move all optimization from runtime to compile time. Your schema is analyzed once at build, queries execute efficiently without interpretation.

---

## 🔧 Installation

### For Rust Applications

**Using the unified crate (recommended):**
```toml
[dependencies]
fraiseql = { version = "2.0.0-alpha.5", features = ["server"] }
```

**For advanced use cases:**
```toml
[dependencies]
fraiseql-core = "2.0.0-alpha.5"
fraiseql-server = "2.0.0-alpha.5"
fraiseql-observers = "2.0.0-alpha.5"
```

### For Schema Authoring

- **Python:** `pip install fraiseql==2.0.0-alpha.5`
- **TypeScript/Node.js:** `npm install fraiseql@2.0.0-alpha.5`
- **Rust SDK:** See above
- **Other languages:** [16+ SDKs available](docs/guides/language-generators.md)

### Feature Flags

| Feature | Includes | Use Case |
|---------|----------|----------|
| `postgres` (default) | PostgreSQL support | Smallest binary, PostgreSQL only |
| `mysql` | MySQL support | Multi-database projects |
| `sqlite` | SQLite support | Local development, testing |
| `sqlserver` | SQL Server support | Enterprise Windows environments |
| `server` | HTTP GraphQL server | Production deployments |
| `observers` | Reactive business logic | Event-driven architectures |
| `arrow` | Apache Arrow Flight | Analytics tool integration |
| `wire` | Streaming JSON (PostgreSQL) | Large result sets, bounded memory |
| `full` | All features | Maximum capabilities |

---

## 📊 v1 vs v2 Comparison

![Version Matrix](docs/diagrams/version-matrix.d2)

| Feature | v1.9.16 | v2.0.0 |
|---------|---------|--------|
| **Status** | ✅ Stable | 🚀 Beta |
| **Runtime** | FastAPI (Python) | Compiled (Rust) |
| **Databases** | PostgreSQL only | PostgreSQL, MySQL, SQLite, SQL Server |
| **Languages** | Python | 16+ languages |
| **Development** | Hot reload, no build | Compile step, maximum speed |
| **Performance** | 7-10x faster than Django | 20-50x faster than traditional GraphQL |

**Migrating from v1?** Both versions are actively maintained. See [Migration Guide](docs/guides/v1-to-v2-migration.md).

**Using Python?** Both v1 and v2 work great:
- **v1** for rapid iteration, hot reload, existing PostgreSQL projects
- **v2** for compile-time safety, multi-database support, polyglot teams

---

## 📚 Documentation

**Quick Links:**
- 🚀 [Getting Started](docs/guides/getting-started.md) — 5-minute quick start
- 📖 [Complete Documentation](https://docs.fraiseql.dev) — Comprehensive searchable docs
- 🛠️ [Language Generators](docs/guides/language-generators.md) — 16+ language SDKs
- 🏗️ [Architecture Guide](docs/internal/.claude/ARCHITECTURE_PRINCIPLES.md) — System design
- 🔒 [Security Checklist](docs/guides/production-security-checklist.md) — Production hardening
- 🔄 [Migration from v1](docs/guides/v1-to-v2-migration.md) — Upgrade path
- 🎓 [Examples](docs/examples/) — 4+ full-stack applications

**Local Documentation:**
- `docs/internal/.claude/ARCHITECTURE_PRINCIPLES.md` — Architectural patterns
- `docs/alpha-testing-guide.md` — Alpha testing and feedback
- `docs/ALPHA_LIMITATIONS.md` — Known limitations and roadmap

---

## 🤔 Is FraiseQL v2 Right For You?

### ✅ Perfect If You:
- Build high-performance APIs with relational databases
- Need compile-time schema validation
- Want deterministic query execution (no runtime surprises)
- Use multiple programming languages in your stack
- Require enterprise security features
- Are adopting PostgreSQL, MySQL, SQLite, or SQL Server
- Need to migrate from v1 to a more powerful engine

### ❌ Consider Alternatives If:
- You only use a single language (v1 Python might be better)
- You need real-time, approximate answers (not consistent data)
- Building your first GraphQL API (use simpler frameworks first)
- Don't have a relational database (use REST, gRPC, or other)

---

## 🔗 Related Versions

- **FraiseQL v1.9.16** (Python + Rust pipeline) — [fraiseql-python](https://github.com/fraiseql/fraiseql-python)
  - Stable, production-ready, Python-first
  - Best for: Python teams, rapid iteration

- **FraiseQL v2 (this repo)** (Fully compiled, polyglot)
  - Beta, feature-complete, compile-time optimized
  - Best for: Multi-database, polyglot teams, maximum performance

---

## 🛡️ Security

FraiseQL prevents SQL injection through:
- **Parameterized queries** - All values passed as bind parameters, never interpolated
- **Schema validation** - All identifiers validated at compile time
- **Type system** - Rust's type system prevents entire categories of bugs
- **Zero unsafe code** - Forbidden at compile time

Additional features:
- Audit logging for all mutations
- Rate limiting on auth endpoints
- Field-level authorization
- Error sanitization (no implementation details)
- OAuth2/OIDC support
- Configurable via TOML with environment overrides

See [Security Checklist](docs/guides/production-security-checklist.md) for production hardening.

---

## 🧪 Quality & Testing

- **4,773+ tests** passing (unit, integration, E2E, chaos)
- **Zero unsafe code** (forbidden at compile time)
- **All Clippy warnings as errors** (strict linting)
- **Chaos engineering validated** (failure scenario testing)
- **Solo-authored** with comprehensive test coverage

Testing strategy in `crates/fraiseql-core/tests/` and `crates/fraiseql-server/tests/`.

---

## 🤝 Contributing

FraiseQL is an open-source project. See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

Development follows a structured phase-based approach. Current implementation status is tracked in [IMPLEMENTATION_ROADMAP.md](.claude/IMPLEMENTATION_ROADMAP.md).

---

## 📄 License

FraiseQL is dual-licensed under MIT or Apache 2.0. See [LICENSE](LICENSE) for details.

---

## 🚀 Next Steps

1. **[Quickstart Guide](docs/guides/getting-started.md)** — Get up and running in 5 minutes
2. **[Examples](docs/examples/)** — See full-stack applications
3. **[Language SDK](docs/guides/language-generators.md)** — Choose your language
4. **[GitHub Issues](https://github.com/fraiseql/fraiseql/issues)** — Report bugs or request features
5. **[Discussions](https://github.com/fraiseql/fraiseql/discussions)** — Ask questions, share ideas

---

**Built with ❤️ in Rust. Designed for developers.**
