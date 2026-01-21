# Phase 4B: Restructuring - Consolidate into fraiseql-server

## Objective

Consolidate webhook, file, and runtime functionality from separate crates into the unified `fraiseql-server` crate, eliminating architectural boundaries that don't reflect the actual system design.

---

## 4B.1 Architecture Migration

### Current State (After Phases 1-4)
```
fraiseql-server/          (~3.5K LOC) - Existing HTTP server
fraiseql-runtime/         (~2.1K LOC) - Config, lifecycle, middleware
fraiseql-webhooks/        (~2.8K LOC) - Webhook handling
fraiseql-files/           (~2.4K LOC) - File upload handling
fraiseql-error/           (~0.6K LOC) - Shared error types
```

### Target State (After Phase 4B)
```
fraiseql-server/          (~11K LOC) - All features integrated
├── config/               - Runtime configuration
├── lifecycle/            - Graceful shutdown, health checks
├── middleware/           - Rate limiting, CORS, admission control
├── webhooks/             - Signature verification, idempotency
├── files/                - Storage, validation, image processing
├── routes/               - HTTP endpoints for all features
└── ...

fraiseql-error/           (~0.6K LOC) - Kept separate (legitimately)
```

### Why This Works

1. **Single Responsibility**: Server is ONE feature - the endpoint runtime
2. **Shared Infrastructure**: All features use the same:
   - Configuration system
   - Error handling and HTTP conversion
   - Middleware pipeline (rate limiting, tracing, metrics)
   - Dependency injection container
   - Database connections
3. **Natural Extension**: Phases 5-10 extend this unified server
4. **Testing Simplicity**: One server to test instead of coordinating multiple crates

---

## 4B.2 Migration Steps

### Step 1: Update fraiseql-server Cargo.toml

Add dependencies needed by runtime, webhooks, files:

```toml
[dependencies]
# Existing
axum = "0.8"
tokio = { version = "1", features = ["full"] }
# ... other existing deps

# From fraiseql-runtime
toml = "0.8"
dirs = "5.0"

# From fraiseql-webhooks
hmac = "0.12"
sha2 = "0.10"

# From fraiseql-files
image = { version = "0.24", features = ["jpeg", "png", "webp"] }
infer = "0.15"
mime_guess = "2"
aws-sdk-s3 = { version = "1", optional = true }

# Optional backends
[features]
default = ["webhooks", "files"]
webhooks = []
files = []
aws-s3 = ["aws-sdk-s3"]
```

### Step 2: Merge fraiseql-runtime modules into fraiseql-server

Move these directories from `crates/fraiseql-runtime/src/` into `crates/fraiseql-server/src/`:

```bash
# Copy configuration system
cp -r crates/fraiseql-runtime/src/config/ crates/fraiseql-server/src/

# Copy lifecycle management
cp -r crates/fraiseql-runtime/src/lifecycle/ crates/fraiseql-server/src/

# Copy middleware
cp -r crates/fraiseql-runtime/src/middleware/ crates/fraiseql-server/src/

# Copy resilience patterns (circuit breaker, backpressure)
cp -r crates/fraiseql-runtime/src/resilience/ crates/fraiseql-server/src/

# Copy template engine
cp -r crates/fraiseql-runtime/src/template/ crates/fraiseql-server/src/

# Copy testing utilities
cp -r crates/fraiseql-runtime/src/testing/ crates/fraiseql-server/src/

# Copy shared state
cp crates/fraiseql-runtime/src/state.rs crates/fraiseql-server/src/
```

### Step 3: Merge fraiseql-webhooks modules into fraiseql-server

```bash
# Copy webhook handling
cp -r crates/fraiseql-webhooks/src/ crates/fraiseql-server/src/webhooks/

# Update imports: fraiseql_webhooks:: → crate::webhooks::
# Update imports: fraiseql_error:: → crate::error:: (or keep if fraiseql-error dependency exists)
```

### Step 4: Merge fraiseql-files modules into fraiseql-server

```bash
# Copy file handling
cp -r crates/fraiseql-files/src/ crates/fraiseql-server/src/files/

# Update imports: fraiseql_files:: → crate::files::
```

### Step 5: Update fraiseql-server/src/lib.rs

