# FraiseQL v2 Architecture Principles

**Last Updated**: October 22, 2026
**Architecture**: Layered Optionality with Feature Gates
**Status**: v2.18.0 released (core production-ready; some enterprise features experimental — see notes)

---

## Core Philosophy

FraiseQL v2 achieves four goals simultaneously:

1. **Best DX** - Simple setup, minimal boilerplate for development (production config via TOML), optional features
2. **Best Performance** - Compiled schema, connection pooling, query caching
3. **Most Simple** - Clean core with composable extensions
4. **Most Complete** - Full-featured backend platform

---

## Architectural Pattern: Layered Optionality

```text
┌─────────────────────────────────────────────────────────────┐
│                 Layer 1: Core (Required)                    │
├─────────────────────────────────────────────────────────────┤
│  fraiseql-core/           GraphQL execution engine          │
│  ├── schema/              Compiled schema representation    │
│  ├── runtime/             Executor (adapter-erased)         │
│  ├── graphql/             Query parsing & fragments         │
│  ├── cache/, apq/         Result cache, persisted queries   │
│  └── security/            Field masking, OIDC, RLS context  │
│                                                             │
│  fraiseql-db/             Database layer                    │
│  ├── traits.rs            DatabaseAdapter (+ Writer)        │
│  ├── postgres/            PostgresAdapter (tokio-postgres,  │
│  │                        deadpool-postgres pool)           │
│  └── where_*, order_by,   SQL building blocks used per      │
│      projection_generator request                          │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│              Layer 2: HTTP Server (Required)                │
├─────────────────────────────────────────────────────────────┤
│  fraiseql-server/         HTTP server (axum)                │
│  ├── server/              Server (holds an Arc<Executor>)   │
│  ├── routes/              GraphQL, health, metrics, REST, … │
│  ├── middleware/          Auth, CORS, rate limiting         │
│  └── server_config/       ServerConfig (--config TOML)      │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│           Layer 3: Optional Extensions (Cargo Features)     │
├─────────────────────────────────────────────────────────────┤
│  fraiseql-observers       [feature = "observers"]           │
│                                                              │
│  An event-driven runtime that runs alongside the GraphQL    │
│  server, in the same process.                               │
│                                                              │
│  Capabilities:                                               │
│  ├── Listener: polls core.tb_entity_change_log → conditions │
│  │   → actions; one replica polls, under an advisory lock   │
│  ├── Actions: webhook, email, Slack, cache, database, log   │
│  ├── Delivery: retry with backoff, dead letter queue        │
│  ├── Transports: PostgreSQL (default), NATS, in-memory      │
│  └── Admin HTTP API: DLQ, checkpoints, runtime health       │
│                                                              │
│  Resource budget when enabled:                               │
│  - 1 additional PostgreSQL connection (the advisory lock)   │
│  - Memory: channel_capacity events buffered (default 1000)  │
│             + DLQ entries up to max_dlq_size                │
│                                                              │
│  fraiseql-arrow           [feature = "arrow"]               │
│  └── Arrow Flight for analytics workloads                   │
│                                                              │
│  Other server features: rest, grpc, mcp, federation,        │
│  metrics, webhooks, secrets, functions-runtime, sources,    │
│  inbound, cdc-outbound, … (see features-compatibility-      │
│  matrix.md)                                                 │
│                                                              │
│  fraiseql-wire is a library, not a server feature: a        │
│  streaming PostgreSQL client wrapped by fraiseql-db's       │
│  read-only FraiseWireAdapter [feature = "wire-backend"].   │
│  The server binary does not use it (#1566).                 │
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

**Schema compilation happens before deploy, not at runtime. SQL is built per request.**

```text
Authoring (Python/TS) → Compilation (Rust) → Runtime (Rust)
         ↓                      ↓                    ↓
   schema.json        schema.compiled.json    fraiseql-server (Server)
