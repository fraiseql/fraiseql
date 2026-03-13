# FraiseQL v2 Roadmap

**Current Stable**: v2.1.0 (Released 2026-03-10)
**In Development**: v2.2.0-dev (active branch: `dev`)

**Vision**: A compiled GraphQL execution engine delivering zero-cost schema compilation, deterministic SQL generation, and enterprise-grade observability at runtime.

---

## v2.0.0 - Stability and Correctness ✅ Released 2026-03-02

**Released**: 2026-03-02

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
- Cross-SDK parity test suite (1,595 tests across Python, TypeScript, Go, Java, PHP, C#, F#, Elixir, Rust SDK)
- Full CI integration test infrastructure (Redis, NATS, TLS, Vault, PostgreSQL)

### Exit Criteria

- ✅ Zero known critical bugs in integration test suite
- ✅ All public items documented (`cargo doc --no-deps` zero warnings)
- ✅ MySQL and SQL Server backends pass full test suite
- ✅ Zero clippy warnings in release build
- ✅ Cross-SDK parity verified by golden fixture regression guards

---

## v2.1.0 - Performance and Observability ✅ Released 2026-03-10

**Released**: 2026-03-10

> **Note**: v2.0.0 and v2.1.0 were developed concurrently on the `dev` branch and
> released within 8 days of each other as a combined stability + observability milestone.
> The brief gap reflects the initial launch phase. Future releases will follow the
> minimum 6-week cadence defined in `releasing.md`.

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
- **C# SDK** (`fraiseql-cli generate csharp`) - Enum, record type, and query generation implemented; integration tests pending
- **Federation circuit breaker** - Per-entity-type thresholds with half-open recovery
- **EXPLAIN introspection endpoint** - `POST /api/v1/admin/explain` runs `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` with caller-supplied parameters; returns `{query_name, sql_source, generated_sql, parameters, explain_output}` (`fraiseql-core/src/runtime/executor/explain.rs`)

- **Pool pressure monitoring** - Emits `fraiseql_pool_tuning_*` Prometheus metrics and scaling
  recommendations; operators act on recommendations by adjusting `max_connections` and restarting.
  Active resizing requires a pool library with `resize()` API (tracked: migrate to `bb8` in v2.2.0 or later)
- **Multi-root query pipelining** - Parallel execution of multi-root GraphQL queries via
  `try_join_all`; `fraiseql_multi_root_queries_total` counter
- **Performance dashboard** - Pre-built 12-panel Grafana 10+ dashboard served from
  `GET /api/v1/admin/grafana-dashboard`; covers latency percentiles, pool health, cache, and errors

> **Architectural note**: FraiseQL's single-view-per-type model makes several classic GraphQL
> performance features unnecessary. N+1 queries cannot occur (no runtime relationship traversal).
> Projection optimization is a no-op (PostgreSQL inlines views and fetches only required columns
> from base tables). Statement caching is already provided by the `sqlx` driver layer.
> DataLoader-style batching is irrelevant without N+1.

### SDK Status

| SDK | Status | Notes |
|-----|--------|-------|
| Python | Functional | Full decorator API (`@fraiseql.type`, `@fraiseql.query`, etc.) |
| TypeScript | Functional | Full decorator API |
| Go | Functional | Struct tag-based generation |
| Java | Functional | Annotation-based generation |
| Kotlin | Functional | Data class generation |
| Scala | Functional | Case class generation |
| Swift | Functional | Struct generation |
| PHP | Functional | Schema authoring and CLI integration |
| Rust | Functional | Derive macro-based generation |
| C# | Functional | Enum, record type, and query generation; integration tests pending |
| Elixir | Not started | No implementation; Mix task skeleton not yet created |
| F# | Not started | No implementation; type providers not designed |

---

## v2.2.0 - Federation Maturity

**Target**: Q1 2027
**Minimum stabilisation**: 6 weeks on `dev` before release cut (earliest: 2026-04-21)

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

## Known Limitations

### Pool Pressure Monitor (recommendation mode only)

`PoolPressureMonitorConfig` (formerly `PoolTuningConfig`, deprecated alias retained)
evaluates connection pool pressure and emits scaling recommendations via Prometheus
metrics and log lines, but **cannot resize the pool at runtime** — `deadpool-postgres`
has no `resize()` API. Operators act on recommendations by adjusting `max_connections`
in `fraiseql.toml` and restarting the server.

**Future work**: migrate to `bb8` (which supports `pool.resize()`) to enable active
pool resizing. Tracked as a v2.2.0 or v2.3.0 milestone.


### `async_trait` migration to native async-fn-in-trait

68 files use `#[async_trait]`, which desugars `async fn` into
`fn(...) -> Pin<Box<dyn Future + Send>>` — a heap allocation per call. This conflicts
with the zero-overhead positioning of FraiseQL on the static-dispatch hot path.

**Why blocked**: Rust's native `dyn async trait` (1.75+) does not propagate `+ Send`
on generated futures. `Arc<dyn DatabaseAdapter + Send + Sync>` used in federation
requires `Future: Send` when spawned via `tokio::spawn`. Until Return Type Notation
(RFC 3425) is stabilised, `async_trait` is the only ergonomic option.

**Tracking**: RFC 3425 — https://github.com/rust-lang/rfcs/pull/3425 |
             Rust issue — https://github.com/rust-lang/rust/issues/109417

**Migration criteria** (all must be true):

1. RTN with `+ Send` bounds is stable on rustc
2. FraiseQL MSRV is updated to that stabilising version
3. Tokio is compatible with native dyn async traits

**When to revisit**: Rust 1.90+ (RTN may have stabilised), or `dynosaur` 0.2+ if it
adds `Send` propagation on generated futures (previous attempt: see MEMORY.md).

**Effort when ready**: Medium — 68 files, mostly mechanical (remove macro from impls,
minor syntax change on trait defs). See migration comment in `fraiseql-db/src/traits.rs`.

### `# Errors` / `# Panics` doc coverage in `fraiseql-core`

`fraiseql-core` has ~300 public fallible functions suppressed under
`#![allow(clippy::missing_errors_doc)]` and `#![allow(clippy::missing_panics_doc)]`.
All public API functions with `Result` returns should document the error variants
they can produce in a `# Errors` section; public functions that can panic should have
`# Panics`.

**Effort**: Large (300+ functions). Recommend a dedicated sprint in v2.2.0.

**Tracking**: CI gate `make lint-gate-errors-doc` counts `# Errors` sections in
`crates/fraiseql-core/src/runtime/` and enforces a minimum floor (currently ≥35,
targeting ≥60 by v2.2.0). The critical execution path (`Executor::execute()`,
`execute_internal()`) is already documented. Remove the crate-level allows once
coverage reaches 100%.

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
- **Minor versions** (v2.1.0, v2.2.0): minimum 6 weeks between releases, new features, backward compatible
- **Patch versions** (v2.0.1, v2.0.2): As needed, bug fixes and improvements, backported to N-1 minor

See `releasing.md` for the full cadence policy.

### Version Support

- **LTS versions**: v1.x (through 2026), v2.x (through 2027)
- **Current stable**: v2.1.0 (released March 2026)
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
