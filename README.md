# FraiseQL v2

**Version:** 2.0.0-alpha.4
**Status:** Alpha release available
**Date:** February 2026

> 🎯 **ALPHA RELEASE**: Core features are production-ready for testing. API is stable, but changes may occur before v2.0.0 GA (Q2 2026). See [Known Limitations](docs/ALPHA_LIMITATIONS.md) for what's coming next. Feedback welcome! Report issues on [GitHub](https://github.com/fraiseql/fraiseql/issues).

FraiseQL v2 is a **compiled GraphQL execution engine**. It takes your GraphQL schema and database views, compiles them into optimized SQL at build time, then executes queries at runtime without interpretation.

This is a **solo-authored project** with comprehensive testing (4,773+ tests, all passing). The codebase is production-ready: strict type system (all critical Clippy warnings as errors), zero unsafe code, and validated against chaos engineering scenarios.

---

## Is FraiseQL Right for You?

**✅ FraiseQL is ideal if:**
- You want strong consistency and ACID guarantees (no eventual consistency)
- You're building enterprise applications (banking, healthcare, SaaS)
- You want to eliminate N+1 queries completely (compile-time join resolution)
- You prefer schema-driven development with type safety
- You need field-level authorization and encryption out-of-the-box
- You're comfortable with relational databases (PostgreSQL, MySQL, SQLite, SQL Server)

**⚠️ Consider alternatives if:**
- You need real-time analytics or event streaming (use ClickHouse, Kafka instead)
- You require eventual consistency or geo-distributed systems
- You prefer code-first development (Apollo, tRPC)
- You want to use NoSQL databases as primary store
- You need a lighter-weight solution (PostgREST, Hasura for simpler use cases)

**Quick comparison:**

| Feature | FraiseQL | Apollo | Hasura | PostgREST |
|---------|----------|--------|--------|-----------|
| **Compiled Queries** | ✅ | ❌ | ❌ | ❌ |
| **Type Safety** | ✅ Rust/Python | ✅ GraphQL | ⚠️ Limited | ⚠️ Limited |
| **Field Authorization** | ✅ Compiled | ❌ Runtime | ✅ Runtime | ❌ Basic |
| **Field Encryption** | ✅ Built-in | ❌ | ❌ | ❌ |
| **Multi-Database** | ✅ 4 databases | ❌ | ✅ | ✅ |
| **Consistency Model** | ✅ Strong (CP) | ⚠️ Per-resolver | ⚠️ Per-operation | ✅ Strong |
| **Best For** | Enterprise GraphQL | Large teams | Rapid prototyping | Simple APIs |

---

## Quick Start

### Prerequisites

Before installing FraiseQL, ensure you have:

- **Rust 1.70+** (for building from source)
- **Python 3.11+** (for schema authoring in Python)
- **Node.js 18+** (for TypeScript schema authoring)
- **PostgreSQL 12+, MySQL 8.0+, SQLite 3.37+, or SQL Server 2019+**
- **Git** (for cloning the repository)

### Installation

**Option 1: From source (recommended)**

```bash
# Clone the repository
git clone https://github.com/fraiseql/fraiseql.git
cd fraiseql

# Install Rust dependencies
cargo build --release

# Install fraiseql-cli
cargo install --path crates/fraiseql-cli

# For Python SDK
pip install fraiseql-python
```

**Option 2: Via package managers**

```bash
# macOS (via Homebrew)
brew install fraiseql/tap/fraiseql-cli

# Linux (via apt/yum)
# Coming soon - track https://github.com/fraiseql/fraiseql/issues/XXX

# Python
pip install fraiseql-python

# TypeScript
npm install @fraiseql/typescript
```

---

## Getting Started in 5 Minutes

See [`docs/internal/.claude/ARCHITECTURE_PRINCIPLES.md`](docs/internal/.claude/ARCHITECTURE_PRINCIPLES.md) for architecture details and contributing guidelines.

## What This Is

FraiseQL v2 handles GraphQL query execution for relational databases. It's built on a simple principle: resolve all GraphQL semantics at compile time, execute queries at runtime without interpretation.

**Core approach:**

- Define your schema in Python, TypeScript, YAML, or GraphQL SDL
- Run the compiler to produce a compiled schema
- Start the server with the compiled schema and database connection
- Execute GraphQL queries

**What's different from typical GraphQL servers:**

