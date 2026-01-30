# Endpoint Runtime Architecture Update

**Date**: January 30, 2026
**Status**: Implemented with Architectural Evolution

---

## Important Notice

The endpoint runtime documentation in this directory describes **historical planning** for the "Endpoint Runtime" feature set (webhooks, files, auth, observers, etc.).

**The actual implementation evolved differently than originally planned.**

---

## What Changed

### Original Plan (from these docs)
- Separate `RuntimeServer` implementation
- Separate `fraiseql-runtime`, `fraiseql-webhooks`, `fraiseql-files` crates
- Phase-based implementation tracking

### Actual Implementation (production)
- **Single unified `Server<A: DatabaseAdapter>`** generic implementation
- **All features integrated into `fraiseql-server`** crate
- **Feature-based architecture** with Cargo feature flags
- **fraiseql-observers** as separate optional crate

---

## Current Architecture

### Layered Optionality

```
Layer 1: Core (Required)
└── fraiseql-core/              # GraphQL execution engine

Layer 2: Server (Required)
└── fraiseql-server/            # Generic Server<DatabaseAdapter>
    ├── routes/                 # GraphQL, health, metrics
    ├── middleware/             # Auth, CORS, rate limiting
    ├── webhooks/               # Webhook handling
    ├── files/                  # File uploads
    └── auth/                   # OIDC authentication

Layer 3: Extensions (Optional via Cargo features)
├── fraiseql-observers/         # Event system [feature = "observers"]
├── fraiseql-arrow/             # Analytics [feature = "arrow"]
└── fraiseql-wire/              # Wire protocol [feature = "wire"]
```

### Key Implementation Details

**Server Structure**:
```rust
pub struct Server<A: DatabaseAdapter> {
    config: ServerConfig,
    executor: Arc<Executor<A>>,
    subscription_manager: Arc<SubscriptionManager>,
    oidc_validator: Option<Arc<OidcValidator>>,

    #[cfg(feature = "observers")]
    observer_runtime: Option<Arc<RwLock<ObserverRuntime>>>,

    #[cfg(feature = "arrow")]
    flight_service: Option<FraiseQLFlightService>,
}
```

**Entry Point** (from `fraiseql-server/src/main.rs`):
```rust
// Load compiled schema
let schema = CompiledSchema::from_file(&config.schema_path)?;

// Create database adapter
let adapter = Arc::new(PostgresAdapter::with_pool_config(...).await?);

// Create server
let server = Server::new(config, schema, adapter, db_pool).await?;

// Start serving
server.serve().await?;
```

---

## Feature Status

All features described in the endpoint-runtime docs are **implemented and production-ready**:

### ✅ Implemented in fraiseql-server
- **Webhooks** (Phase 3) - 15+ signature verification schemes
- **File Uploads** (Phase 4) - S3, local storage, image processing
- **Authentication** (Phase 5) - OIDC, OAuth 2.0, JWT sessions
- **Rate Limiting** (Phase 2) - Sliding window, memory & Redis backends
- **CORS** (Phase 2) - Wildcard patterns, preflight handling
- **Metrics** (Phase 2) - Prometheus format
- **Health Checks** (Phase 1) - Liveness, readiness
- **Graceful Shutdown** (Phase 1) - Request draining, timeouts

### ✅ Implemented in fraiseql-observers (optional feature)
- **Observers** (Phase 6) - Database event reactions
- **Actions** - Email, SMS, Slack, webhook, push notifications
- **Retry Logic** - Exponential backoff, dead letter queue
- **Job Queues** (Phase 8C) - Redis-backed queuing
- **Caching** (Phase 8B) - Query result caching
- **Search** (Phase 8A) - Elasticsearch integration

### 📋 Planned but Not Yet Implemented
- **Interceptors** (Phase 9) - WASM plugins
- **Additional Notifications** (Phase 7) - More provider integrations

---

## Why the Architecture Changed

### Original Design Issues
1. **RuntimeServer** was a duplicate implementation alongside `Server<A>`
2. Maintaining two server implementations was confusing
3. Separate crates created unnecessary complexity
4. Phase-based tracking didn't match actual development flow

### Benefits of Current Architecture
1. ✅ **Single clear entry point** - `Server<A>` is the only server
2. ✅ **Type-generic design** - Works with any `DatabaseAdapter`
3. ✅ **Optional features** - Pay only for what you use via Cargo features
4. ✅ **Simple configuration** - TOML-based with environment variables
5. ✅ **Production-tested** - 294+ tests passing

---

## Documentation Mapping

### If you're reading these docs, here's what to look at instead:

| Old Document | Current Reference |
|-------------|-------------------|
| RuntimeServer | See `fraiseql-server/src/server.rs` - `Server<A>` implementation |
| Phase 1-5 Implementation | See actual code in `fraiseql-server/src/` |
| Observer Runtime | See `fraiseql-observers/` crate |
| Architecture Principles | See `.claude/ARCHITECTURE_PRINCIPLES.md` |
| Development Guide | See `.claude/CLAUDE.md` |

---

## For New Contributors

**Don't start here!** These endpoint-runtime docs are historical planning documents.

**Instead, read:**
1. **[`.claude/ARCHITECTURE_PRINCIPLES.md`](../../.claude/ARCHITECTURE_PRINCIPLES.md)** - Current architecture
2. **[`.claude/CLAUDE.md`](../../.claude/CLAUDE.md)** - Development guide
3. **[`fraiseql-server/src/server.rs`](../../crates/fraiseql-server/src/server.rs)** - Server implementation
4. **[`README.md`](../../README.md)** - Project overview

---

## Feature Implementation Examples

### How Features Are Actually Implemented

**Webhooks** (Phase 3):
```rust
// Located in: fraiseql-server/src/webhooks/
// Signature verification via trait
pub trait SignatureVerifier {
    fn verify(&self, payload: &[u8], signature: &str) -> Result<()>;
}

// Implementations: Stripe, GitHub, Shopify, etc.
```

**Observers** (Phase 6):
```rust
// Located in: fraiseql-observers/ (separate crate)
// Optional via: #[cfg(feature = "observers")]
// Server integration via:
#[cfg(feature = "observers")]
observer_runtime: Option<Arc<RwLock<ObserverRuntime>>>
```

**Authentication** (Phase 5):
```rust
// Located in: fraiseql-server/src/auth/
// OIDC validator
pub struct Server<A> {
    oidc_validator: Option<Arc<OidcValidator>>,
    // ...
}
```

---

## Summary

**These docs represent the planning phase.** The actual implementation is:

- **Simpler** - Single `Server<A>` instead of dual implementations
- **More flexible** - Generic over database adapter
- **Production-ready** - 294+ tests, feature-gated extensions
- **Well-documented** - See `.claude/ARCHITECTURE_PRINCIPLES.md`

**For current architecture:** See `.claude/ARCHITECTURE_PRINCIPLES.md`
**For development:** See `.claude/CLAUDE.md`
**For code:** See `fraiseql-server/src/server.rs`

---

**Last Updated**: January 30, 2026 (Post-architectural consolidation)
