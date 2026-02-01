# FraiseQL v2

**Version:** 2.0.0-alpha.1
**Status:** Alpha release available
**Date:** February 2026

FraiseQL v2 is a compiled GraphQL execution engine. It takes your GraphQL schema and database views, compiles them into optimized SQL at build time, then executes queries at runtime without interpretation.

This is a **solo-authored project** with comprehensive testing (2,400+ tests, all passing). The codebase is production-ready: strict type system (all Clippy warnings resolved), zero unsafe code, and validated against chaos engineering scenarios.

See [`.claude/CLAUDE.md`](.claude/CLAUDE.md) for development details, [`.claude/ARCHITECTURE_PRINCIPLES.md`](.claude/ARCHITECTURE_PRINCIPLES.md) for architecture, or [`.phases/README.md`](.phases/README.md) for development history.

---

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

**What's included in v2.0.0-alpha.1:**
- Core GraphQL execution (queries, mutations, types, interfaces, unions)
- Multi-database support (PostgreSQL, MySQL, SQLite, SQL Server)
- Automatic WHERE type generation from GraphQL scalar types
- Apollo Federation v2 with SAGA transaction support across services
- Webhooks integration (11 external providers)
- Streaming JSON results via fraiseql-wire (process rows as they arrive, bounded memory)
- Backup and disaster recovery (point-in-time restore, failover support)
- Multi-tenant isolation with per-tenant data scoping
- Change Data Capture (CDC) at the database layer with full entity context
- Enterprise security (rate limiting, audit logging, constant-time token comparison, field-level authorization, encryption)
- Event system with webhook dispatch, message queues, and 15+ action types
- Automatic Persisted Queries (APQ) with query allowlisting
- Query result caching with automatic invalidation
- Apache Arrow Flight data plane (columnar format, 50x faster than JSON for analytics, 10x better memory)
- 2,400+ tests, all passing

---

## How It Works

The workflow is straightforward:

```
1. Define Schema                    2. Compile to SQL
   (Python/TypeScript/YAML)            (fraiseql-cli compile)

   Schema definition              CompiledSchema.json
   + database views               (with optimized SQL)
   + config (TOML)                    │
         │                            ▼
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

**Automatic WHERE type generation:** FraiseQL generates filter input types at compile time based on GraphQL scalar types. For each scalar type in your schema (String, Int, DateTime, etc.), it checks a capability manifest to see which operators your database supports, then generates filter types with only those operators included. PostgreSQL gets 150+ operators for String type alone (regex, LIKE variants, full-text search); SQLite gets just the basic ones. No manual filter type definitions needed, and no fake abstractions—each database gets a truthful GraphQL schema that matches what it can actually do.

---

## Key Design Decisions

**No interpreters, no resolvers.** All GraphQL logic is resolved at build time. Queries bind to database views, mutations call stored procedures. The runtime simply validates, authorizes, and executes pre-compiled SQL.

**Database is the optimizer.** Joins, filters, aggregations all happen in SQL where they belong. FraiseQL doesn't try to optimize relational queries—it lets your database do that.

**Deterministic execution.** Because all schema semantics are determined at compile time, queries execute the same way every time. No resolver chains, no runtime magic.

**Authorization as metadata.** Auth rules are compiled into the schema as metadata, not runtime logic. This means they can't be bypassed by chaining resolvers differently.

**Security by default.** All queries are parameterized. Column names and identifiers come only from the schema, never from user input. Built-in rate limiting, audit logging, constant-time token comparison.

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
- OAuth2/OIDC support (Auth0, GitHub, Google, Keycloak, Okta, Azure AD)
- Field-level authorization via GraphQL directives
- Configurable via TOML with environment variable overrides for production

See [`.claude/ARCHITECTURE_PRINCIPLES.md`](.claude/ARCHITECTURE_PRINCIPLES.md) for architectural details.

---

## Getting Started

### 1. Define Schema

Create `schema.py`:

```python
from fraiseql import type as fraiseql_type, query as fraiseql_query, schema

@fraiseql_type
class User:
    id: int
    name: str
    email: str | None

@fraiseql_query(sql_source="v_users")
def users(limit: int = 10) -> list[User]:
    pass

