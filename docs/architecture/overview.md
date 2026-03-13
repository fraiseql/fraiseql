# FraiseQL v2 Architecture Principles

**Last Updated**: March 8, 2026
**Architecture**: Layered Optionality with Feature Gates
**Status**: v2.1.0 Production-Ready

---

## Core Philosophy

FraiseQL v2 achieves four goals simultaneously:

1. **Best DX** - Simple setup, minimal boilerplate for development (production config via TOML), optional features
2. **Best Performance** - Compiled schema, connection pooling, query caching
3. **Most Simple** - Clean core with composable extensions
4. **Most Complete** - Full-featured backend platform

---

## Architectural Pattern: Layered Optionality

```
┌─────────────────────────────────────────────────────────────┐
│                 Layer 1: Core (Required)                    │
├─────────────────────────────────────────────────────────────┤
│  fraiseql-core/           Pure GraphQL execution engine     │
│  ├── schema/              Compiled schema representation    │
│  ├── runtime/             Executor<DatabaseAdapter>         │
│  ├── db/                  Multi-database support            │
│  └── graphql/             Query parsing & projection        │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│              Layer 2: HTTP Server (Required)                │
├─────────────────────────────────────────────────────────────┤
│  fraiseql-server/         Generic HTTP wrapper              │
│  ├── Server<A>            Type-generic over DatabaseAdapter │
│  ├── routes/              GraphQL, health, metrics          │
│  ├── middleware/          Auth, CORS, rate limiting         │
│  └── server_config.rs     TOML configuration                │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│           Layer 3: Optional Extensions (Cargo Features)     │
├─────────────────────────────────────────────────────────────┤
│  fraiseql-observers       [feature = "observers"]           │
│                                                              │
│  A parallel event-driven runtime that runs alongside the    │
│  GraphQL server. Enabling this feature adds a second        │
│  execution environment with its own thread pool,            │
│  PostgreSQL LISTEN/NOTIFY connection, and job scheduler.    │
│                                                              │
│  Capabilities:                                               │
│  ├── Event listener: PostgreSQL NOTIFY → conditions → acts  │
│  ├── Actions: webhook, email, SMS, Slack, push, search idx  │
│  ├── Delivery: retry with backoff, dead letter queue, dedup │
│  ├── Transports: in-memory, NATS, PG NOTIFY, MySQL/MSSQL   │
│  ├── Job queue: persistent async jobs (Redis or PG)         │
│  ├── High availability: multi-listener with lease mgmt      │
│  └── CLI: observer management (80+ commands)                │
│                                                              │
│  Resource budget when enabled:                               │
│  - 1 additional PostgreSQL connection (LISTEN/NOTIFY)       │
│  - 1 configurable thread pool (max_concurrency, default 50) │
│  - Memory: channel_capacity events buffered (default 1000)  │
│             + DLQ entries up to max_dlq_size                │
│                                                              │
│  ~30K lines of Rust — effectively a separate service        │
│  compiled into the same binary.                             │
│                                                              │
│  fraiseql-arrow           [feature = "arrow"]               │
│  └── Arrow Flight for analytics workloads                   │
│                                                              │
│  fraiseql-wire            [feature = "wire"]                │
│  └── PostgreSQL wire protocol compatibility                 │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│          Layer 4: Runtime Configuration (TOML)              │
├─────────────────────────────────────────────────────────────┤
│  Opt-in features via fraiseql.toml:                         │
│  ├── auth = Some(OidcConfig)     Enable OIDC               │
│  ├── tls = Some(TlsConfig)       Enable TLS/mTLS           │
│  ├── observers = Some(...)        Enable observers          │
│  └── metrics_enabled = true       Enable Prometheus         │
└─────────────────────────────────────────────────────────────┘
```

---

## Core Principles

### 1. Compilation Boundary

**Schema compilation happens at build time, not runtime.**

```
Authoring (Python/TS) → Compilation (Rust) → Runtime (Rust)
         ↓                      ↓                    ↓
   schema.json        schema.compiled.json    Server<DatabaseAdapter>
```

**Why:**

- Zero runtime parsing overhead
- Schema optimizations applied once
- Type safety guaranteed at compile time
- Deploy compiled artifacts, not source code

**Never mix authoring with runtime.** Python/TypeScript are authoring languages only.

---

### 2. Trait-Based Adapters

