# FraiseQL v2 Roadmap

**Current Stable**: v2.0.0 (Released March 2026)
**In Development**: v2.1.0-dev (active branch: `dev`)

**Vision**: A compiled GraphQL execution engine delivering zero-cost schema compilation, deterministic SQL generation, and enterprise-grade observability at runtime.

---

## v2.0.0 - Stability and Correctness ✅ Released March 2026

**Released**: March 2026

Beta.2 established a solid foundation across infrastructure, protocol safety, and dependency hygiene. v2.0.0 focuses on removing known limitations and hardening the server for production workloads.

### Completed (Beta.2)
- Docker Compose integration test infrastructure
- Wire protocol decoder hardened against malformed messages
- Environment isolation in tests (temp_env migration)
- Lazy-static to std::sync::LazyLock migration
- Redis crate upgraded 0.25 → 0.28
- Crate extraction (fraiseql-auth, fraiseql-webhooks, fraiseql-secrets)
- SDK consolidation (6 stable SDKs maintained, 10 deprecated)
- Fuzz testing infrastructure and property-based tests
- k6 load testing framework setup
- Operational runbooks and SLA documentation

### Completed (v2.0.0)
- SQL Server and MySQL backends at feature parity for production workloads
- Relay cursor pagination on SQL Server with UUID support and backward pagination
- SQLSTATE error code mapping on SQL Server and MySQL
- PKCE OAuth routes (`/auth/start`, `/auth/callback`) with encrypted state tokens
- Redis backends for PKCE state store and rate limiter (production clustering)
- Per-user rate limiting, proxy-aware IP extraction, `Retry-After` accuracy
- Cookie security hardening (`__Host-` prefix, RFC 6265 quoting, `Max-Age` default)
- OIDC provider error sanitization (no internal details reflected to clients)
- Server-side context injection (`inject={"param": "jwt:<claim>"}`)
- Typed mutation error variants with scalar field population from metadata JSONB
- Federation circuit breaker with per-entity thresholds and half-open recovery
- NATS URL plumbing in ObserversConfig
- Cache key correctness and per-entry TTL overrides
- Cross-SDK parity test suite (5100+ tests across Rust, Python, TypeScript, Go, Java, PHP)
- Full CI integration test infrastructure (Redis, NATS, TLS, Vault, PostgreSQL)

### Exit Criteria
- ✅ Zero known critical bugs in integration test suite
- ✅ All public items documented (`cargo doc --no-deps` zero warnings)
- ✅ MySQL and SQL Server backends pass full test suite
- ✅ Zero clippy warnings in release build
- ✅ Cross-SDK parity verified by golden fixture regression guards

---

## v2.1.0 - Performance and Observability

**Target**: Q3 2026

With stability locked in v2.0.0, v2.1.0 delivers enterprise observability, query optimization, and performance guarantees.

### Completed (in active development on `dev`)
- **Automatic persisted queries (APQ)** - Redis-backed query caching with smart invalidation (`fraiseql-core/src/apq/`)
- **Prometheus metrics** - Query latency percentiles, connection pool health, error rates (`fraiseql-server/src/observability/metrics.rs`)
- **Structured logging** - Request/response logging with correlation IDs (`fraiseql-server/src/observability/logging.rs`)
- **OpenTelemetry tracing** - Distributed tracing integration (`fraiseql-server/src/observability/tracing.rs`)
- **Domain-specific newtypes** - Type-safe schema identifiers (`TypeName`, `FieldName`, `SqlSource`, `RoleName`)
- **64-shard LRU cache** - Reduced lock contention with per-entry TTL and cascade invalidation
- **MySQL RelayDatabaseAdapter** - Keyset cursor pagination with UUID support
- **CheckpointStrategy enum** - `AtLeastOnce` and `EffectivelyOnce` delivery guarantees for observers
- **PHP SDK** - Schema authoring and CLI integration
- **C#, Elixir, F# SDK stubs** - Schema authoring support
- **Federation circuit breaker** - Per-entity-type thresholds with half-open recovery
- **EXPLAIN introspection endpoint** - `POST /api/v1/admin/explain` runs `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` with caller-supplied parameters; returns `{query_name, sql_source, generated_sql, parameters, explain_output}` (`fraiseql-core/src/runtime/executor/explain.rs`)

### Remaining
- **Connection pool auto-tuning** - Adaptive pool sizing based on queue depth and p99 latency
  percentiles; prevents starvation spikes without exhausting `max_connections` on the database
- **Multi-root query pipelining** - Send all root-field SQL statements in a single PostgreSQL
  extended-query pipeline round-trip; eliminates one network RTT per multi-field operation
- **Performance dashboard** - Pre-built Grafana dashboard consuming the existing Prometheus metrics
  endpoint; covers query latency percentiles, pool health, error rates, and cache hit ratio

> **Architectural note**: FraiseQL's single-view-per-type model makes several classic GraphQL
> performance features unnecessary. N+1 queries cannot occur (no runtime relationship traversal).
> Projection optimization is a no-op (PostgreSQL inlines views and fetches only required columns
> from base tables). Statement caching is already provided by the `sqlx` driver layer.
> DataLoader-style batching is irrelevant without N+1.

---

## v2.2.0 - Federation Maturity

**Target**: Q1 2027

Apollo Federation enables distributed GraphQL architectures. v2.2.0 makes FraiseQL a production-grade federation participant.

### Federation Gateway
- **Apollo Federation 2 compatibility** - Full support for subgraph specs, entity resolution, cross-service queries
- **Federated query planning** - Optimize query plans across multiple subgraph databases
- **Entity type resolution** - Correct reference resolution for types owned by different services
- **Field-level authorization** - Enforce ownership and permissions at federation boundaries

