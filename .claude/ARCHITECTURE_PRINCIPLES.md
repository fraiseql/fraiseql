# FraiseQL v2 Architecture Principles

**Last Updated**: February 5, 2026
**Architecture**: Layered Optionality with Feature Gates
**Status**: v2.0.0-alpha.1 Production-Ready

---

## Core Philosophy

FraiseQL v2 achieves four goals simultaneously:

1. **Best DX** - Simple setup, zero boilerplate, optional features
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
│  ├── Event system with retry/DLQ                            │
│  ├── Actions: email, SMS, webhook, Slack                    │
│  ├── Job queues, caching, deduplication                     │
│  └── Search integration (Elasticsearch)                     │
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
// Load config from file or environment
let config = ServerConfig::from_file("fraiseql.toml")?;

// Start server with config
let server = Server::new(config, schema, adapter, db_pool).await?;
server.serve().await?;
```

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
```

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

**Simple 5-line setup:**

```rust
// 1. Load compiled schema
let schema = CompiledSchema::from_file("schema.compiled.json")?;

// 2. Create database adapter
let adapter = Arc::new(PostgresAdapter::new(&db_url).await?);

// 3. Load configuration
let config = ServerConfig::from_file("fraiseql.toml")?;

// 4. Create server
let server = Server::new(config, schema, adapter, None).await?;

// 5. Start serving
server.serve().await?;
```

**That's it.** This is a production-ready GraphQL server.

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
All queries use parameters, never string concatenation:
```rust
// Safe
execute("SELECT * FROM users WHERE id = $1", &[user_id])

// Never this
execute(&format!("SELECT * FROM users WHERE id = {}", user_id))
```

### 2. Authentication Layers

- OAuth2/OIDC support with 7+ providers:
  * GitHub, Google, Auth0, Azure AD, Keycloak, Okta, extensible provider system
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
  * Dynamic secrets with TTL and automatic renewal
  * Transit encryption for sensitive data in transit
  * Lease management and automatic key rotation
  * Fallback to environment variables and file-based backends
- **Credential Rotation**: Automated rotation of authentication credentials
  * Monitor rotation status and refresh triggers
  * Dashboard for rotation history and compliance auditing

### 5. Audit & Compliance

- **Audit Logging**: Track all mutations and admin operations
  * Multiple backends: file, PostgreSQL, Syslog
  * Redacted secrets in logs (implementation details hidden)
  * Structured logging for compliance tooling
- **Error Sanitization**: Hide implementation details from error messages
- **Rate Limiting on Auth Endpoints**: Brute-force protection with configurable thresholds
- **RBAC Database Schema**: Role-based access control with permission system
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

**Decision:** Keep observers as separate optional crate

**Reason:**
- Large feature (9K LOC)
- Many dependencies (Redis, NATS, Elasticsearch)
- Can be disabled entirely for minimal deployments
- Independent testing and versioning

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
- APQ over Redis backend (currently in-memory only)
- Schema registry integration (Confluent, AWS Glue)
- More database backends (CockroachDB, YugabyteDB)
- Configuration management enhancements (v2.1 planning: hierarchical config, observability, finalization)

### NOT Planned

- Resolvers (conflicts with compilation model)
- Runtime schema changes (use compilation pipeline)
- JavaScript/Python plugins (use compiled approach)

---

## Quick Reference

### Key Files

- `fraiseql-core/src/runtime/executor.rs` - Core execution engine
- `fraiseql-server/src/server.rs` - HTTP server implementation
- `fraiseql-server/src/server_config.rs` - Configuration schema
- `fraiseql-server/src/main.rs` - Binary entry point

### Key Traits

- `DatabaseAdapter` - Database backend abstraction
- `OAuthProvider` - Authentication provider abstraction
- `StorageBackend` - File storage abstraction
- `ActionExecutor` - Observer action abstraction (fraiseql-observers)

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

**Architecture Status**: Production-ready (v2.0.0-alpha.1)
**Last Updated**: February 5, 2026 (Enterprise features: encryption, secrets, auth, RBAC complete)
**Lines of Code**: ~180,000 across workspace
**Test Coverage**: 2,400+ tests (unit, integration, E2E, chaos engineering)
**Unsafe Code**: Zero (forbidden at compile time)