**Every external dependency is abstracted behind a trait.**

```rust
// Core abstraction
pub trait DatabaseAdapter: Send + Sync {
    async fn execute_where_query(...) -> Result<Vec<JsonbValue>>;
    fn database_type(&self) -> DatabaseType;
    async fn health_check(&self) -> Result<()>;
    fn pool_metrics(&self) -> PoolMetrics;
}

// Implementations
impl DatabaseAdapter for PostgresAdapter { ... }
impl DatabaseAdapter for MysqlAdapter { ... }
impl DatabaseAdapter for SqliteAdapter { ... }
impl DatabaseAdapter for MssqlAdapter { ... }
```

**Benefits:**

- ✅ Easy to mock for testing (no external services needed)
- ✅ Swappable implementations (Postgres → MySQL without code changes)
- ✅ Clear contracts (trait defines the interface)
- ✅ Type-generic code (`Server<A: DatabaseAdapter>`)

**Pattern extends to all integrations:**

- `OAuthProvider` trait (Google, GitHub, Keycloak, etc.)
- `StorageBackend` trait (S3, GCS, local filesystem)
- `SessionStore` trait (Postgres, Redis, in-memory)
- `SignatureVerifier` trait (Stripe, GitHub, Shopify, etc.)

---

### 3. Feature-Gated Extensions

**Opt-in complexity via Cargo features.**

```toml
[features]
default = []

# Optional subsystems
observers = ["fraiseql-observers", "sqlx"]
arrow = ["fraiseql-arrow", "tonic", "arrow"]
metrics = ["prometheus"]
wire = ["fraiseql-wire", "tokio-postgres"]

# Database backends
postgres = ["sqlx/postgres"]
mysql = ["sqlx/mysql"]
sqlite = ["sqlx/sqlite"]
mssql = ["tiberius"]
```

**Usage:**

```bash
# Minimal build (GraphQL only)
cargo build --no-default-features

# With observers + metrics
cargo build --features "observers,metrics"

# Everything
cargo build --all-features
```

**Benefits:**

- ✅ Users pay only for what they use (compilation time, binary size)
- ✅ Clear feature boundaries
- ✅ Independent testing (feature combinations are composable)

---

### 4. Config-Driven Runtime

**All behavior controlled by configuration, not code.**

```rust
// Construct config — always load from file in production.
// FRAISEQL_DATABASE_URL, FRAISEQL_BIND_ADDR, etc. override any field
let config = ServerConfig::from_file("fraiseql.toml")?;

// Start server
let server = Server::new(config, schema, adapter, db_pool).await?;
server.serve().await?;
```

> **Never use `ServerConfig::default()` in production.** It disables TLS,
> authentication, and rate limiting. It exists for unit tests and local development only.

> **Note on TOML configuration**: `ServerConfig::from_file` loads a `fraiseql.toml`
> file directly. When using the `fraiseql-server` binary, pass `--config fraiseql.toml`;
> the binary loads `RuntimeConfig` (an internal type) and translates it to `ServerConfig`
> before calling `Server::new()`. For library users, construct `ServerConfig` directly
> or via `from_file`.

**Example Configuration:**

```toml
[server]
bind_addr = "0.0.0.0:4000"
schema_path = "schema.compiled.json"

[database]
url_env = "DATABASE_URL"
pool_min_size = 5
pool_max_size = 20

# Optional: OIDC authentication
[auth]
issuer = "https://accounts.google.com"
client_id_env = "GOOGLE_CLIENT_ID"

# Optional: Observers
[observers]
enabled = true
channel_capacity = 1000
max_dlq_size = 10000   # prevent unbounded memory growth on persistent failures
```

> **Resource note**: enabling `observers` adds one PostgreSQL connection
> (for LISTEN/NOTIFY) and a thread pool. Size your connection pool
> (`pool_max_size`) and container memory accordingly.
>
> Minimum additional resources per instance:
> - 1 PostgreSQL connection
> - `max_concurrency` × (average action memory) of working memory
> - `channel_capacity` × (average event size) of channel buffer
> - Up to `max_dlq_size` × (average DLQ entry size) for the dead letter queue

**Benefits:**

- ✅ No recompilation for config changes
- ✅ Environment-specific configs (dev, staging, prod)
- ✅ Feature toggles without code changes
- ✅ Infrastructure-as-code compatibility

---

### 5. Arc-Shared State