```rust
// crates/fraiseql-server/src/lib.rs

pub mod config;
pub mod lifecycle;
pub mod middleware;
pub mod resilience;
pub mod template;
pub mod testing;
pub mod state;
pub mod error;

pub mod webhooks;
pub mod files;

pub mod routes;
pub mod server;

// Re-exports for convenience
pub use config::RuntimeConfig;
pub use state::AppState;
pub use error::RuntimeError;
pub use server::RuntimeServer;
pub use routes::RuntimeRouter;
```

### Step 6: Update routes/mod.rs to include webhook and file routes

```rust
// crates/fraiseql-server/src/routes/mod.rs

use axum::{
    Router,
    routing::{get, post, delete},
};

pub mod graphql;
pub mod webhooks;
pub mod files;
pub mod auth;  // Phase 5
pub mod health;

pub fn build_routes(state: Arc<AppState>) -> Router {
    Router::new()
        // GraphQL endpoint
        .route("/graphql", post(graphql::handler))

        // Health checks
        .route("/health", get(health::liveness))
        .route("/ready", get(health::readiness))

        // Webhook routes (Phase 3)
        .nest("/webhooks", webhooks::routes(state.clone()))

        // File upload routes (Phase 4)
        .nest("/files", files::routes(state.clone()))

        // Auth routes (Phase 5)
        .nest("/auth", auth::routes(state.clone()))

        .with_state(state)
}
```

### Step 7: Update imports in all merged files

Replace import statements:

```rust
// OLD
use fraiseql_runtime::config::RuntimeConfig;
use fraiseql_webhooks::traits::SignatureVerifier;
use fraiseql_files::traits::StorageBackend;

// NEW
use crate::config::RuntimeConfig;
use crate::webhooks::traits::SignatureVerifier;
use crate::files::traits::StorageBackend;
```

### Step 8: Delete old crates from workspace

Update `Cargo.toml` in repo root:

```toml
[workspace]
members = [
    "crates/fraiseql-core",
    "crates/fraiseql-server",  # Now includes everything
    "crates/fraiseql-cli",
    "crates/fraiseql-wire",
    "crates/fraiseql-error",   # Keep as separate dependency
]
```

Delete directories:
```bash
rm -rf crates/fraiseql-runtime
rm -rf crates/fraiseql-webhooks
rm -rf crates/fraiseql-files
```

### Step 9: Update all test imports

Files like `crates/fraiseql-server/tests/integration/` should update:

```rust
// OLD
use fraiseql_runtime::config::RuntimeConfig;
use fraiseql_webhooks::testing::mocks::MockSignatureVerifier;
use fraiseql_files::testing::mocks::MockStorage;

// NEW
use fraiseql_server::config::RuntimeConfig;
use fraiseql_server::webhooks::testing::mocks::MockSignatureVerifier;
use fraiseql_server::files::testing::mocks::MockStorage;
```

### Step 10: Verify compilation and tests

```bash
# Check for compilation errors
cargo check -p fraiseql-server

# Run all tests
cargo test -p fraiseql-server

# Run specific test suites
cargo test -p fraiseql-server --test webhook_test
cargo test -p fraiseql-server --test file_test
cargo test -p fraiseql-server --test config_test
```

---

## 4B.3 Impact on Phases 5-10

With this restructuring, Phases 5-10 are simplified:

### Phase 5: Auth
- Add `crate::auth/` module to `fraiseql-server`
- Use shared config, middleware, error handling
- Reuse same database connection pool
- Reuse same route builder

### Phase 6: Observers
- Add `crate::observers/` module
- Use shared template engine (already available)
- Use shared database connections
- Use shared middleware for backpressure

### Phase 7: Notifications
- Add `crate::notifications/` module
- Use shared template engine
- Use shared circuit breaker from `resilience/`
- Use shared error handling

### Phase 8: Advanced Features
- Search, cache, queue: all added as submodules
- Reuse existing infrastructure

### Phase 9: Interceptors
- Add WASM runtime as `crate::interceptors/`
- Use shared middleware pipeline

### Phase 10: Polish
- Single crate to test and optimize
- Single binary to benchmark
- Unified documentation

---

## 4B.4 Files to Create/Modify

### Delete (after copying contents)
- `crates/fraiseql-runtime/`
- `crates/fraiseql-webhooks/`
- `crates/fraiseql-files/`

