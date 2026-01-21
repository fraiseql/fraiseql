# FraiseQL Endpoint Runtime: Implementation Summary

## Strategic Decision

**Architecture**: Unified `fraiseql-server` crate instead of separate crates for webhooks, files, and runtime.

**Rationale**:
- Single configuration system
- Shared error handling
- Reused middleware pipeline
- Easier testing and debugging
- Natural extension for Phases 5-10

---

## Document Structure

This implementation plan consists of:

1. **04B-PHASE-4B-RESTRUCTURING.md** - Consolidate Phases 1-4 into fraiseql-server
2. **05-PHASE-5-AUTH.md** - Add authentication module
3. **06-10-PHASES-6-10-OVERVIEW.md** - Roadmap for remaining features

---

## Phase Timeline

### Phase 1: Foundation ✅ COMPLETE
- Configuration loading (TOML, environment variables)
- Graceful shutdown coordination
- Health checks (liveness, readiness)
- Status: 560 LOC implemented in fraiseql-runtime

### Phase 2: Core Runtime ✅ COMPLETE
- Rate limiting (sliding window, memory & Redis backends)
- CORS support with wildcard patterns
- Metrics collection (Prometheus)
- Admission control (backpressure)
- Status: 2,091 LOC implemented in fraiseql-runtime

### Phase 3: Webhooks ✅ COMPLETE
- Signature verification for 15+ providers (Stripe, GitHub, Shopify, etc.)
- Idempotency with composite keys
- Multi-phase transactions
- Status: 2,800 LOC implemented in fraiseql-webhooks

### Phase 4: File Upload ✅ COMPLETE
- Storage abstraction (S3, local filesystem)
- File validation (size, MIME type, magic bytes)
- Image processing with variants (fit, fill, crop)
- Security: filename sanitization, path traversal prevention
- Status: 2,400 LOC implemented in fraiseql-files

**Total Implemented**: 7,851 LOC across 4 crates

### Phase 4B: Restructuring (PLANNED)
- Consolidate fraiseql-runtime modules into fraiseql-server
- Consolidate fraiseql-webhooks into fraiseql-server
- Consolidate fraiseql-files into fraiseql-server
- Update all imports and dependencies
- Result: Single unified fraiseql-server crate (~11K LOC)
- **Effort**: 3-6 hours

### Phase 5: Authentication (PLANNED)
- OAuth 2.0 / OpenID Connect support
- 12+ providers: Google, GitHub, Microsoft, Apple, Discord, etc.
- JWT session management with token rotation
- User creation and provider linking
- CSRF protection via state tokens
- **Effort**: 6-8 hours (major feature)

### Phases 6-10: Extended Features (PLANNED)
- **Phase 6**: Observers & Events (reactivity)
- **Phase 7**: Notifications (multi-channel delivery)
- **Phase 8A**: Full-Text Search
- **Phase 8B**: Caching & Query Optimization
- **Phase 8C**: Job Queues & Scheduling
- **Phase 9**: Interceptors (WASM/Lua customization)
- **Phase 10**: Polish (performance, observability)

---

## Key Architectural Decisions

### 1. Unified fraiseql-server
Instead of:
```
fraiseql-server (HTTP layer)
fraiseql-runtime (config, lifecycle)
fraiseql-webhooks (webhook handling)
fraiseql-files (file handling)
```

Use:
```
fraiseql-server (everything)
  ├── config/
  ├── lifecycle/
  ├── middleware/
  ├── webhooks/
  ├── files/
  ├── auth/        (Phase 5)
  ├── observers/   (Phase 6)
  └── ...
```

**Benefits**:
- Single Cargo.toml to manage dependencies
- Unified error handling (one error enum)
- Shared middleware (rate limiting applies to all routes)
- Simpler dependency injection (AppState contains all features)
- Easier testing (one server to test)

### 2. Trait-Based Dependencies
All external integrations use traits for testability:

```rust
// Interfaces defined
pub trait OAuthProvider { ... }
pub trait SessionStore { ... }
pub trait StorageBackend { ... }
pub trait SignatureVerifier { ... }

// Mock implementations for testing
pub struct MockOAuthProvider { ... }
pub struct MockSessionStore { ... }
pub struct MockStorage { ... }
pub struct MockSignatureVerifier { ... }
```

**Benefits**:
- No external service calls in tests
- Fast test execution
- Clear interfaces
- Easy to extend with new implementations

### 3. Configuration Composition
All configs merge into `RuntimeConfig`:

```rust
pub struct RuntimeConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub lifecycle: LifecycleConfig,
    pub webhooks: Option<WebhookConfig>,     // Phase 3
    pub files: Option<FileConfig>,           // Phase 4
    pub auth: Option<AuthConfig>,            // Phase 5
    pub observers: Option<ObserversConfig>,  // Phase 6
    // ... more features
}
```

**Benefits**:
- Single configuration file (fraiseql.toml)
- Features can be enabled/disabled
- Environment variable interpolation for all configs