**Zero-copy concurrency via Arc.**

```rust
// Create once
let executor = Arc::new(Executor::new(schema, adapter));

// Share across all requests (no cloning)
let app_state = AppState {
    executor: executor.clone(),  // Only clones Arc pointer, not data
    subscription_manager: Arc::new(SubscriptionManager::new(...)),
    // ...
};

// Axum shares state across handlers
Router::new()
    .route("/graphql", post(graphql_handler))
    .with_state(app_state)
```

**Benefits:**

- ✅ High performance (no data copying)
- ✅ Simple concurrency model (no mutexes for read-only data)
- ✅ Safe across threads (Arc guarantees)

---

## Entry Point Pattern

### Development setup (5 lines)

For local development and testing:

```rust
let schema = CompiledSchema::from_file("schema.compiled.json")?;
let adapter = Arc::new(PostgresAdapter::new(&db_url).await?);
let config = ServerConfig::default();   // sensible dev defaults, no TLS
let server = Server::new(config, schema, adapter, None).await?;
server.serve().await?;
```

This starts a fully functional GraphQL server on `127.0.0.1:4000` suitable for development.

### Production setup

Production deployments additionally require TLS, authentication, rate limiting, connection
pool sizing, and structured logging. These are all configured via `fraiseql.toml`:

```toml
# fraiseql.toml

[server]
bind_addr = "0.0.0.0:4000"
schema_path = "schema.compiled.json"
shutdown_timeout_secs = 30   # drain in-flight requests on SIGTERM

[tls]
cert_path = "/etc/ssl/certs/server.crt"
key_path = "/etc/ssl/private/server.key"

[auth]
issuer = "https://your-auth-provider.example.com"
client_id_env = "OIDC_CLIENT_ID"
audience = "your-api-audience"

[security.rate_limiting]
enabled = true
auth_start_max_requests = 100
auth_start_window_secs = 60

[database]
url_env = "DATABASE_URL"
pool_min_size = 5
pool_max_size = 20

[logging]
level = "info"
format = "json"   # structured logs for log aggregation
```

The Rust entry point remains the same 5 substantive lines — `ServerConfig::from_file`
loads TLS, auth, rate limiting, and pool sizing from the TOML file:

```rust
tracing_subscriber::fmt().json().with_env_filter(EnvFilter::from_env("FRAISEQL_LOG")).init();
let config = ServerConfig::from_file("fraiseql.toml")?;
let schema = CompiledSchema::from_file(&config.schema_path)?;
let adapter = Arc::new(PostgresAdapter::new(&config.database.url()?).await?);
let server = Server::new(config, schema, adapter, None).await?;
server.serve().await?;
```

**Minimum production checklist:**

- [ ] TLS configured (or terminated at load balancer with mTLS inside cluster)
- [ ] Auth provider configured (`[auth]` section or `FRAISEQL_AUTH_*` env vars)
- [ ] Rate limiting enabled on auth endpoints
- [ ] `pool_max_size` sized for expected concurrency
- [ ] `shutdown_timeout_secs` set (default 30 is usually appropriate)
- [ ] Structured logging initialised and shipped to a log aggregator
- [ ] Health check endpoints verified with your orchestrator (`/health`, `/readiness`)

---

## Adding New Features

### Core Features (Required)

**Location**: `fraiseql-core/`

**When:**

- GraphQL execution logic
- Database query generation
- Schema compilation
- Core abstractions

**Example:** Adding window functions to GraphQL queries

### HTTP Features (Server-level)

**Location**: `fraiseql-server/src/`

**When:**

- HTTP endpoints
- Middleware
- Request/response handling
- Server configuration

**Example:** Adding a new `/export` endpoint

### Optional Subsystems (New Crate)

**Location**: `fraiseql-<feature>/` + Cargo feature

**When:**

- Large, optional functionality
- External integrations
- Can be disabled without breaking core

**Example:** Adding a notification system

**Steps:**

1. Create `fraiseql-notifications/` crate
2. Add feature flag: `notifications = ["fraiseql-notifications"]`
3. Add optional dependency in `fraiseql-server`
4. Add config section to `ServerConfig`

---

## Performance Patterns

### 1. Compile-Time Optimization

**Schema compiled to SQL templates:**

```rust
// At build time: GraphQL → SQL template
query { users(where: { age: { gt: 18 } }) { id name } }
  ↓
SELECT data FROM v_user WHERE (data->>'age')::int > $1
```