### Modify
- `crates/fraiseql-server/Cargo.toml` - Add new dependencies
- `crates/fraiseql-server/src/lib.rs` - Add new module exports
- `crates/fraiseql-server/src/routes/mod.rs` - Include webhook/file routes
- `Cargo.toml` (workspace) - Remove crates from members list
- All test files - Update imports

### Create (new in fraiseql-server)
- `src/webhooks/` (from fraiseql-webhooks)
- `src/files/` (from fraiseql-files)
- `src/config/` (from fraiseql-runtime)
- `src/lifecycle/` (from fraiseql-runtime)
- `src/middleware/` (from fraiseql-runtime)
- `src/resilience/` (from fraiseql-runtime)
- `src/template/` (from fraiseql-runtime)
- `src/routes/webhooks.rs` - Webhook HTTP handlers
- `src/routes/files.rs` - File HTTP handlers

---

## 4B.5 Breaking Changes

For users of these crates:

**BEFORE:**
```rust
use fraiseql_runtime::config::RuntimeConfig;
use fraiseql_webhooks::traits::SignatureVerifier;
use fraiseql_files::handler::FileHandler;
```

**AFTER:**
```rust
use fraiseql_server::config::RuntimeConfig;
use fraiseql_server::webhooks::traits::SignatureVerifier;
use fraiseql_server::files::handler::FileHandler;
```

This is a **major version bump** (1.0 → 2.0) since the public API has changed.

---

## 4B.6 Benefits After Restructuring

1. **Simplified Architecture**: One crate to understand, not four
2. **Easier Maintenance**: Changes that affect multiple subsystems only require updates in one place
3. **Better Integration**: No crate boundaries means deeper integration (e.g., shared rate limiting for webhooks and files)
4. **Faster Compilation**: One crate compiles faster than multiple crates
5. **Cleaner Dependencies**: No need for complex inter-crate dependency management
6. **Natural Extension**: Phases 5-10 naturally extend the unified server

---

## 4B.7 Testing Strategy

After consolidation, tests should reflect the unified architecture:

```
crates/fraiseql-server/tests/
├── integration/
│   ├── config_test.rs          # Configuration loading and validation
│   ├── webhook_test.rs         # Webhook signature verification + idempotency
│   ├── file_test.rs            # File upload, storage, processing
│   ├── auth_test.rs            # OAuth flows (Phase 5)
│   └── full_stack_test.rs      # Complete request flows
├── unit/
│   ├── config/
│   ├── webhooks/
│   ├── files/
│   └── ...
└── fixtures/
    ├── webhooks/               # Sample webhook payloads
    ├── files/                  # Sample files for testing
    └── schemas/
```

---

## 4B.8 Acceptance Criteria

- [ ] All modules from fraiseql-runtime merged into fraiseql-server
- [ ] All modules from fraiseql-webhooks merged into fraiseql-server
- [ ] All modules from fraiseql-files merged into fraiseql-server
- [ ] All imports updated throughout the codebase
- [ ] Old crates removed from workspace
- [ ] `cargo check` passes
- [ ] All tests pass (`cargo test`)
- [ ] No compilation warnings
- [ ] Documentation updated to reflect new structure
- [ ] README explains consolidated architecture

---

## 4B.9 Rollback Plan

If consolidation causes issues:

1. Keep copies of original crate structures
2. Git commits are atomic - can revert to pre-4B state
3. No data migrations needed (code-only change)
4. Tests provide safety net

---

## Notes for Phase 5+

After Phase 4B completes:

- Phase 5 should add `crate::auth/` directly to fraiseql-server
- Phase 6 should add `crate::observers/` directly
- No more separate crate creation
- All new features become fraiseql-server submodules
- Single Cargo.toml to manage all dependencies

This streamlines development for the remaining phases.

---

## Estimated Effort

- Code consolidation: 1-2 hours
- Import updates: 1-2 hours
- Testing and verification: 1-2 hours
- **Total: 3-6 hours**

Most of this is mechanical work that could be automated with script/find+replace.

---

## Success Metrics

After Phase 4B:

1. One crate to maintain (fraiseql-server)
2. Phases 1-4 features still work identically
3. Compilation time may decrease (fewer crate boundaries)
4. Code is easier to navigate (fewer inter-crate imports)
5. Future phases extend existing structure naturally