```

Compiling (`fraiseql-cli compile`) validates the schema, binds each query to the view or
relation it reads (`sql_source`), and embeds the project configuration. With
`--database`, it also checks those relations against the live database and records native
columns, foreign-key join columns and pagination order (`crates/fraiseql-cli/src/commands/compile.rs`).

Compiling does not generate SQL. Each request builds its projection SQL and statement text
from the selection set (`crates/fraiseql-core/src/runtime/executor/runners/query_regular.rs`,
`crates/fraiseql-db/src/postgres/adapter/`) and runs it as a prepared statement (#1557,
#1636).

**Why:**

- Schema errors are found before deploy, not on the first request
- The server loads JSON; it runs no authoring-language code
- Deploy compiled artifacts, not source code

**Never mix authoring with runtime.** Python/TypeScript are authoring languages only.

---

### 2. Trait-Based Adapters

**Every external dependency is abstracted behind a trait.**

```rust,ignore
// Core abstraction
pub trait DatabaseAdapter: Send + Sync {
    async fn execute_where_query(...) -> Result<Vec<JsonbValue>>;
    fn database_type(&self) -> DatabaseType;
    async fn health_check(&self) -> Result<()>;
    fn pool_metrics(&self) -> PoolMetrics;
}

// Implementation (PostgreSQL is the only production adapter;
// test mocks implement the same trait)
impl DatabaseAdapter for PostgresAdapter { ... }
```

**Benefits:**

- ✅ Easy to mock for testing (no external services needed)
- ✅ Clear contracts (trait defines the interface)
- ✅ Generic constructors: `Server::new` takes any `Writer` adapter, `Server::new_read_only`
  any `DatabaseAdapter`. The `Server` type itself names no adapter; it holds an
  adapter-erased `Executor`.

**The same pattern is used for other integrations:**

- `OAuthProvider` trait (Apple, Discord, Facebook, GitHub, Google, generic OIDC)
- `SessionStore` trait (Postgres, in-memory)
- `SignatureVerifier` trait (Stripe, GitHub, Shopify, etc.)

File storage is an enum rather than a trait: `fraiseql_storage::StorageBackend` (local,
S3, GCS, Azure Blob).

---

### 3. Feature-Gated Extensions

**Opt-in complexity via Cargo features.**

An excerpt of `crates/fraiseql-server/Cargo.toml`:

```toml
# Cargo.toml
[features]
default = ["auth", "cli"]

# Optional subsystems
observers = ["dep:fraiseql-observers", "fraiseql-observers/checkpoint"]
arrow = ["dep:fraiseql-arrow", "dep:tonic"]
metrics = ["dep:metrics-exporter-prometheus"]
mcp = ["dep:rmcp"]
federation = ["fraiseql-core/federation"]
```

PostgreSQL is not a feature of the server: `fraiseql-server` always depends on
`fraiseql-core` with `postgres` enabled. The full list is in
[features-compatibility-matrix.md](../features-compatibility-matrix.md).

**Usage:**

```bash
# Minimal server build (no auth, no CLI binary)
cargo build -p fraiseql-server --no-default-features

# With observers + metrics
cargo build -p fraiseql-server --features "observers,metrics"