**At runtime:** Just bind parameters, no parsing.

### 2. Connection Pooling

```rust
pub struct PostgresAdapter {
    pool: PgPool,  // Shared connection pool
}

// All requests share the same pool
// No per-request connection overhead
```

### 3. Query Plan Caching

```rust
pub struct QueryPlanner {
    cache_enabled: bool,
    // Caches query AST → execution plan mapping
}

// First query: parse + plan
// Subsequent queries: cached plan
```

### 4. Introspection Pre-computation

```rust
// At server startup:
let introspection = IntrospectionResponses::build(&schema);

// At runtime:
// __schema queries return pre-built response (zero overhead)
```

---

## Testing Strategy

### Unit Tests

**Pattern:** Mock all external dependencies via traits

```rust
#[cfg(test)]
struct MockDatabaseAdapter {
    responses: Vec<Vec<JsonbValue>>,
}

// Note: #[async_trait] is currently required for dyn-compatible async traits.
// Native dyn-async-trait with Send is not yet stable in Rust 1.88 (MSRV),
// and dynosaur is incompatible due to Tokio's Send requirement on futures.
#[async_trait]
impl DatabaseAdapter for MockDatabaseAdapter {
    async fn execute_where_query(...) -> Result<Vec<JsonbValue>> {
        Ok(self.responses.pop().unwrap())
    }
}
```

### Integration Tests

**Pattern:** Test full request/response flows

```rust
#[tokio::test]
async fn test_graphql_query_execution() {
    let server = setup_test_server().await;
    let response = server.graphql(query).await?;
    assert_eq!(response.status(), 200);
}
```

### Feature Tests

**Pattern:** Test feature combinations

```bash
# Test with and without observers
cargo test --features observers
cargo test --no-default-features
```

---

## Security Model

### 1. Parameterized Queries (SQL Injection Prevention)

Standard queries and mutations use prepared statement parameters exclusively.
User input (variables, WHERE operators, inject values) is never concatenated
into SQL strings:

```rust
// Standard path — always parameterized
execute("SELECT data FROM v_user WHERE (data->>'id')::bigint = $1", &[user_id])
```

**Aggregate and window queries** (updated in v2.1.0): These paths use
`execute_parameterized_aggregate` with `$N` / `?` / `@P1` bind parameters
throughout. Column names in `PARTITION BY` / `ORDER BY` are schema-derived and
validated against `WindowAllowlist` before SQL assembly; they are never taken
directly from user input. The guarantee of fully parameterised execution
extends to all query paths.

### 2. Authentication Layers

- OAuth2/OIDC support with 7+ providers:
  - GitHub, Google, Auth0, Azure AD, Keycloak, Okta, extensible provider system
- JWT token validation for sessions with automatic rotation
- Bearer token for metrics endpoints
- TLS/mTLS for transport security
- Constant-time token comparison (prevents timing attacks)
- PKCE flow support for secure authorization code exchange

### 3. Input Validation

- GraphQL query complexity limits
- Request size limits
- Rate limiting (per-IP, per-user, auth endpoints)
- Webhook signature verification

### 4. Data Protection at Rest

- **Field-Level Encryption**: Encrypt sensitive database columns with configurable key rotation
- **Secrets Management**: HashiCorp Vault integration with automatic secret refresh
  - Dynamic secrets with TTL and automatic renewal
  - Transit encryption for sensitive data in transit
  - Lease management and automatic key rotation
  - Fallback to environment variables and file-based backends
- **Credential Rotation**: Automated rotation of authentication credentials
  - Monitor rotation status and refresh triggers
  - Dashboard for rotation history and compliance auditing

### 5. Audit & Compliance

- **Audit Logging**: Track all mutations and admin operations
  - Multiple backends: file, PostgreSQL, Syslog
  - Redacted secrets in logs (implementation details hidden)
  - Structured logging for compliance tooling
