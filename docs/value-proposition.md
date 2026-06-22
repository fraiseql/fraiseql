# FraiseQL Value Proposition

**Status**: v2.8.0 (released) · v2.9.0 in development
**Maturity**: The core engine is production-ready. Some enterprise features are experimental or not yet implemented — see the per-feature notes under [Feature Tiers](#feature-tiers).
**Last Updated**: June 22, 2026

---

## Core Value Proposition

FraiseQL is a compiled GraphQL execution engine that eliminates runtime interpretation overhead by transforming schema definitions into optimized SQL at build time. This compilation boundary enables deterministic, production-grade query execution without the performance penalties of traditional GraphQL resolvers.

As a result, applications built with FraiseQL achieve significantly faster query execution (SQL is pre-compiled at build time), automatic SQL injection protection, and lower runtime parsing overhead—all while maintaining type safety across the entire stack.

---

## What FraiseQL Does Better

### 1. Performance Without Compromise

**FraiseQL**: Compilation phase determines all schema logic. Runtime executes pre-optimized SQL with direct JSONB/Arrow response.

**Hasura/PostGraphile**: Runtime interpretation of GraphQL queries into SQL. Resolvers execute sequentially; N+1 prevention requires careful schema design.

**Advantage**: FraiseQL achieves:

- Faster query execution (SQL pre-compiled at build time; measured speedup varies by workload)
- Automatic elimination of N+1 queries (determined at compile time)
- Bounded memory usage with streaming support for large result sets
- Connection pooling optimized at compile time

---

### 2. True Multi-Database Support

**FraiseQL**: Single compiled schema runs unchanged on PostgreSQL, MySQL, SQLite, or SQL Server. SQL generation is database-agnostic; runtime swaps adapters.

**Hasura**: PostGraphQL focused; migrations complex when switching databases.

**PostGraphile**: PostgreSQL only by design.

**Advantage**: FraiseQL enables:

- Write once, deploy anywhere (same schema compiles for all 4 databases)
- Test locally with SQLite, deploy to PostgreSQL without modification
- No vendor lock-in; migration is configuration-only

---

### 3. Security Built Into Compilation

**FraiseQL**: All queries are parameterized at compile time. Schema security rules are embedded in the compiled artifact. No runtime security decisions.

**Hasura/PostGraphile**: Runtime authentication/authorization checks. Schema introspection creates constant attack surface.

**Advantage**: FraiseQL provides:

- SQL injection is mathematically impossible (parameterized queries generated at compile time)
- Field-level authorization pre-computed; no runtime decision overhead
- Auth event logging (login attempts) via the optional auth layer
- Error messages automatically sanitized (configurable per environment)

---

## What FraiseQL Does NOT Do

FraiseQL is deliberately specialized. These anti-patterns are out of scope:

### Not a Code Generator

FraiseQL does not generate Rust code. Schema authoring generates JSON; the compiler processes JSON. No Rust procedural macros, no code generation. The compiled artifact is data only.

### Not an ORM

FraiseQL is not an object-relational mapper. It handles schema-to-SQL mapping deterministically at compile time. It does not provide runtime query builders, migrations, or model object tracking.

### Not a Database Abstraction Layer

FraiseQL is a query execution engine, not a database abstraction layer. It assumes SQL databases only. Document databases (MongoDB, DynamoDB) are unsupported.

### Not a Query Language Learning Tool

FraiseQL assumes GraphQL knowledge. We do not provide GraphQL tutorials or teach GraphQL fundamentals. Documentation assumes readers understand GraphQL semantics.

### Not Fully Schema-Driven Database Design

FraiseQL maps pre-existing database schemas to GraphQL. It does not derive optimal database schemas from GraphQL definitions. Developers must design their database schema first; FraiseQL generates GraphQL on top.

### Not a Managed Service

FraiseQL is a self-hosted, on-premise execution engine. No managed cloud offering, no vendor-hosted backend. Users operate their own infrastructure.

### Not Backward Compatible Across Major Versions

FraiseQL v2 is not compatible with v1 schemas or SDKs. The v1→v2 migration is non-trivial. This is intentional: v2 simplifies the architecture significantly, and compatibility would prevent those improvements.

---

## Feature Tiers

FraiseQL provides a layered architecture where features are opt-in via Cargo features and configuration. Teams pay complexity cost only for features they use.

### Core Tier (Always Included)

**GraphQL Execution Engine**

- Standard GraphQL operations: queries, mutations, subscriptions
- Type system: objects, interfaces, unions, enums, input types, scalars
- Automatic `WHERE` clause generation (150+ comparison operators)
- Apollo Federation v2 (entity resolution, federated subscriptions)
- Query validation and projection optimization
- Connection pooling and health checks
- Four database backends: PostgreSQL, MySQL, SQLite, SQL Server

**No external dependencies beyond database drivers. Minimal binary footprint (~15MB).**

### Enterprise Tier (Optional Features)

**Security & Access Control**

- Field-level authorization with JWT scope validation
- Rate limiting on auth endpoints with configurable thresholds
- Auth event logging (login attempts) via fraiseql-auth
- Constant-time token comparison (prevents timing attacks)
- OAuth2/OIDC with 7+ pre-built providers (GitHub, Google, Auth0, Azure AD, Keycloak, Okta, custom)
- Multi-tenant isolation with automatic data scoping
- PKCE state encryption for OAuth state parameter protection

**Enabled**: `features = ["auth"]` in Cargo.toml
**Configuration**: `fraiseql.toml` [security] section with environment variable overrides

---

**Query Optimization & Caching**

- Automatic Persisted Queries (APQ) with allowlisting for production safety
- Query result caching with automatic invalidation on mutation
- Schema analysis for join optimization
- N+1 query elimination (compile-time determination)
- Connection pool metrics and monitoring
- Slow query detection and logging

**Enabled**: Query result caching is built into the core engine (no feature flag). Redis-backed cache invalidation is provided by the `observers` feature.
**Configuration**: Runtime TOML configuration

---

**Event Processing & Webhooks**

- Webhook delivery with exponential backoff retry (5 attempts, configurable)
- Action types: webhook, Slack, email, and Redis cache-invalidation (SMS, push, and search are not yet implemented and fail loud)
- Dead-letter queues for failed events
- Event deduplication to prevent duplicates
- Job persistence for durability
- Observability: Prometheus metrics for event processing

**Enabled**: `features = ["observers"]` in Cargo.toml
**Requires**: Redis for job queue and deduplication
**SDK Support**: Python, TypeScript, Java, Go all provide observer decorators

---

### Analytics Tier (Optional)

**Apache Arrow Flight Integration**

- Columnar format for analytical queries (10-100x faster than row-oriented JSON)
- Integration with Arrow-native tools: DuckDB, Polars, Pandas, Apache Spark
- Fact tables with predefined measures and dimensions
- OLAP-style aggregation queries
- Automatic cardinality hints for query planner optimization

**Enabled**: `features = ["arrow"]` in Cargo.toml
**SDK Support**: Python, TypeScript provide @fact_table and @aggregate_query decorators

---

**PostgreSQL Wire Protocol Compatibility**

- Native PostgreSQL wire protocol support for drop-in tool compatibility
- Connect using psql, DBeaver, Tableau, and other PostgreSQL clients without modification
- Streaming results for large datasets
- Automatic JSONB response serialization
- Connection pooling with protocol-level health checks

**Enabled**: `features = ["wire"]` in Cargo.toml
**Supported Clients**: Any PostgreSQL-compatible client library

---

### Integration Tier (Optional)

**Change Data Capture**

- Capture trigger records database write events to a durable change log (per-table opt-in)
- Durable outbox drain worker forwards changes to a message broker (at-least-once)
- NATS JetStream sink for event streaming

**Enabled**: The capture trigger ships with the compiled schema; outbound streaming is provided by the standalone `fraiseql-cdc-sinks` crate (feature `cdc-nats-jetstream`). NATS JetStream is the only sink today, and there is no server auto-mount yet (Kafka/Kinesis/Pulsar sinks and Avro/Protobuf encoding are planned).

---

**Disaster Recovery & Backup**

- Backup and restore procedures for compiled schemas
- Backup provider interface (`BackupProvider` trait) for custom integrations
- Backup encryption and signing

> **Note**: Automated scheduling, cloud storage (S3/GCS) integration, and point-in-time recovery
> are planned for a future release. The current `BackupManager` provides the provider registry
> and execution infrastructure; scheduling must be handled externally (e.g. cron, K8s CronJob).

**Enabled**: Built-in (no feature flag required)
**Documentation**: Runbooks at `docs/runbooks/`

---

## Explicitly Out of Scope

### Not Supported: Databases

- **Oracle Database**: No supported Rust driver. Community drivers exist but lack production maturity.
- **NoSQL Databases** (MongoDB, DynamoDB, Cassandra): Fundamentally incompatible with SQL compilation model.
- **Graph Databases** (Neo4j, ArangoDB): Require different query models; not GraphQL-to-SQL capable.

### Not Supported: GraphQL Features

- **Live Queries / Streaming**: GraphQL spec lacks formal semantics for streaming. Arrow Flight handles analytics; GraphQL subscriptions do not stream arbitrarily.
- **Custom Directives with Runtime Logic**: Directives are compile-time annotations only. Runtime directive execution is not supported (use field resolvers instead).
- **Union Type Lazy Loading**: Unions are resolved at query time; lazy field loading is not supported.

### Not Supported: Language SDKs

**Tier 1 (Officially Supported)**:

- Python
- TypeScript
- Java
- Go

**Tier 2 (Maintained)**:

- PHP
- Rust

**Community (Deprecated)**:

- .NET, Kotlin, Ruby, Elixir, Swift, Dart, C++, R, Julia, Haskell

JVM language users should use the Java SDK with interop libraries (Kotlin, Clojure, etc.).

### Not Supported: Deployment Models

- **Serverless / AWS Lambda**: Compiled schema is stateful (connection pooling). Serverless violates the connection pooling model. Use containerized deployment instead.
- **Managed Service**: FraiseQL is self-hosted only. No SaaS offering.
- **GraphQL-as-a-Service**: No vendor-provided deployment.

---

## Comparison to Alternatives

### FraiseQL vs. Hasura

| Aspect | FraiseQL | Hasura |
|--------|----------|--------|
| **Architecture** | Compiled, statically-analyzed | Runtime interpretation |
| **Performance** | Faster (compiled SQL, workload-dependent) | Baseline (interpreted) |
| **Databases** | PostgreSQL, MySQL, SQLite, SQL Server | Postgres-first, others secondary |
| **Schema Authoring** | 6 languages (Python, TS, Java, Go, PHP, Rust) | Manual YAML/API |
| **N+1 Prevention** | Automatic at compile time | Manual via schema design |
| **Startup Time** | Fast (no parsing) | Fast (no interpretation) |
| **Cost** | Free, open-source, self-hosted | Free open-source + commercial cloud |
| **Learning Curve** | Requires understanding compilation phase | Lower for pure GraphQL users |

**Best for Hasura**: Users prioritizing ease of setup and GraphQL-first development. Fast prototyping.

**Best for FraiseQL**: Users requiring predictable performance, multi-database support, and security-first defaults.

---

### FraiseQL vs. PostGraphile

| Aspect | FraiseQL | PostGraphile |
|--------|----------|--------------|
| **Database** | PostgreSQL, MySQL, SQLite, SQL Server | PostgreSQL only |
| **Performance** | Faster (compiled SQL, workload-dependent) | Baseline (interpreted) |
| **Build Step** | Required (schema → compiled JSON) | No build step |
| **Type Safety** | Compile-time guarantee | Runtime reflection |
| **Extensibility** | Trait-based adapters | PostgreSQL plugins |
| **Learning Curve** | Requires compilation concept | Lower (no build step) |
| **Schema Authoring** | 6 language SDKs | SQL comments + decorators |

**Best for PostGraphile**: PostgreSQL-only shops, rapid development, minimal infrastructure.

**Best for FraiseQL**: Multi-database deployments, performance-critical applications, security-first architectures.

---

### FraiseQL vs. Build-Your-Own GraphQL

| Aspect | FraiseQL | Custom Implementation |
|--------|----------|----------------------|
| **Setup Time** | Hours (define schema, compile, deploy) | Weeks (build resolvers, connect DB, test) |
| **Security** | Built-in (parameterized queries, auth, audit) | Manual implementation per feature |
| **N+1 Prevention** | Automatic | Manual optimization per resolver |
| **Type Safety** | End-to-end (schema → SQL → response) | Language-dependent (Python dynamic, Rust static) |
| **Maintenance** | FraiseQL team owns execution engine | In-house team owns everything |
| **Learning Curve** | Moderate (GraphQL + compilation model) | Steep (GraphQL semantics + custom code) |

**Best for Custom**: Projects requiring deep customization of resolvers or non-standard data flows.

**Best for FraiseQL**: Projects valuing time-to-value, security defaults, and operational simplicity.

---

## When FraiseQL Is the Right Choice

**Choose FraiseQL if your application requires:**

- Multi-database support with schema consistency
- Predictable sub-100ms query latency
- Automatic N+1 query prevention
- Field-level authorization without runtime overhead
- SQL injection protection guaranteed at compile time
- Auth event logging (login attempts)
- Type-safe authoring in Python, TypeScript, Java, or Go
- Self-hosted, full control over infrastructure

---

## When FraiseQL Is the Wrong Choice

**Do not choose FraiseQL if you need:**

- Rapid prototype-to-production with zero setup time (use Hasura)
- PostgreSQL only, willing to trade setup time for extreme simplicity (use PostGraphile)
- Arbitrary query builder flexibility at runtime (use SQLAlchemy + custom GraphQL)
- NoSQL databases (MongoDB, DynamoDB, etc.)
- WebSocket subscriptions out-of-the-box (implement separately)
- Managed service without infrastructure responsibility
- GraphQL federation with arbitrary data sources (Apollo Federation is simpler)

---

## Technical Differentiation

### Compilation Boundary

FraiseQL's core differentiation is the **compilation boundary**:

```
Input:        schema.json (type definitions) + fraiseql.toml (configuration)
              ↓
Compiler:     fraiseql-cli validates, optimizes, generates SQL templates
              ↓
Output:       schema.compiled.json (sealed, immutable schema artifact)
              ↓
Runtime:      Server<DatabaseAdapter> loads compiled schema, executes queries
              Zero parsing, zero interpretation, zero schema changes
```

This boundary enables:

- Type safety at compile time
- Security rules embedded in artifact
- Performance optimization once (not per-query)
- Deterministic behavior (reproducible across executions)

### Trait-Based Architecture

Every integration (databases, authentication, storage) is abstraction behind a trait:

```rust
pub trait DatabaseAdapter: Send + Sync {
    async fn execute_query(...) -> Result<JsonbValue>;
    fn database_type(&self) -> DatabaseType;
}

impl DatabaseAdapter for PostgresAdapter { ... }
impl DatabaseAdapter for MysqlAdapter { ... }
impl DatabaseAdapter for SqliteAdapter { ... }
impl DatabaseAdapter for MssqlAdapter { ... }
```

Benefits:

- Easy mocking for testing
- Runtime swapping of implementations
- Type-safe integration of new features
- Clean dependency injection

### Feature-Gated Extensions

Non-essential features are opt-in via Cargo features:

```toml
[dependencies]
fraiseql = { version = "2.0", features = ["auth", "observers", "arrow"] }
```

This enables:

- Minimal binary for basic deployments (~15MB)
- Full-featured deployments for enterprise (add 20-50MB per extension)
- No hidden dependencies; users know exactly what they're including

---

## Maturity & Production Readiness

**FraiseQL** (v2.8.0 released; v2.9.0 in development) ships a production-ready core:

- Comprehensive test coverage across unit, integration, and E2E scenarios
- Zero unsafe code blocks (`unsafe_code = "forbid"` workspace-wide)
- All Clippy `pedantic` warnings treated as errors
- Comprehensive security audit (SECURITY.md with documented risk assessment)
- Production documentation with 15 operational runbooks and disaster recovery procedures
- Enterprise SLA documentation with uptime targets and incident response procedures

API surface is stable under semantic versioning; breaking changes are reserved for major releases.

---

## Roadmap & Future Direction

See [`roadmap.md`](../roadmap.md) for the authoritative roadmap. Highlights of recently shipped work:

- GraphQL subscriptions over WebSocket (`graphql-ws` / `graphql-transport-ws`) — shipped in `fraiseql-server`
- REST transport with auto-generated OpenAPI 3.0 spec — shipped (`features = ["rest"]`)
- Apollo Federation v2 (full directive set + federated subscriptions) — shipped in v2.2.0
- Error taxonomy consolidation, ArcSwap observer reload, strict rate-limit insert guard — shipped in v2.3.0
- C# and JavaScript/Node.js SDKs — functional

### Explicitly Not Planned

- NoSQL database support (fundamentally incompatible with SQL compilation model)
- Serverless / AWS Lambda support (incompatible with persistent connection pooling)
- Managed cloud service (FraiseQL is self-hosted only)

---

## Getting Started

**For new projects**:

1. Choose an SDK language (Python, TypeScript, Java, Go)
2. Define schema using language decorators
3. Run `fraiseql compile schema.json`
4. Deploy compiled schema with `fraiseql-server`

**For Hasura/PostGraphile migrations**:

- Schema structure translates with ~80% equivalence
- Requires reauthoring in chosen SDK language
- No runtime code changes needed (wire format is compatible)
- Typical migration: 1-3 weeks for production schema

---

## Support & Community

- **Documentation**: `/docs` directory with architecture, runbooks, and API reference
- **Examples**: `/examples` directory with working implementations
- **Testing**: Reproducible test cases demonstrate all features
- **Security**: Vulnerability reporting via SECURITY.md
- **Stability**: Semantic versioning with 2+ years of backward compatibility guarantee

---

## License

FraiseQL is dual-licensed: MIT OR Apache 2.0. Use freely in commercial and open-source projects.