# Everything
cargo build -p fraiseql-server --all-features
```

**One registration per capability.** Each optional subsystem is one entry of
`fraiseql_server::capability::REGISTRY`. Its metadata (`fraiseql_core::capability::CapabilityMeta`)
names the feature, its config keys and compiled-schema sections, the published artifacts that
ship it, and the ways of serving that start it. Its `Capability` impl
(`crates/fraiseql-server/src/server/capabilities/`) holds the code: the tables it provisions,
the routes it mounts, what it starts, and how it reports its health. Every serve path walks the
same table in the same order:

1. **Provision.** The core steps and each compiled-in capability's steps form one table. Each
   step runs under the boot migration lock, before anything is mounted or started. The
   constructor runs no DDL.
2. **Mount.** The router merges each capability's routes before the admission layer.
3. **Start.** Each configured capability starts if this way of serving is one its metadata
   declares; otherwise `/health` reports it `started: false`.

`fraiseql capabilities` and the generated table in
[features-compatibility-matrix.md](../features-compatibility-matrix.md) print the same
registry.

**Benefits:**

- ✅ Users pay only for what they use (compilation time, binary size)
- ✅ Clear feature boundaries
- ✅ Feature combinations are checked in CI (`.dagger/feature-combos.go`)

---

### 4. Config-Driven Runtime

**All behavior controlled by configuration, not code.**

```rust,no_run
# use std::sync::Arc;
# use fraiseql::{CompiledSchema, db::PostgresAdapter, server::{Server, ServerConfig}};
# async fn example(
#     schema: CompiledSchema,
#     adapter: Arc<PostgresAdapter>,
#     db_pool: Option<fraiseql::server::sqlx::PgPool>,
# ) -> Result<(), Box<dyn std::error::Error>> {
// Construct config — always load from file in production.
// `from_file` reads the TOML only; it applies no env-var layer. The
// `fraiseql-server` binary additionally applies its explicit CLI/env
// overrides (DATABASE_URL, FRAISEQL_BIND_ADDR, … — see `--help`) on top.
let config = ServerConfig::from_file("server.toml")?;

// Start server
let server = Server::new(config, schema, adapter, db_pool).await?;
server.serve().await?;
# Ok(())
# }
```

> **Never use `ServerConfig::default()` in production.** It disables TLS,
> authentication, and rate limiting. It exists for unit tests and local development only.

> **Note on TOML configuration**: the file passed to the `fraiseql-server` binary via
> `--config` is deserialized **directly into `ServerConfig`** — there is no intermediate
> type and no translation step, and the keys are **top-level**, not grouped under
> `[server]`/`[database]` tables. An unknown key refuses to boot with an error naming it.
> (The project `fraiseql.toml` consumed by `fraiseql compile` / `fraiseql run` is a
> different file with a different shape; see the compiled-schema lifecycle doc.)

**Example Configuration:**

```toml
# server.toml — deserialized directly into ServerConfig; keys are top-level.
bind_addr = "0.0.0.0:4000"
schema_path = "schema.compiled.json"

# Database: URL plus pool sizing. In containers, prefer leaving `database_url`
# out of the file and setting the DATABASE_URL environment variable instead
# (the binary's env override wins over the file).
pool_min_size = 5
pool_max_size = 20

# Optional: OIDC authentication
[auth]
issuer = "https://accounts.google.com"
audience = "your-api-audience"   # REQUIRED for security when auth is enabled

# Optional: Observers. `fraiseql.toml [observers] enabled = true` switches the runtime on
# (it is compiled into the schema, #1733); the server config only tunes it. Tuning lives
# under [observers.runtime] (see #342) so the [observers] table keeps the same shape the
# compiler schema uses.
[observers.runtime]
channel_capacity = 1000
max_dlq_size = 10000   # prevent unbounded memory growth on persistent failures
# Persist the triggering event payload into tb_observer_log.request_payload (#468).
# Off by default: the non-sensitive audit columns (action_type, action_index,
# response_status_code, response_payload summary) are always written, but the
# request payload may carry PII so it is opt-in (oversized payloads are truncated).
log_payloads = false

# Optional: event transport tuning (#350). The transport itself is
# `fraiseql.toml [observers] backend` ("postgres", the default, or "nats") and its
# URL `nats_url`, both compiled into the schema. "nats" requires a binary built
# with `--features observers-nats`; a NATS selection that cannot run (feature
# missing or no broker URL) or whose broker is unreachable refuses to boot in
# production rather than silently falling back to PostgreSQL. Override at boot
# with FRAISEQL_OBSERVER_TRANSPORT / FRAISEQL_NATS_*.
[observers.runtime.transport.nats]
subject_prefix = "fraiseql.mutation"  # the default