### Federation Observability
- **Multi-service tracing** - Trace execution across subgraph boundaries with correct context propagation
- **Federated query analytics** - Show which subgraphs contributed to query results
- **Service health checks** - Automated monitoring of subgraph availability and response times

### Subgraph Developer Experience
- **Federation schema generation** - Python/TypeScript decorators auto-generate @external, @requires
- **Reference entity support** - Simplified patterns for entities referenced across services
- **Easy deployment** - Unified deployment model with gateway as optional configuration

---

## Future (Unprioritized)

Items considered valuable but not scheduled. Prioritized based on customer demand and bandwidth.

### Language SDKs and Bindings
- **JavaScript/Node.js native binding** - Electron/server-side Node performance without runtime FFI
- **Ruby, Go client library** improvements - Expand officially maintained SDK breadth
- **GraphQL schema federation tools** - Schema composition and conflict detection utilities

### Advanced Query Features
- **Live queries** - WebSocket subscriptions with efficient change tracking
- **Query fragments** - Client-side composition for query reusability
- **Directive support** - Custom directives for schema extensions and client-side hints
- **Conditional schema exports** - Feature-flag dependent schema fragments for A/B testing

### Data Pipeline Integration
- **Change Data Capture (CDC)** - Automatic observer triggers on database mutations
- **Event sourcing patterns** - Immutable event log for audit and replay capability
- **Time-travel queries** - Query historical state at specific timestamps
- **Data lineage tracking** - Show provenance of data through transformations and joins

### Advanced Security
- **Row-level security (RLS)** - Database-native RLS with GraphQL-level enforcement
- **Field masking** - Automatic sensitive field redaction by role
- **IP allowlisting and geofencing** - Network-based access controls
- **Secrets rotation automation** - Zero-downtime credential rotation for database connections

### Developer Experience
- **Schema diffing tools** - Show breaking and non-breaking changes in schema versions
- **Migration guides** - Automated documentation for schema evolution
- **Plugin system** - User-defined resolvers and middleware (likely post-v2 major version)
- **Visual schema editor** - Web-based schema authoring for non-developers

---

## Not Planned

Explicitly excluded from roadmap. These require architectural changes or lack strong demand signals.

### Runtime Language Bindings
Language bindings that require runtime FFI (Python native bindings, Ruby native extensions). FraiseQL is compile-time only; Python/TypeScript are authoring languages, not runtime. SDKs provide client-side access.

### Traditional ORM Support
Hibernate, Sequelize, or similar ORMs are fundamentally incompatible with compiled-at-build-time SQL. FraiseQL generates SQL, not bindings.

### NoSQL Databases
MongoDB, Cassandra support requires fundamentally different SQL generation strategies and would dilute database abstraction design. PostgreSQL-first strategy remains.

### GraphQL Streaming / Defer / Stream
Streaming responses and deferred fragment execution add significant complexity for marginal adoption. Focus remains on RPC-style queries.

### Built-in Business Logic Layer
Workflows, state machines, rules engines. FraiseQL executes GraphQL to SQL only. Business logic belongs in application services.

### Automatic API Versioning
Maintaining multiple API versions is an organizational problem, not a technical one. Schema evolution and deprecation directives serve versioning needs better than separate endpoints.

---

## Release Schedule and Process

### Release Cadence
- **Major versions** (v3.0.0, v4.0.0): ~18 months, breaking changes allowed
- **Minor versions** (v2.1.0, v2.2.0): ~4-6 months, new features, backward compatible
- **Patch versions** (v2.0.1, v2.0.2): As needed, bug fixes only, backported to N-1 minor

### Version Support
- **LTS versions**: v1.x (through 2026), v2.x (through 2027)
- **Current stable**: v2.0.0 (released March 2026); v2.1.0-dev active on `dev` branch
- **EOL policy**: Previous major version supported for 12 months after new major release

### Breaking Changes
Breaking changes only in major versions. All minor and patch releases maintain backward compatibility with clear deprecation warnings.

---

## Performance Targets

### Query Execution
- **P50 latency**: < 10ms for simple queries (10-field resolution)
- **P99 latency**: < 100ms for complex queries (50+ fields, joins)
- **Throughput**: > 10,000 requests/sec per instance on 4-core hardware
- **Connection pool**: < 50ms wait time in steady-state under load

### Compilation
- **Schema compilation**: < 5s for 100-type schemas
- **Incremental validation**: < 1s for schema changes

### Memory
- **Baseline server**: < 50MB resident memory (empty schema)
- **Per-schema overhead**: < 1MB per 10 types
- **Connection pool**: < 10MB per 10 connections

---

## Infrastructure and DevOps

### Deployment Models
- **Docker images** - Multi-stage builds, Alpine base, ~15MB compressed size
- **Kubernetes manifests** - Helm charts with sensible defaults for scaling
- **Lambda/Serverless** - Event handler for request-based deployments (via fraiseql-server)
- **Standalone binary** - Static binary with embedded schema, zero external dependencies

### Observability
- **Prometheus metrics** - Standard in v2.1.0 and beyond
- **Structured logging** - JSON output compatible with ELK, Datadog, CloudWatch
- **Health checks** - `/health` endpoint for load balancers and orchestrators
- **SLA dashboards** - Pre-built Grafana dashboards for operations teams

### Support and SLAs
- **Enterprise SLA** - Available for v2.0.0 and later (pending commercial offering)
- **Community support** - GitHub issues, Discord, email (best-effort)
- **Security updates** - Released within 7 days of disclosure to maintainers