- **Error Sanitization**: Hide implementation details from error messages
- **Rate Limiting on Auth Endpoints**: Brute-force protection with configurable thresholds
- **RBAC Management API**: Role-based access control with a built-in REST management API
  - Endpoints: `POST /api/rbac/roles`, `GET /api/rbac/roles`, `POST /api/rbac/permissions`,
    `GET /api/rbac/permissions`, `POST /api/rbac/assignments`, `GET /api/rbac/assignments`
  - **Authentication**: RBAC endpoints are protected by an admin bearer token (`admin_token`
    in `ServerConfig`, or `FRAISEQL_ADMIN_TOKEN` env var). Requests without a valid bearer
    token receive `401 Unauthorized`. This is independent of OIDC configuration.

    > **Important**: RBAC endpoints are only mounted when `admin_token` is set. If the
    > server starts without `admin_token` configured, the RBAC management API is disabled
    > entirely — a startup error is logged and the endpoints are not registered. This means
    > the guard is all-or-nothing: either the endpoints exist and are protected, or they do
    > not exist. For production, always set `admin_token` to a strong random value (≥ 32
    > characters). Consider network-level controls (VPC rules, `allowedCidrs`) as a
    > defence-in-depth layer.
  - **Schema initialization**: `RbacDbBackend::ensure_schema()` is called automatically at
    server startup; no manual migration is required.
  - **Tenant isolation**: All RBAC tables include a `tenant_id` column. Every query is
    scoped to the authenticated tenant; cross-tenant access is blocked at the query level.
    Multi-tenant deployments share a single schema; row-level security enforces isolation.
- **Multi-Tenant Isolation**: Per-tenant data scoping with strict isolation

---

## Migration from Other Systems

### From Hasura

```rust
// Similar pattern: schema → GraphQL execution
// Difference: Compiled ahead of time, not interpreted
```

### From PostGraphile

```rust
// Same: Database-centric approach
// Difference: Multi-database, compiled execution
```

### From Apollo Server

```rust
// Different: No resolvers, fully compiled
// Benefit: 10-100x faster, deterministic behavior
```

---

## Common Patterns

### Database Backend Capability Matrix

| Backend | Queries | Mutations | Relay Pagination |
|---------|---------|-----------|-----------------|
| PostgreSQL | ✅ | ✅ | ✅ |
| MySQL | ✅ | ✅ | ✅ |
| SQL Server | ✅ | ✅ | ✅ |
| SQLite | ✅ | ❌ | ❌ |

Mutation support is gated by the `MutationCapable` marker trait.
`SqliteAdapter` intentionally does not implement `MutationCapable` — attempting a mutation
against `SqliteAdapter` returns `FraiseQLError::Validation` at runtime with a clear
diagnostic message. Use SQLite for read-only development and unit testing.

Adapters that support mutations: `PostgresAdapter`, `MySqlAdapter`, `SqlServerAdapter`,
and `CachedDatabaseAdapter<A>` when `A: MutationCapable`.

> **Note**: true compile-time enforcement would require a separate `execute_mutation()`
> public API method. The current `execute()` entry point accepts raw GraphQL strings and
> determines the operation type at runtime. This is a known limitation tracked in roadmap.md.

### Adding a New Database Backend

1. Implement `DatabaseAdapter` trait
2. Add feature flag
3. Add tests
4. Update docs

```rust
// 1. Implement trait
pub struct MyDatabaseAdapter { ... }

#[async_trait]
impl DatabaseAdapter for MyDatabaseAdapter {
    async fn execute_where_query(...) -> Result<Vec<JsonbValue>> {
        // Your implementation
    }
    // ... other trait methods
}

// 2. Add feature
[features]
mydb = ["mydatabase-driver"]

// 3. Test
cargo test --features mydb
```

### Adding Optional Middleware

```rust
// In fraiseql-server/src/middleware/
pub fn my_middleware<B>() -> Middleware<B> {
    // Implementation
}

// In server.rs, conditionally apply:
if config.my_feature_enabled {
    app = app.layer(my_middleware());
}
```

---

## Anti-Patterns (Don't Do This)

### ❌ Don't Mix Authoring and Runtime

```rust
// Wrong: Authoring in runtime
fraiseql_core.compile_schema(python_code)  // NO!

// Right: Compile separately
fraiseql-cli compile schema.json  // Build time
Server::new(compiled_schema)       // Runtime
```

### ❌ Don't Add Required Dependencies for Optional Features

```toml
# Wrong
[dependencies]
redis = "0.25"  # Required even if not using observers

# Right
[dependencies]
redis = { version = "0.25", optional = true }

[features]
observers = ["redis"]
```

### ❌ Don't Create Tight Coupling