# Optional: SMTP for the email observer action (#349). Strict block — a typo
# fails the parse. Credentials are env var NAMES, never literals. Without this
# block the email action fails loud rather than silently dropping messages.
[observers.runtime.email]
host         = "smtp.example.com"
port         = 587
from         = "alerts@example.com"
tls          = "start_tls"   # start_tls (default) | tls | none
username_env = "FRAISEQL_SMTP_USERNAME"
password_env = "FRAISEQL_SMTP_PASSWORD"
```

> **Resource note**: enabling `observers` adds one PostgreSQL connection
> (for LISTEN/NOTIFY) and a thread pool. Size your connection pool
> (`pool_max_size`) and container memory accordingly.
>
> Minimum additional resources per instance:
>
> - 1 PostgreSQL connection
> - `channel_capacity` × (average event size) of channel buffer
> - Up to `max_dlq_size` × (average DLQ entry size) for the dead letter queue

**Benefits:**

- ✅ No recompilation for config changes
- ✅ Environment-specific configs (dev, staging, prod)
- ✅ Feature toggles without code changes
- ✅ Infrastructure-as-code compatibility

---

### 5. Arc-Shared State

**Shared state via Arc.**

```rust,ignore
// Create once
let executor = Arc::new(Executor::new(schema, adapter));

// AppState holds the executor behind Arc<ArcSwap<Executor>>, so a schema
// hot-reload can swap it without a lock on the request path.
let app_state = AppState::new(executor);

// Axum shares state across handlers; cloning AppState clones Arc pointers.
Router::new()
    .route(&config.graphql_path, post(graphql_handler))
    .with_state(app_state)
```

**Benefits:**

- ✅ Requests share the schema and executor without copying them
- ✅ No mutex on the read path for read-only data
- ✅ Safe across threads (Arc guarantees)

---

## Entry Point Pattern

### Development setup (5 lines)

For local development and testing:

```rust,no_run
# use std::sync::Arc;
# use fraiseql::{CompiledSchema, db::PostgresAdapter, server::{Server, ServerConfig}};
# async fn example(db_url: String) -> Result<(), Box<dyn std::error::Error>> {
let schema_json = std::fs::read_to_string("schema.compiled.json")?;
let schema = CompiledSchema::from_json(&schema_json, true)?;
let adapter = Arc::new(PostgresAdapter::new(&db_url).await?);
let config = ServerConfig::default();   // sensible dev defaults, no TLS
let server = Server::new(config, schema, adapter, None).await?;
server.serve().await?;
# Ok(())
# }
```

This starts a fully functional GraphQL server on `127.0.0.1:8000` (the `ServerConfig::default()` bind address) suitable for development.

### Production setup

Production deployments additionally require TLS, authentication, rate limiting, connection
pool sizing, and structured logging. TLS, auth, rate limiting and pool sizing are
configured in the `--config` TOML file (top-level `ServerConfig` keys); logging is
configured via environment variables (`RUST_LOG` filter, `FRAISEQL_LOG_FORMAT=json`):

```toml
# server.toml — top-level ServerConfig keys; no [server]/[database] grouping tables.

bind_addr = "0.0.0.0:4000"
schema_path = "schema.compiled.json"
shutdown_timeout_secs = 30   # drain in-flight requests on SIGTERM

# Set DATABASE_URL in the environment rather than writing database_url here.
pool_min_size = 5
pool_max_size = 20

[tls]
enabled = true
cert_path = "/etc/ssl/certs/server.crt"
key_path = "/etc/ssl/private/server.key"

[auth]
issuer = "https://your-auth-provider.example.com"
audience = "your-api-audience"