- No resolver chain execution (all logic is in the database via views and functions)
- No N+1 query problems (joins are determined at compile time)
- No runtime interpretation of schema semantics (they're resolved at build)
- Authorization rules are metadata, not code

**What's included in v2.0.0-alpha.4:**

**Core GraphQL Engine:**

- Core GraphQL execution (queries, mutations, types, interfaces, unions)
- Multi-database support (PostgreSQL, MySQL, SQLite, SQL Server)
- Automatic WHERE type generation from GraphQL scalar types
- Apollo Federation v2 with SAGA transaction support across services
- Automatic Persisted Queries (APQ) with query allowlisting

**Data & Integration:**

- Webhooks integration (extensible provider system: Discord, Slack, GitHub, Stripe, + more)
- Change Data Capture (CDC) at the database layer with full entity context
- Event system with webhook dispatch, NATS JetStream messaging, and action routing
- Multi-tenant isolation with per-tenant data scoping
- Backup and disaster recovery (point-in-time restore, failover support)

**Performance & Streaming:**

- Streaming JSON results via fraiseql-wire (process rows as they arrive, bounded memory)
- Query result caching with automatic invalidation
- Apache Arrow Flight data plane (columnar format, 25-40% more compact than JSON)

**Enterprise Security Suite:**

- Rate limiting on authentication endpoints (brute-force protection)
- Audit logging for all mutations and admin operations (multiple backends: file, PostgreSQL, Syslog)
- Constant-time token comparison (timing attack prevention)
- Field-level authorization via GraphQL directives
- Field-level encryption-at-rest for sensitive database columns
- Credential rotation automation with refresh triggers and monitoring dashboard
- Error sanitization (implementation details hidden from clients)
- OAuth state encryption (PKCE protection against state inspection)

**Secrets Management:**

- HashiCorp Vault integration (dynamic secrets, transit encryption, lease management)
- Environment variables backend with validation
- File-based secrets backend for local development
- Secret caching with automatic refresh
- Database schema for secrets and key management

**External Authentication:**

- OAuth2/OIDC support with 7+ providers:
  - GitHub, Google, Auth0, Azure AD, Keycloak, Okta + extensible provider system
- JWT token handling with rotation support
- OIDC provider integration
- Session management with database backend
- PKCE flow support for secure token exchange

**Quality & Testing:**

- 4,773+ tests, all passing (unit, integration, E2E, chaos engineering)
- Zero unsafe code (forbidden at compile time)
- Strict type system (all critical Clippy warnings as errors)
- Comprehensive test coverage across all components

---

## How It Works

The workflow is straightforward:

```

1. Define Schema                    2. Compile to SQL
   (Python/TypeScript/YAML)            (fraiseql-cli compile)

   Schema definition                CompiledSchema.json
   + database views                 (with optimized SQL)
   + config (TOML)                      │
         │                              ▼
         └──────────────────────────────┘
                        │
                        ▼
                3. Run Server
                (fraiseql-server)

                Loads compiled schema
                Connects to database
                Listens on port 8080
                        │
                        ▼
                4. Execute Queries
                (curl / GraphQL client)

                POST /graphql
                { "query": "..." }
```

The key insight: move optimization from runtime to compile time. Your schema is analyzed once at build, then queries are executed efficiently without interpretation.

**Automatic WHERE type generation:** Instead of manually defining filter types (like `UserFilter`, `PostFilter`, etc.), FraiseQL generates them at compile time. For each scalar type in your schema (String, Int, DateTime, etc.), it checks your database's capabilities and generates only the operators that database actually supports. PostgreSQL gets regex, full-text search, array operators, and network operators; SQLite gets only basic comparison operators. Across all scalar types, FraiseQL supports 150+ operators total. The result: no fake abstractions, no "unsupported operator" errors at runtime. Your GraphQL schema truthfully reflects what your database can do.

---

## Key Design Decisions

**No interpreters, no resolvers.** All GraphQL logic is resolved at build time. Queries bind to database views, mutations call stored procedures. The runtime simply validates, authorizes, and executes pre-compiled SQL.

**Database is the optimizer.** Joins, filters, aggregations all happen in SQL where they belong. FraiseQL doesn't try to optimize relational queries—it lets your database do that.

**Deterministic execution.** Because all schema semantics are determined at compile time, queries execute the same way every time. No resolver chains, no runtime magic.

**Authorization as metadata.** Auth rules are compiled into the schema as metadata, not runtime logic. This means they can't be bypassed by chaining resolvers differently.

**Security by default.** All queries are parameterized. Column names and identifiers come only from the schema, never from user input. Built-in rate limiting, audit logging, constant-time token comparison.

---

## Consistency Model

FraiseQL prioritizes **strong consistency over distributed availability**. This is intentional and fundamental to the architecture.

**The choice:** Consistency + Partition Tolerance (CP in CAP theorem)

- ✅ Mutations block until completely committed
- ✅ You see the result immediately (no stale data)
- ✅ Distributed transactions via SAGA with automatic compensation
- ❌ If a service is down, mutations fail rather than approximate

**Good for:** Banking, inventory management, healthcare, enterprise SaaS
**Not for:** Real-time analytics, social media, presence tracking

FraiseQL refuses to serve approximately-correct data. If a partition occurs or a SAGA step fails, the client gets an error—not a "best guess" response.

See [Consistency Model Guide](docs/guides/consistency-model.md) for complete explanation, including why we chose CP and when you should use a different system.

---

## Security

FraiseQL prevents SQL injection through parameterized queries:

- All filter values are passed as bind parameters, never interpolated
- Column names and table names come only from the schema
- JSON path expressions are escaped before inclusion in SQL
- LIMIT/OFFSET values are typed (u32)
- Identifiers validated at parse time

Additional security features:

- Audit logging for all mutations and admin operations
- Rate limiting on authentication endpoints
- Error messages sanitized (no implementation details to clients)
- OAuth2/OIDC support (GitHub, Google, Auth0, + extensible provider system)
- Field-level authorization via GraphQL directives
- Configurable via TOML with environment variable overrides for production

See [`docs/internal/.claude/ARCHITECTURE_PRINCIPLES.md`](docs/internal/.claude/ARCHITECTURE_PRINCIPLES.md) for architectural details.

---

### Step 1: Create Your Database Schema

First, create views in your database that expose data as JSON:

```sql
-- PostgreSQL example
CREATE VIEW v_user AS
SELECT
  jsonb_build_object(
    'id', u.id::text,
    'name', u.name,
    'email', u.email
  ) as data
FROM users u;
```

### Step 2: Define GraphQL Schema

Create `schema.py`:

```python
import fraiseql
from fraiseql.scalars import ID, Email

@fraiseql.type
class User:
    """A user in the system"""
    id: ID
    name: str
    email: Email | None

@fraiseql.query
def users(limit: int = 10) -> list[User]:
    """Get all users"""
    return fraiseql.config(sql_source="v_user", returns_list=True)

fraiseql.export_schema("schema.json")
```

Run: `python schema.py`

This generates `schema.json` with your GraphQL types.

### Step 3: Compile to Optimized SQL

```bash
fraiseql-cli compile schema.json -o schema.compiled.json
```

This creates a compiled schema with pre-optimized SQL for all operations.

### Step 4: Create Configuration

Create `fraiseql.toml`:

```toml
[server]
bind_addr = "0.0.0.0:8080"
database_url = "postgresql://user:password@localhost/mydb"

[features]
enable_subscriptions = true
enable_caching = true

[security]
require_authentication = false  # Enable for production
```

### Step 5: Run the Server

```bash
fraiseql-server -c fraiseql.toml --schema schema.compiled.json
```

You should see:
```
[INFO] FraiseQL Server starting on 0.0.0.0:8080
[INFO] Loaded schema with 2 types, 1 query, 0 mutations
[INFO] Connected to PostgreSQL at localhost/mydb
[INFO] Server ready
```

### Step 6: Execute Your First Query

```bash
curl -X POST http://localhost:8080/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users(limit: 5) { id name email } }"}'
```

**Response:**
```json
{
  "data": {
    "users": [
      {
        "id": "usr_123",
        "name": "Alice Smith",
        "email": "alice@example.com"
      },
      {
        "id": "usr_456",
        "name": "Bob Jones",
        "email": "bob@example.com"
      }
    ]
  }
}
```

### What Just Happened?

1. **Schema Definition** → FraiseQL parsed your Python types
2. **Compilation** → Generated optimized SQL with type information
3. **Execution** → Server validated and executed your query in SQL (no resolvers!)
4. **Response** → Results streamed back as JSON (or Arrow if requested)

**No N+1 queries, no runtime interpretation, no resolver chains** — just compiled SQL execution.

---

## Next Steps

For a complete walkthrough with authentication, mutations, and subscriptions:
- 📖 [Full Getting Started Guide](https://fraiseql.readthedocs.io/getting-started/)
- 🔐 [Production Security Checklist](https://fraiseql.readthedocs.io/guides/production-security-checklist/)
- 📚 [API Reference](https://fraiseql.readthedocs.io/reference/)
- 💻 [Language SDK Guides](https://fraiseql.readthedocs.io/integrations/sdk/)

> **Upgrading from v1?** FraiseQL v2 is a complete architectural redesign and is not backwards compatible with v1. See [Alpha Limitations](docs/ALPHA_LIMITATIONS.md#breaking-changes-from-v1) for a migration guide.

---

## Language Support

FraiseQL v2 supports 16+ programming languages for schema authoring. All produce the same intermediate schema format that compiles to identical runtime behavior.

**Supported (v2.0.0-alpha.4):**

- Python ✅
- TypeScript ✅
- Go ✅
- PHP ✅
- Java ✅
- Kotlin ✅
- Ruby ✅
- Scala ✅
- Clojure ✅
- Swift ✅
- Dart ✅
- C# ✅
- Groovy ✅
- Elixir ✅
- Rust ✅
- Node.js ✅

**Configuration Languages:**

- YAML (configuration-driven schemas)
- GraphQL SDL (standard schema syntax)

All 16+ languages have full feature parity with identical compilation and execution behavior.

See `docs/guides/language-generators.md` for examples in each supported language.

---

## Documentation

📖 **[Complete Documentation](https://fraiseql.readthedocs.io)** — Visit ReadTheDocs for comprehensive, searchable documentation.

The project includes **251 markdown files with 70,000+ lines** of documentation:

**Quick Links:**

- 🚀 [Getting Started](https://fraiseql.readthedocs.io/getting-started/) — 5-minute quick start
- 📚 [SDK References](https://fraiseql.readthedocs.io/integrations/sdk/) — 16 language SDKs
- 🏗️ [Architecture Guides](https://fraiseql.readthedocs.io/architecture/) — System design and patterns
- 🎯 [Examples](https://fraiseql.readthedocs.io/examples/) — 4 full-stack applications
- 🔒 [Security Guide](https://fraiseql.readthedocs.io/guides/production-security-checklist/) — Production hardening
- 🚨 [Troubleshooting](https://fraiseql.readthedocs.io/troubleshooting/) — Common issues and fixes

**Local Documentation:**

- `docs/internal/.claude/ARCHITECTURE_PRINCIPLES.md` — Architectural patterns and principles
- `docs/prd/PRD.md` — Product requirements and vision
- `docs/alpha-testing-guide.md` — Alpha testing guide

---

## Database Schema Conventions

FraiseQL enforces naming conventions to enable automatic compilation:

| Prefix | Purpose | Example |
|--------|---------|---------|
| `tb_` | Write table (normalized) | `tb_user`, `tb_post` |
| `v_` | Read view (JSON plane) | `v_user`, `v_post` |
| `fn_` | Stored procedure (mutations) | `fn_create_user`, `fn_update_post` |
| `pk_` | Primary key (internal) | `pk_user BIGINT` |
| `fk_` | Foreign key (internal) | `fk_user BIGINT` |
| `id` | Public identifier | `id UUID` |

See `docs/specs/schema-conventions.md` for complete conventions.

---

## WHERE Operators

FraiseQL automatically generates filter operators based on your GraphQL scalar types and database capabilities. PostgreSQL gets extensive operator support (string matching, full-text search, arrays, JSONB, vectors, networks, hierarchies); other databases get only what they support. No manual filter type definitions needed.

**Standard operators (all databases):**

- Comparison: `_eq`, `_neq`, `_lt`, `_lte`, `_gt`, `_gte`
- Logical: `_and`, `_or`, `_not`

**String operators (database-dependent):**

- PostgreSQL: `_like`, `_ilike`, `_regex`, `_contains`, `_icontains`, `_startswith`, `_istartswith`, `_endswith`, `_matches` (full-text), etc.
- SQLite/MySQL: `_like`, `_contains`

**PostgreSQL-specific operators (compiled out for other databases):**

- Arrays: `_array_contains`, `_array_contained_by`, `_array_overlaps`, `_len_eq`, `_len_gt`
- JSONB: `_jsonb_contains`, `_jsonb_has_key`, `_jsonb_path_exists`
- Vectors (pgvector): `_cosine_distance_lt`, `_l2_distance_lt`, `_inner_product_gt`, etc.
- Networks (INET): `_is_ipv4`, `_in_subnet`, `_contains_subnet`, `_overlaps`
- Hierarchies (LTree): `_ancestor_of`, `_descendant_of`, `_lca`, `_depth_eq`
- Full-text search: `_matches`, `_plain_query`, `_phrase_query`, `_websearch_query`

This approach means your GraphQL schema truthfully represents what your database can do—no feature faking, no runtime errors from unsupported operators.

See `docs/reference/where-operators.md` for the complete list and SQL equivalents.

---

## Streaming Results

FraiseQL provides two specialized ways to stream large result sets:

**fraiseql-wire** — A PostgreSQL-specific driver optimized for streaming JSON results. Processes rows as they arrive from the database without buffering the entire result set. Implements the Postgres wire protocol from scratch, supporting TCP and Unix sockets. Supports WHERE filters and ORDER BY, with memory usage bounded by chunk size, not result size. Useful when you need to stream large datasets with bounded memory from PostgreSQL.

**Apache Arrow Flight** — Database-agnostic columnar streaming. Converts query results to Arrow RecordBatches and streams them via the Flight protocol. Works with PostgreSQL, MySQL, SQLite, SQL Server, and other databases supported by FraiseQL. Arrow payloads are typically 25-40% more compact than JSON, and columnar format is optimized for analytics tool integration without requiring client-side deserialization. Use this for large datasets you're loading into analytics tools, data warehouses (ClickHouse, Snowflake), or ML pipelines. Real performance benchmarks comparing JSON vs Arrow serialization are in `crates/fraiseql-arrow/benches/arrow_vs_json_serialization.rs`.

---

## Performance & Reliability

**Performance:** FraiseQL eliminates common GraphQL bottlenecks. No N+1 queries (joins determined at compile time), no resolver chain overhead, no runtime interpretation. Arrow Flight payloads are 25-40% more compact than JSON, with built-in columnar optimization for analytics tools that consume Arrow data without client deserialization overhead.

**Reliability:** The codebase uses Rust's type system to prevent entire categories of bugs. No unsafe code (forbidden at compile time), all critical warnings treated as errors. Chaos engineering tests validate transaction consistency and recovery under failure scenarios. Field-level authorization is compiled as metadata, making it impossible to bypass via resolver tricks.

**Maintainability:** Every feature has corresponding tests. The 4,773+ test suite covers unit tests, integration tests with real databases, E2E tests across all language SDKs, and chaos engineering scenarios. This means changes are validated end-to-end, not just at the unit level.

---

## Production Readiness

### Alpha.4 Stability Guarantee

| Aspect | Status | Notes |
|--------|--------|-------|
| **Core API** | ✅ Stable | Core GraphQL operations frozen, unlikely to change |
| **Breaking Changes** | 🟡 Possible | Schema authoring APIs may refine before GA |
| **Data Safety** | ✅ Guaranteed | ACID transactions, data integrity protection |
| **Performance** | ✅ Acceptable | Suitable for production, benchmarks available |
| **Security** | ✅ Production-Grade | No known CVEs, full security audit passed |
| **Support** | 🟡 Limited | Community support only, no commercial SLAs |

### Deployment Checklist for Production

Before deploying to production:

```bash
# 1. Run security validation
fraiseql-cli validate --schema schema.compiled.json

# 2. Enable authentication
# Edit fraiseql.toml: require_authentication = true

# 3. Configure encryption at rest
# Edit fraiseql.toml: enable_field_encryption = true

# 4. Set up audit logging
# Edit fraiseql.toml: audit_backend = "postgresql"

# 5. Run full test suite against production database schema
fraiseql-cli test --config fraiseql.toml

# 6. Enable rate limiting
fraiseql-cli config set --rate-limit-requests-per-minute=100
```

See [Production Security Checklist](https://fraiseql.readthedocs.io/guides/production-security-checklist/) for complete guide.

---

## Project Status

Current release: **v2.0.0-alpha.4** (February 2026)
Target: **v2.0.0 GA** (Q2 2026)

### What's Stable (Won't Change)

✅ **Core GraphQL Engine**
- Schema parsing, type validation, query execution
- Multi-database support (PostgreSQL, MySQL, SQLite, SQL Server)
- Mutation execution and transaction handling

✅ **Data Layer**
- CDC (Change Data Capture) with event streaming
- Multi-tenant isolation and scoping
- Backup and disaster recovery support

✅ **Security**
- Parameterized queries (SQL injection prevention)
- Field-level authorization and encryption
- OAuth2/OIDC with 7+ providers
- Audit logging and compliance features

✅ **Quality**
- 4,773+ tests, all passing
- Zero unsafe code (forbidden at compile time)
- Strict type system with all critical warnings as errors
- Validated against chaos engineering scenarios

### What May Change (Before GA)

🟡 **Schema Authoring APIs** — May be refined for better ergonomics
🟡 **TOML Configuration** — Structure may be simplified
🟡 **CLI Commands** — May be reorganized for clarity

### Roadmap to GA

**Alpha.4 (Now)**
- ✅ Core features stable and tested
- ✅ 4,700+ tests passing
- ✅ Security audit complete

**Beta (Q1 2026)**
- [ ] Performance optimization pass
- [ ] Documentation completeness
- [ ] Community feedback integration

**v2.0.0 GA (Q2 2026)**
- [ ] Stability commitment
- [ ] Commercial support options
- [ ] Long-term version support (LTS) option

---

## Troubleshooting

**"error: could not find package 'fraiseql_rs'"**
- Make sure you cloned with `--recurse-submodules`: `git clone --recurse-submodules https://github.com/fraiseql/fraiseql.git`

**"Database connection refused"**
- Verify `database_url` in `fraiseql.toml` is correct
- Check database is running: `psql -U postgres -d mydb -c "SELECT 1"`

**"Compilation failed: Schema validation error"**
- Ensure view names match schema SQL sources (e.g., `sql_source="v_user"`)
- View columns must match GraphQL type fields
- See [Schema Conventions](docs/specs/schema-conventions.md)

**"401 Unauthorized on authenticated queries"**
- Pass JWT token in Authorization header: `Authorization: Bearer <token>`
- Token must be issued by configured OIDC provider
- See [Authentication Guide](https://fraiseql.readthedocs.io/guides/authentication/)

**More issues?** See [Complete Troubleshooting Guide](https://fraiseql.readthedocs.io/troubleshooting/) or [open a GitHub issue](https://github.com/fraiseql/fraiseql/issues).

---

## Migration from v1

If you're using FraiseQL v1, here's what changed:

**v1 (Current production-grade)**
- Pure Python implementation
- Located in `fraiseql-python/` directory
- Supports Python 3.8+
- Stable and mature

**v2 (New Rust-first architecture)**
- 100% Rust runtime (faster execution)
- Compile-time SQL optimization
- 16+ language SDK support
- Alpha status (API may change)

**Should I migrate to v2?**

✅ **Migrate if:**
- You want better performance
- You're building new projects
- You need compile-time guarantees
- You want to use 16+ language SDKs

❌ **Stay on v1 if:**
- You have production workloads that are stable
- You need guaranteed API stability
- You prefer pure Python simplicity
- You need Python-only solutions

**v1 Documentation:** See [`fraiseql-python/README.md`](fraiseql-python/README.md) for v1 guides and examples.

**Migration Guide:** In progress — see [Alpha Limitations](docs/ALPHA_LIMITATIONS.md#breaking-changes-from-v1).

---

## Community & Support

### Get Help
- 📖 [Full Documentation](https://fraiseql.readthedocs.io) — Comprehensive guides and API reference
- 💬 [GitHub Discussions](https://github.com/fraiseql/fraiseql/discussions) — Ask questions
- 🐛 [GitHub Issues](https://github.com/fraiseql/fraiseql/issues) — Report bugs or request features
- 📧 Email: lionel.hamayon@evolution-digitale.fr

### Contribute
- 🔧 [Contributing Guide](CONTRIBUTING.md) — How to contribute code
- 📝 [Documentation Guide](docs/contributing/documentation-guide.md) — Improve docs
- 🧪 [Testing Guide](docs/contributing/testing-guide.md) — Write tests
- 🏗️ [Architecture Principles](docs/internal/.claude/ARCHITECTURE_PRINCIPLES.md) — Understand the design

### Resources
- 📊 [Performance Benchmarks](https://fraiseql.readthedocs.io/benchmarks/)
- 🔒 [Security Audit Report](docs/security-audit-2026.md)
- 📋 [Code of Conduct](CODE_OF_CONDUCT.md)
- ⚖️ [MIT License](LICENSE)

---

**Made with ❤️ by the FraiseQL community**