```rust
// Wrong: Direct dependency
use fraiseql_observers::ObserverRuntime;

// Right: Optional + feature-gated
#[cfg(feature = "observers")]
use fraiseql_observers::ObserverRuntime;
```

---

## Decision Log

### Why Generic `Server<A>` Instead of Concrete Types?

**Decision:** Keep `Server<A: DatabaseAdapter>` generic

**Reason:**

- Users can implement custom database adapters
- Testing is easier (mock adapters)
- Type safety enforced at compile time
- No runtime type erasure overhead

### Why Separate fraiseql-observers Crate?

**Decision:** Keep observers as a separate optional crate compiled into the
binary when enabled.

**What it is:** fraiseql-observers is a full event-driven runtime, not a
lightweight plugin. It includes its own transport layer, job queue, condition
DSL, high-availability coordinator, and CLI. It adds approximately 30K lines
of Rust and a second PostgreSQL connection to any deployment that enables it.

**Reason for separation:**

- Deployments that don't need event-driven behaviour pay zero cost
  (no compile time, no binary size, no runtime resources)
- Dependencies are isolated: Redis, NATS, Elasticsearch are not pulled in
  unless the feature is enabled
- The observer runtime can be tested and versioned independently
- Future option: deploy as a standalone sidecar if needed

**Tradeoff:** Users choosing `--features observers` are getting a second
runtime embedded in their server process. They should size their pod/container
accordingly and configure `max_concurrency`, `channel_capacity`, and
`max_dlq_size` for their workload.

### Why Remove RuntimeServer?

**Decision:** Consolidated to single `Server<A>` implementation

**Reason:**

- RuntimeServer was never used (dead code)
- Maintaining two server implementations confusing
- All features can be achieved with `Server<A>` + config
- Simpler codebase, easier to maintain

---

## Future Directions

### Potential Extensions

- Distributed tracing (OpenTelemetry integration for observability)
- APQ over Redis backend (landed in v2.1.0; in-memory and Redis backends available)
- Schema registry integration (Confluent, AWS Glue)
- More database backends (CockroachDB, YugabyteDB)
- Configuration management enhancements (hierarchical config, expanded observability)

### NOT Planned

- Resolvers (conflicts with compilation model)
- Runtime schema changes (use compilation pipeline)
- JavaScript/Python plugins (use compiled approach)

---

## Quick Reference

### Key Files

- `fraiseql-core/src/runtime/executor/mod.rs` - Core execution engine
- `fraiseql-server/src/server.rs` - HTTP server implementation
- `fraiseql-server/src/server_config.rs` - Configuration schema
- `fraiseql-server/src/main.rs` - Binary entry point

### Key Traits

- `DatabaseAdapter` - Database backend abstraction
- `OAuthProvider` - Authentication provider abstraction
- `StorageBackend` - File storage abstraction
- `ActionExecutor` - Observer action abstraction (fraiseql-observers)

### Configuration Guide

| Approach | Method | When to Use |
|----------|--------|-------------|
| Defaults | `ServerConfig::default()` | Development, tests |
| TOML file | `ServerConfig::from_file("fraiseql.toml")` | Production |
| Env vars | `FRAISEQL_DATABASE_URL`, `FRAISEQL_BIND_ADDR`, etc. | Container deployments |
| Binary | `fraiseql-server --config fraiseql.toml` | Managed deployments |

### Key Commands

```bash
# Build minimal
cargo build --no-default-features

# Build with all features
cargo build --all-features

# Run server
cargo run -- --config fraiseql.toml

# Run tests
cargo test --all-features

# Check without building
cargo check --all-features
```

---

## Conclusion

FraiseQL v2's architecture achieves:

✅ **Best DX** through simple entry points and optional features
✅ **Best Performance** through compilation and zero-copy sharing
✅ **Most Simple** through clean abstractions and trait-based design
✅ **Most Complete** through comprehensive optional subsystems

The layered optionality pattern allows users to start minimal and grow as needed, while maintaining architectural clarity throughout.

---

**Architecture Status**: Production-ready (v2.1.0)
**Last Updated**: March 8, 2026 (Enterprise features: encryption, secrets, auth, RBAC complete)
**Lines of Code**: ~350,000 across workspace (hand-written source; excludes generated fuzz corpus and build artefacts)
**Test Coverage**: 15,000+ tests (unit, async integration, property-based, snapshot)
**Unsafe Code**: Zero (forbidden at compile time)