# Request throttling (token bucket, per IP / per user). The table carries
# `#[serde(default)]` (#874), so naming only the keys you are changing is fine.
# The auth endpoint brute-force limits are separate: they are compiled settings
# ([fraiseql.security.rate_limiting] in the project fraiseql.toml).
#
# ⚠ This whole table is ignored when the compiled schema carries a
# [security.rate_limiting] section — that section wins, and env overrides win over
# it. Set the values there, or override per deployment with FRAISEQL_RATE_LIMIT_*.
[rate_limiting]
enabled = true
rps_per_ip = 100
rps_per_user = 1000
burst_size = 200
cleanup_interval_secs = 60
trust_proxy_headers = false   # true requires trusted_proxy_cidrs in production
trusted_proxy_cidrs = []
max_buckets = 100000          # memory ceiling per tracking map, ~200 bytes a bucket
```

Every transport keys its per-IP limit by the same rule, an embedder-mounted gRPC service
included: `X-Real-IP` and `X-Forwarded-For` count only when `trust_proxy_headers = true`
and, if `trusted_proxy_cidrs` is set, the connection comes from one of them. Otherwise
the bucket is the connecting peer's address, whatever the headers claim.

The Rust entry point remains the same few substantive lines — `ServerConfig::from_file`
loads TLS, auth, rate limiting, and pool sizing from the TOML file. Initialise your own
tracing subscriber first (for JSON logs filtered by `RUST_LOG`, e.g.
`tracing_subscriber::fmt().json().with_env_filter(EnvFilter::from_default_env()).init()`):

```rust,no_run
# use std::sync::Arc;
# use fraiseql::{CompiledSchema, db::PostgresAdapter, server::{Server, ServerConfig, anyhow}};
# async fn example() -> anyhow::Result<()> {
let config = ServerConfig::from_file("server.toml").map_err(anyhow::Error::msg)?;
let schema_json = std::fs::read_to_string(&config.schema_path)?;
let schema = CompiledSchema::from_json(&schema_json, true)?;
let adapter = Arc::new(PostgresAdapter::new(&config.database_url).await?);
let server = Server::new(config, schema, adapter, None).await?;
server.serve().await?;
# Ok(())
# }
```

**Minimum production checklist:**

- [ ] TLS configured (or terminated at load balancer with mTLS inside cluster)
- [ ] Auth provider configured (`[auth]` section of the `--config` file)
- [ ] Rate limiting enabled (`[rate_limiting]` above for request throttling; compiled
      `[fraiseql.security.rate_limiting]` for auth-endpoint brute-force limits)
- [ ] `pool_max_size` sized for expected concurrency
- [ ] `shutdown_timeout_secs` set (default 30 is usually appropriate)
- [ ] Structured logging initialised (`FRAISEQL_LOG_FORMAT=json`, `RUST_LOG=info`) and
      shipped to a log aggregator
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

### 1. Per-Request SQL, Prepared Statements

Compilation binds each query to its view; it does not produce SQL. At request time the
executor builds the projection and `WHERE` for the selection set, with user values as
bind parameters, and the adapter runs it as a prepared statement (#1557, #1636):

```sql
-- Request: query { users(where: { age: { gt: 18 } }) { id name } }
-- SQL built for this request, values bound as parameters, e.g.:
SELECT jsonb_build_object('id', data->'id', 'name', data->'name')
FROM v_user WHERE (data->>'age')::int > $1
```

`deadpool-postgres`'s `prepare_cached` keeps the parsed statement per connection, keyed on
the SQL text (`crates/fraiseql-db/src/postgres/adapter/database.rs`).

### 2. Connection Pooling

```rust,ignore
pub struct PostgresAdapter {
    pool: Pool,  // deadpool-postgres pool, shared by every request
    // ...
}
```

### 3. Query Result Caching

`fraiseql-core/src/cache/` holds a W-TinyLFU result cache (moka) with view-based
invalidation, enabled by `cache_enabled` in `ServerConfig`. Automatic Persisted Queries
(`fraiseql-core/src/apq/`) let clients send a query hash instead of the query text.
`QueryPlanner` keeps a `cache_enabled` flag but caches no plans.

### 4. Introspection Pre-computation

```rust
# use fraiseql::{CompiledSchema, schema::IntrospectionResponses};
# fn example(schema: CompiledSchema) {
// When the Executor is constructed:
let introspection = IntrospectionResponses::build(&schema);