schema.export_schema("schema.json")
```

Run: `python schema.py`

### 2. Compile

```bash
fraiseql-cli compile schema.json -o schema.compiled.json
```

### 3. Configure and Run

Create `config.toml`:
```toml
[server]
bind_addr = "0.0.0.0:8080"
database_url = "postgresql://localhost/mydb"
```

Run: `fraiseql-server -c config.toml --schema schema.compiled.json`

### 4. Query

```bash
curl -X POST http://localhost:8080/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users(limit: 5) { id name email } }"}'
```

That's the basic flow. For more examples and language-specific guides, see the documentation.

---

## Language Support

FraiseQL v2 supports 15+ programming languages for schema authoring. All produce the same intermediate schema format that compiles to identical runtime behavior.

**Tier 1 (Production Ready):**
- Python
- TypeScript
- Go
- PHP

**Tier 2 (Ready):**
- Java
- Kotlin
- Ruby
- Scala

**Tier 3 (Ready):**
- Clojure
- Swift
- Dart
- C#
- Groovy
- Elixir
- Rust

Also supports:
- YAML (configuration-driven schemas)
- GraphQL SDL (standard schema syntax)

Pick one canonical language for your organization. It's straightforward to convert between languages if needed.

See `docs/language-generators.md` for examples in each language.

---

## Documentation

The project includes comprehensive documentation:

**Architecture & Design:**
- `.claude/ARCHITECTURE_PRINCIPLES.md` — Architectural patterns and principles
- `docs/prd/PRD.md` — Product requirements and vision
- `docs/architecture/` — Compilation pipeline, execution model, database targeting

**Specifications:**
- `docs/specs/` — Schema conventions, compiled schema format, CDC format
- `docs/reference/` — Scalar types, WHERE operators, complete API reference

**Operations:**
- `docs/guides/production-deployment.md` — Kubernetes setup and hardening
- `docs/guides/monitoring.md` — Prometheus metrics, OpenTelemetry tracing
- `docs/enterprise/` — RBAC, audit logging, key management

**Getting started:**
- `docs/language-generators.md` — Examples for each supported language
- `docs/e2e-testing.md` — Testing setup and CI/CD integration

See `.phases/README.md` for development history and phase completion details.

---

## Database Schema Conventions

FraiseQL enforces naming conventions to enable automatic compilation:

| Prefix | Purpose | Example |
|--------|---------|---------|
| `tb_` | Write table (normalized) | `tb_user`, `tb_post` |
| `v_` | Read view (JSON plane) | `v_user`, `v_post` |
| `fn_` | Stored procedure (mutations) | `fn_create_user`, `fn_update_post` |
| `pk_` | Primary key (internal) | `pk_user INTEGER` |
| `fk_` | Foreign key (internal) | `fk_user INTEGER` |
| `id` | Public identifier | `id UUID` |

See `docs/specs/schema-conventions.md` for complete conventions.

---

## WHERE Operators

FraiseQL automatically generates filter operators based on your GraphQL scalar types. PostgreSQL deployments get 150+ operators across 15 categories; other databases get only what they support. No manual filter type definitions.

**Standard (all databases):**
- Comparison: `_eq`, `_neq`, `_lt`, `_lte`, `_gt`, `_gte`
- Logical: `_and`, `_or`, `_not`

**String (database-dependent):**
- PostgreSQL: `_like`, `_ilike`, `_regex`, `_contains`, `_icontains`, `_startswith`, `_istartswith`, `_endswith`, etc.
- SQLite/MySQL: basic set (`_like`, `_contains`)

**PostgreSQL-specific (compiled out for other databases):**
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

**fraiseql-wire** — A PostgreSQL-specific driver optimized for streaming JSON results. Processes rows as they arrive from the database without buffering the entire result set. Implements the Postgres wire protocol from scratch, supporting TCP and Unix sockets. Supports WHERE filters and ORDER BY, with memory usage bounded by chunk size, not result size. Useful when you need to stream large datasets with low latency and bounded memory from PostgreSQL.

**Apache Arrow Flight** — Database-agnostic columnar streaming. Converts query results to Arrow RecordBatches and streams them via the Flight protocol. Works with PostgreSQL, MySQL, SQLite, SQL Server, and other databases supported by FraiseQL. Provides 50x better throughput than JSON with 10x better memory efficiency. Use this for large datasets you're loading into analytics tools, data warehouses (ClickHouse, Snowflake), or ML pipelines.

---

## Performance & Reliability

**Performance:** FraiseQL eliminates common GraphQL bottlenecks. No N+1 queries (joins determined at compile time), no resolver chain overhead, no runtime interpretation. For typical workloads, this translates to single-digit millisecond latency. Arrow Flight provides 50x better throughput than JSON for analytics workloads with 10x better memory efficiency—useful when you have large result sets.

**Reliability:** The codebase uses Rust's type system to prevent entire categories of bugs. No unsafe code (forbidden at compile time), all warnings treated as errors. Chaos engineering tests validate zero data loss. Field-level authorization is compiled as metadata, making it impossible to bypass via resolver tricks.

**Maintainability:** Every feature has corresponding tests. The 2,400+ test suite covers unit tests, integration tests with real databases, E2E tests across all language SDKs, and chaos engineering scenarios. This means changes are validated end-to-end, not just at the unit level.

---

## Project Status

Current release: **v2.0.0-alpha.1** (all planned features complete)

**Complete:**
- ✅ Core GraphQL engine (schema parsing, type validation, query execution, mutation support)
- ✅ Multi-database support (PostgreSQL, MySQL, SQLite, SQL Server with database-specific optimizations)
- ✅ Schema authoring in 15+ languages with compile-time verification
- ✅ Automatic WHERE type generation from scalar types (150+ operators for PostgreSQL)
- ✅ Compilation pipeline (6-phase build process with full validation)
- ✅ Enterprise security (OAuth2/OIDC, field-level auth, audit logging, rate limiting, KMS integration)
- ✅ Apollo Federation v2 with SAGA transactions across services
- ✅ CDC (Change Data Capture) with database-agnostic event format
- ✅ Streaming query results via fraiseql-wire
- ✅ Apache Arrow Flight columnar data plane
- ✅ Query result caching with automatic invalidation
- ✅ Automatic Persisted Queries (APQ) with query allowlisting
- ✅ Event system with webhooks, message queues, and job dispatch
- ✅ Multi-tenant isolation with per-tenant data scoping
- ✅ Comprehensive test suite (2,400+ tests across all components)
- ✅ Production deployment guides and monitoring setup

**Next steps:**
- Community testing and deployment feedback
- Real-world production validation
- Performance optimization based on usage patterns
- Path to v2.0.0 GA

See `.phases/README.md` for detailed development history and phase completion status.

---

## Contact & Contributions

For bugs, features, or questions:
- GitHub issues
- GitHub discussions
- Review `.claude/CLAUDE.md` for contribution guidelines