### 4. Error Consolidation
All errors map to RuntimeError:

```rust
pub enum RuntimeError {
    Config(ConfigError),
    Webhook(WebhookError),
    File(FileError),
    Storage(StorageError),
    Auth(AuthError),
    // ...
}

impl IntoResponse for RuntimeError {
    // Convert all errors to HTTP responses
}
```

**Benefits**:
- Consistent HTTP error responses
- Error codes for client handling
- Documentation links in error responses
- Request logging of all errors

---

## File Organization After Phase 4B

```
crates/fraiseql-server/
├── src/
│   ├── lib.rs                    # Module exports
│   ├── main.rs                   # Binary entry point
│   ├── state.rs                  # AppState (dependency injection)
│   ├── error.rs                  # RuntimeError enum
│   │
│   ├── config/                   # Configuration system
│   │   ├── mod.rs
│   │   ├── loader.rs
│   │   └── validation.rs
│   │
│   ├── lifecycle/                # Graceful shutdown, health
│   │   ├── mod.rs
│   │   ├── shutdown.rs
│   │   └── health.rs
│   │
│   ├── middleware/               # Tower middleware
│   │   ├── mod.rs
│   │   ├── rate_limit.rs
│   │   ├── cors.rs
│   │   └── admission.rs
│   │
│   ├── observability/            # Tracing, metrics
│   │   ├── mod.rs
│   │   ├── tracing.rs
│   │   └── metrics.rs
│   │
│   ├── webhooks/                 # Phase 3
│   │   ├── mod.rs
│   │   ├── traits.rs
│   │   ├── signature/
│   │   ├── handler.rs
│   │   ├── testing.rs
│   │   └── routes.rs
│   │
│   ├── files/                    # Phase 4
│   │   ├── mod.rs
│   │   ├── traits.rs
│   │   ├── storage/
│   │   ├── validation.rs
│   │   ├── processing.rs
│   │   ├── handler.rs
│   │   ├── testing.rs
│   │   └── routes.rs
│   │
│   ├── auth/                     # Phase 5
│   │   ├── mod.rs
│   │   ├── config.rs
│   │   ├── error.rs
│   │   ├── traits.rs
│   │   ├── jwt.rs
│   │   ├── session.rs
│   │   ├── handler.rs
│   │   ├── providers/
│   │   ├── testing.rs
│   │   └── routes.rs
│   │
│   ├── observers/                # Phase 6
│   ├── notifications/            # Phase 7
│   ├── search/                   # Phase 8A
│   ├── cache/                    # Phase 8B
│   ├── jobs/                     # Phase 8C
│   ├── interceptors/             # Phase 9
│   │
│   ├── routes/
│   │   ├── mod.rs
│   │   ├── graphql.rs
│   │   ├── health.rs
│   │   ├── webhooks.rs
│   │   ├── files.rs
│   │   ├── auth.rs
│   │   └── ...
│   │
│   └── server.rs                 # RuntimeServer orchestration
│
├── tests/
│   ├── integration/
│   │   ├── config_test.rs
│   │   ├── webhook_test.rs
│   │   ├── file_test.rs
│   │   ├── auth_test.rs
│   │   └── full_stack_test.rs
│   │
│   ├── unit/
│   │   ├── config/
│   │   ├── webhooks/
│   │   ├── files/
│   │   ├── auth/
│   │   └── ...
│   │
│   └── fixtures/
│       ├── webhooks/
│       ├── files/
│       └── schemas/
│
├── migrations/                   # Database migrations
│   ├── 001_initial.sql
│   ├── 002_webhooks.sql
│   ├── 003_files.sql
│   ├── 004_auth.sql
│   └── ...
│
└── Cargo.toml
```

---

## Development Workflow After Phase 4B

### 1. Configuration
All features configured in one file:

```toml
# fraiseql.toml

[server]
host = "0.0.0.0"
port = 4000

[database]
url_env = "DATABASE_URL"

[webhooks]
tolerance_secs = 300
max_payload_size = "10MB"

[files]
storage = "s3"
max_file_size = "100MB"

[auth]
providers = ["google", "github"]

[observers]
# ... more features
```

### 2. Running the Server
```bash
# With all features enabled
RUST_LOG=debug cargo run --release

# Development mode
cargo watch -x run

# Run tests
cargo test

# Run specific test
cargo test auth::jwt_test
```

### 3. Adding a New Feature
1. Create module: `src/new_feature/mod.rs`
2. Add config: extend `RuntimeConfig`
3. Add to AppState: `state.rs`
4. Add routes: `routes/new_feature.rs`
5. Add tests: `tests/integration/new_feature_test.rs`

---

## Testing Strategy

### Unit Tests
Located in each module:
- Config parsing and validation
- Provider implementations
- Error handling

### Integration Tests
Test full request/response flows:
- OAuth callback handling
- Webhook signature verification
- File upload with image processing
- Multi-step transactions

### Mock Implementations
All external dependencies have mocks:

```rust
#[cfg(any(test, feature = "testing"))]
pub mod mocks {
    pub struct MockOAuthProvider { ... }
    pub struct MockStorage { ... }
    pub struct MockSignatureVerifier { ... }
}
```

Enable in tests:
```bash
cargo test --features testing
```

---

## Dependency Management

### Core Dependencies
```toml
[dependencies]
# Runtime
tokio = { version = "1", features = ["full"] }
axum = "0.8"

# Data
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sqlx = { version = "0.7", features = ["postgres"] }

# Async
async-trait = "0.1"
futures = "0.3"

# Error handling
thiserror = "1"

# Utilities
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
base64 = "0.21"
hex = "0.4"

# Phase 3: Webhooks
hmac = "0.12"
sha2 = "0.10"

# Phase 4: Files
image = { version = "0.24", features = ["jpeg", "png", "webp"] }
infer = "0.15"
mime_guess = "2"
aws-sdk-s3 = { version = "1", optional = true }

# Phase 5: Auth
jsonwebtoken = "9"
rand = "0.8"

# Observability
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json"] }
```

---

## Performance Considerations

### After Phase 4B
- Single crate compilation (faster than 4 separate crates)
- Shared middleware means rate limiting covers all routes
- Connection pooling managed in one place

### Phase 8 Focus
- Query result caching
- N+1 query detection
- Batch loading (DataLoader pattern)
- Index optimization

### Phase 10 Focus
- Performance profiling
- Flame graphs
- Custom metrics
- Optimization opportunities

---

## Security Model

### Defense in Depth

1. **Input Validation**
   - File size limits
   - MIME type checking
   - Filename sanitization
   - Webhook timestamp tolerance

2. **Authentication**
   - OAuth 2.0 with CSRF protection
   - JWT tokens with expiry
   - Refresh token rotation
   - Rate limiting on auth endpoints

3. **Authorization**
   - Session revocation
   - User-specific data filtering
   - Role-based access (Phase 5 extension)

4. **Data Protection**
   - Token hashing before storage
   - Encrypted webhooks
   - Secure headers (CORS, CSP)

5. **Audit Logging**
   - Auth events logged
   - Webhook processing tracked
   - File upload audited
   - Error events recorded

---

## Success Metrics

### Phase 4B
- ✅ All tests pass
- ✅ No new compilation warnings
- ✅ Single source of truth for configuration

### Phase 5
- ✅ OAuth flows work end-to-end
- ✅ JWT tokens valid and rotated correctly
- ✅ User creation/linking works
- ✅ CSRF protection prevents attacks

### Phases 6-10
- ✅ Each phase adds feature without breaking existing tests
- ✅ Performance benchmarks maintained
- ✅ No security regressions

---

## Migration Path

For users currently on separate crates (after Phase 4 but before 4B):

**Breaking Change**: Consolidation is a major version bump

```rust
// Before (Phase 4)
use fraiseql_runtime::config::RuntimeConfig;
use fraiseql_webhooks::handler::WebhookHandler;
use fraiseql_files::handler::FileHandler;

// After (Phase 4B+)
use fraiseql_server::config::RuntimeConfig;
use fraiseql_server::webhooks::handler::WebhookHandler;
use fraiseql_server::files::handler::FileHandler;
```

---

## Next Steps

1. **Phase 4B**: Execute restructuring (3-6 hours)
   - Consolidate 3 crates into fraiseql-server
   - Update all imports
   - Verify all tests pass

2. **Phase 5**: Implement auth (6-8 hours)
   - Add OAuth provider implementations
   - Implement JWT and session management
   - Add HTTP routes and tests

3. **Phases 6-10**: Features in order
   - Each phase extends fraiseql-server
   - Reuses existing infrastructure
   - Optional dependencies via config

---

## References

- **Phase 1-2**: Configuration, rate limiting, middleware
- **Phase 3**: Webhook handling with 15+ provider signatures
- **Phase 4**: File upload, storage, validation, image processing
- **Phase 4B**: Consolidation plan (this document)
- **Phase 5**: OAuth 2.0, JWT, multi-provider auth
- **Phases 6-10**: Extended features overview

All phases are documented in `/home/lionel/code/fraiseql/docs/endpoint-runtime/`

---

## Decision Timeline

- **Done**: Phases 1-4 implemented (7,851 LOC)
- **Next**: Phase 4B restructuring (consolidate crates)
- **Then**: Phase 5 auth (add OAuth module)
- **Finally**: Phases 6-10 (extend features)

Total implementation time (Phases 4B-10): ~60-80 hours

---

## Conclusion

This plan consolidates the endpoint runtime into a single, cohesive `fraiseql-server` crate while maintaining the architectural principles of:

1. **Trait-based design** for testability
2. **Unified configuration** for simplicity
3. **Shared infrastructure** for consistency
4. **Optional features** for flexibility
5. **Comprehensive testing** for reliability

The result is a production-ready, maintainable GraphQL server with auth, webhooks, file handling, and extensibility for future features.