// At runtime: __schema / __type queries are answered from the pre-built response.
# let _ = introspection;
# }
```

---

## Testing Strategy

### Unit Tests

**Pattern:** Mock all external dependencies via traits

```rust,ignore
#[cfg(test)]
struct MockDatabaseAdapter {
    responses: Vec<Vec<JsonbValue>>,
}

// Note: #[async_trait] is currently required for dyn-compatible async traits.
// Native dyn-async-trait with Send is not yet stable in Rust 1.95.0 (MSRV),
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
cargo test -p fraiseql-server --features observers
cargo test -p fraiseql-server --no-default-features
```

---

## Security Model

### 1. Parameterized Queries (SQL Injection Prevention)

Standard queries and mutations use prepared statement parameters exclusively.
User input (variables, WHERE operators, inject values) is never concatenated
into SQL strings:

```rust,ignore
// Standard path — always parameterized
execute("SELECT data FROM v_user WHERE (data->>'id')::bigint = $1", &[user_id])
```

**Aggregate and window queries** (updated in v2.1.0): These paths use
`execute_parameterized_aggregate` with `$N` bind parameters throughout. Column names in `PARTITION BY` / `ORDER BY` are schema-derived and
validated against `WindowAllowlist` before SQL assembly; they are never taken
directly from user input. The guarantee of fully parameterised execution
extends to all query paths.

### 2. Authentication Layers

- OAuth2/OIDC support with these providers (`fraiseql-auth/src/providers/`):
  - Apple, Discord, Facebook, GitHub and Google, and a generic OIDC provider for any other
    issuer
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

- **Field-Level Encryption**: *Not implemented in this build.* The write path is a no-op and the server refuses to boot if any field is marked for encryption. Encrypt sensitive columns at the database or storage layer instead.
- **Secrets Management**: HashiCorp Vault integration for fetching secrets (database password, JWT/OIDC signing secrets, API keys) at startup
  - Lease/TTL and renewal handled by Vault
  - Fallback to environment-variable and file-based backends

### 5. Audit & Compliance

- **Auth Event Logging**: `fraiseql-auth` records authentication events (login attempts, authorization denials)
  - Redacted secrets in logs (implementation details hidden)
  - Note: a generic compliance audit subsystem with file/PostgreSQL/Syslog backends is **not** provided; the in-memory secrets audit logger was removed
- **Error Sanitization**: Hide implementation details from error messages
- **Rate Limiting on Auth Endpoints**: Brute-force protection with configurable thresholds
- **RBAC Management API**: Role-based access control with a built-in REST management API
  - Endpoints: `/api/roles`, `/api/permissions`, `/api/user-roles` and `/api/audit/permissions`
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
  - **Tenant isolation**: roles and assignments carry a `tenant_id`, and the permission
    catalogue is global. The deployment `admin_token` names the tenant on each request. A
    tenant admin token is confined to its own tenant: other tenants' roles and assignments
    answer `404`, and it may read but not change the permission catalogue. See
    [tenant administrators](../auth/tenant-administrators.md).
  - **Assignments grant on the next request**: a role assigned through `/api/user-roles` or
    SCIM group sync is added to the roles a human principal's token carries, on every
    transport, with no new token. `user_id` is the token's `sub`, matched under every
    trusted issuer: with `[[auth.additional_issuers]]`, each issuer must use the same subject
    for the same person. A global assignment applies in every tenant, a tenant's assignment
    only to principals in that tenant (the schema's `tenant_claim`).
    - Service accounts and system jobs keep exactly the roles they declare, and an agent
      acting for a user (`act`) keeps only its token's roles.
    - Assigning, revoking, renaming or deleting a role applies on the next request served by
      the replica that made the change. Other replicas serve a cached answer for up to 30
      seconds.
    - If the lookup fails, the request is answered `503`. It never runs with fewer roles.
    - The role's name is what is granted. Permissions stored on a role are not enforced yet
      ([#1676](https://github.com/fraiseql/fraiseql/issues/1676)).
- **Multi-Tenant Isolation**: Per-tenant data scoping with strict isolation

---

## Migration from Other Systems

### From Hasura

```rust
// Similar pattern: schema → GraphQL execution
// Difference: the schema is compiled and validated ahead of time;
// SQL is still built per request
```

### From PostGraphile

```rust
// Same: Database-centric approach
// Difference: the server reads a compiled schema; it does not introspect the
// database at startup (`fraiseql compile --database` does, before deploy)
```

### From Apollo Server

```rust
// Different: no resolvers; every operation reads a view or calls a function
```

---

## Common Patterns

### Database Backend

PostgreSQL 18+ is the only supported backend: queries, mutations (direct-SQL and
stored-procedure `fn_*`), Relay pagination, JSONB fact tables, and LISTEN/NOTIFY
subscriptions are all available. Non-PostgreSQL adapters were removed in v2.15.0 —
see [Database Compatibility](../database-compatibility.md) for the record.

### Adding Optional Middleware

```rust
# use fraiseql::server::axum;
use axum::{Router, extract::Request, middleware::{self, Next}, response::Response};

// In fraiseql-server/src/middleware/
pub async fn my_middleware(request: Request, next: Next) -> Response {
    // Implementation
    next.run(request).await
}

// Where the router is assembled (fraiseql-server/src/server/routing/), apply it
// conditionally:
fn apply_optional_layers(mut app: Router, my_feature_enabled: bool) -> Router {
    if my_feature_enabled {
        app = app.layer(middleware::from_fn(my_middleware));
    }
    app
}
```

---

## Anti-Patterns (Don't Do This)

### ❌ Don't Mix Authoring and Runtime

```text
// Wrong: Authoring in runtime
fraiseql_core.compile_schema(python_code)  // NO!

// Right: Compile separately
fraiseql-cli compile schema.json  // Build time
Server::new(config, compiled_schema, adapter, db_pool)  // Runtime
```

### ❌ Don't Add Required Dependencies for Optional Features

Wrong:

```toml
# Cargo.toml
[dependencies]
redis = "0.25"  # Required even if not using observers
```

Right:

```toml
# Cargo.toml
[dependencies]
redis = { version = "0.25", optional = true }

[features]
observers = ["redis"]
```

### ❌ Don't Create Tight Coupling

```rust,ignore
// Wrong: Direct dependency
use fraiseql_observers::ObserverExecutor;

// Right: Optional + feature-gated
#[cfg(feature = "observers")]
use fraiseql_observers::ObserverExecutor;
```

---

## Decision Log

### Why Does `Server` Name No Adapter Type?

**Decision:** `Server` is a concrete type. Its constructors are generic over the adapter
(`Server::new<A: Writer>`, `Server::new_read_only<A: DatabaseAdapter>`) and hand it straight
to the `Executor`, which erases it (`crates/fraiseql-server/src/server/mod.rs`).

**Reason:**

- Swapping backends and injecting mock adapters in tests stay possible: they are a choice
  made at construction
- The server and its state types do not carry a type parameter around
- Whether a server can write is decided by the constructor: a backend that is not a
  `Writer` is served read-only, and mutations are refused with a diagnostic

### Why Separate fraiseql-observers Crate?

**Decision:** Keep observers as a separate optional crate compiled into the
binary when enabled.

**What it is:** fraiseql-observers is a full event-driven runtime, not a
lightweight plugin. It includes its own transport layer, condition DSL,
retry and dead-letter handling, and a checkpoint lease so one replica polls at a time. It adds about 14K lines
of non-test Rust and one more PostgreSQL connection (the advisory lock that elects the
polling replica) to any deployment that enables it.

**Reason for separation:**

- Deployments that don't need event-driven behaviour pay zero cost
  (no compile time, no binary size, no runtime resources)
- Dependencies are isolated: Redis and NATS are not pulled in unless the
  feature is enabled
- The observer runtime can be tested and versioned independently
- Future option: deploy as a standalone sidecar if needed

**Tradeoff:** Users choosing `--features observers` are getting a second
runtime embedded in their server process. They should size their pod/container
accordingly and configure `channel_capacity` and `max_dlq_size` for their
workload. The actions of one event run one after another.

### Why Remove RuntimeServer?

**Decision:** Consolidated to a single `Server` implementation

**Reason:**

- RuntimeServer was never used (dead code)
- Maintaining two server implementations confusing
- All features can be achieved with `Server` + config
- Simpler codebase, easier to maintain

---

## Future Directions

### Potential Extensions

- Schema registry integration (Confluent, AWS Glue)
- Configuration management enhancements (hierarchical config, expanded observability)

### NOT Planned

- Resolvers (conflicts with compilation model)
- Runtime schema changes (use compilation pipeline)
- JavaScript/Python plugins (use compiled approach)

---

## Quick Reference

### Key Files

- `fraiseql-core/src/runtime/executor/mod.rs` - Core execution engine
- `fraiseql-db/src/traits.rs` - `DatabaseAdapter` trait
- `fraiseql-db/src/postgres/adapter/mod.rs` - `PostgresAdapter`
- `fraiseql-server/src/server/mod.rs` - HTTP server implementation
- `fraiseql-server/src/server_config/mod.rs` - Configuration schema
- `fraiseql-server/src/main.rs` - Binary entry point

### Key Traits

- `DatabaseAdapter` - Database backend abstraction
- `OAuthProvider` - Authentication provider abstraction
- `StorageBackend` - File storage backend enum (fraiseql-storage)
- `ActionExecutor` - Observer action abstraction (fraiseql-observers)

### Configuration Guide

| Approach | Method | When to Use |
|----------|--------|-------------|
| Defaults | `ServerConfig::default()` | Development, tests |
| TOML file | `ServerConfig::from_file("server.toml")` | Production |
| Env vars | `DATABASE_URL`, `FRAISEQL_BIND_ADDR`, … (the explicit list in `fraiseql-server --help`) | Container deployments |
| Binary | `fraiseql-server --config server.toml` | Managed deployments |

### Key Commands

```bash
# Build minimal server
cargo build -p fraiseql-server --no-default-features

# Build with all features
cargo build --workspace --all-features

# Run server
cargo run -p fraiseql-server -- --config server.toml

# Run tests
cargo test --workspace --all-features

# Check without building
cargo check --workspace --all-features
```

---

## Conclusion

FraiseQL v2's architecture achieves:

✅ **Best DX** through simple entry points and optional features
✅ **Best Performance** through prepared statements, pooling, result caching and shared state
✅ **Most Simple** through clean abstractions and trait-based design
✅ **Most Complete** through comprehensive optional subsystems

The layered optionality pattern allows users to start minimal and grow as needed, while maintaining architectural clarity throughout.

---

**Architecture Status**: Core production-ready (v2.18.0 released)
**Last Updated**: October 22, 2026 (Enterprise features: secrets, auth, RBAC available; field-level at-rest encryption not implemented)
**Lines of Code**: ~750,000 lines of Rust tracked under `crates/`, tests included, fuzz targets excluded (counted October 22, 2026)
**Test Coverage**: 15,000+ tests (unit, async integration, property-based, snapshot)
**Unsafe Code**: No hand-written `unsafe`. The workspace lint is `unsafe_code = "deny"`, not `forbid`, so `fraiseql-functions` can allow it on `wasmtime` bindgen-generated glue

**See also**: [Database Compatibility](../database-compatibility.md) — PostgreSQL is the only supported backend (non-PostgreSQL adapters were removed in v2.15.0).
