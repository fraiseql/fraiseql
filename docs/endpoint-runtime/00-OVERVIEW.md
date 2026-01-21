# FraiseQL Endpoint Runtime Implementation Plan

## Overview

This document outlines the phased implementation of the FraiseQL Endpoint Runtime - a comprehensive backend runtime that extends the core GraphQL engine with webhooks, file uploads, authentication, notifications, observers, and 50+ integrations.

## Project Scope

```
┌─────────────────────────────────────────────────────────────────────┐
│                   FRAISEQL ENDPOINT RUNTIME                         │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Phase 1: Foundation                                                │
│  ├─ Configuration system (fraiseql.toml parsing)                   │
│  ├─ HTTP server infrastructure                                      │
│  ├─ Middleware pipeline                                             │
│  ├─ Error handling framework                                        │
│  └─ Graceful shutdown & lifecycle management                        │
│                                                                     │
│  Phase 2: Core Runtime Features                                     │
│  ├─ Rate limiting (with backpressure)                              │
│  ├─ CORS                                                            │
│  ├─ Health checks & metrics                                         │
│  ├─ Request tracing                                                 │
│  └─ Admission control                                               │
│                                                                     │
│  Phase 3: Webhook Runtime                                           │
│  ├─ Signature verification (15+ schemes)                            │
│  ├─ Event routing & mapping                                         │
│  ├─ Idempotency handling                                            │
│  ├─ Provider implementations                                        │
│  └─ Transaction boundaries & consistency                            │
│                                                                     │
│  Phase 4: File Runtime                                              │
│  ├─ Upload handling & validation                                    │
│  ├─ Image processing pipeline                                       │
│  ├─ Storage backends (S3, R2, GCS, etc.)                           │
│  ├─ CDN & signed URL support                                        │
│  └─ Virus scanning integration                                      │
│                                                                     │
│  Phase 5: Auth Runtime                                              │
│  ├─ OAuth 2.0 / OIDC implementation                                │
│  ├─ JWT generation & validation                                     │
│  ├─ Session management                                              │
│  ├─ Provider implementations (12+)                                  │
│  └─ Token rotation & revocation                                     │
│                                                                     │
│  Phase 6: Observer Runtime                                          │
│  ├─ Event emission from database                                    │
│  ├─ Action execution (email, slack, webhook, etc.)                 │
│  ├─ Retry & dead letter queue                                       │
│  ├─ Backpressure & admission control                               │
│  └─ Transaction semantics (at-least-once)                          │
│                                                                     │
│  Phase 7: Notification Runtime                                      │
│  ├─ Email providers (10+)                                           │
│  ├─ Chat providers (8+)                                             │
│  ├─ Push notification providers (8+)                                │
│  ├─ SMS providers (6+)                                              │
│  └─ Delivery tracking & receipts                                    │
│                                                                     │
│  Phase 8: Advanced Features                                         │
│  ├─ Search integration (with SLOs)                                 │
│  ├─ Cache integration (with coherency)                             │
│  ├─ Queue/job integration                                           │
│  └─ Real-time subscriptions                                         │
│                                                                     │
│  Phase 9: Interceptors & Custom Handlers                            │
│  ├─ Interceptor runtime (WASM with security sandbox)               │
│  ├─ Custom endpoint handlers                                        │
│  ├─ Plugin architecture                                             │
│  └─ Capability-based security                                       │
│                                                                     │
│  Phase 10: Polish & Production Readiness                            │
│  ├─ Comprehensive testing (unit, integration, e2e, contract)       │
│  ├─ Documentation                                                   │
│  ├─ Performance optimization                                        │
│  ├─ Security audit                                                  │
│  ├─ Migration strategy for existing users                          │
│  └─ Feature flags for compile-time subsystem selection             │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

## Crate Structure

```
fraiseql/crates/
├── fraiseql-core/              # Existing: GraphQL engine
├── fraiseql-server/            # Existing: HTTP server
├── fraiseql-cli/               # Existing: CLI
├── fraiseql-wire/              # Existing: Wire protocol
│
├── fraiseql-error/             # NEW: Shared error types (all crates depend on this)
│   ├── src/
│   │   ├── lib.rs              # Re-exports
│   │   ├── error.rs            # Root FraiseQLError enum
│   │   ├── config.rs           # ConfigError
│   │   ├── auth.rs             # AuthError
│   │   ├── webhook.rs          # WebhookError
│   │   ├── file.rs             # FileError
│   │   ├── notification.rs     # NotificationError
│   │   ├── observer.rs         # ObserverError
│   │   ├── integration.rs      # Search/Cache/Queue errors
│   │   └── http.rs             # IntoResponse impls
│   └── Cargo.toml
│
├── fraiseql-runtime/           # NEW: Main runtime crate
│   ├── src/
│   │   ├── lib.rs
│   │   ├── config/             # TOML configuration
│   │   │   ├── mod.rs
│   │   │   ├── loader.rs       # Configuration loading
│   │   │   └── validation.rs   # Cross-field validation
│   │   ├── middleware/         # HTTP middleware
│   │   ├── template/           # Shared template engine
│   │   ├── resilience/         # Circuit breaker, retry, backpressure
│   │   │   ├── mod.rs
│   │   │   ├── circuit_breaker.rs
│   │   │   ├── retry.rs
│   │   │   └── backpressure.rs # Admission control
│   │   ├── lifecycle/          # Graceful shutdown
│   │   │   ├── mod.rs
│   │   │   ├── shutdown.rs
│   │   │   └── health.rs
│   │   └── testing/            # Test utilities
│   │       ├── mod.rs
│   │       ├── mock_server.rs
│   │       └── fixtures.rs
│   └── Cargo.toml
│
├── fraiseql-webhooks/          # NEW: Webhook handling
│   ├── src/
│   │   ├── lib.rs
│   │   ├── providers/          # Provider implementations
│   │   ├── signature.rs        # Signature verification
│   │   ├── routing.rs          # Event routing
│   │   └── idempotency.rs      # With transaction support
│   └── Cargo.toml
│
├── fraiseql-files/             # NEW: File handling
│   ├── src/
│   │   ├── lib.rs
│   │   ├── storage/            # Storage backends
│   │   │   ├── mod.rs          # StorageBackend trait
│   │   │   ├── s3.rs
│   │   │   ├── gcs.rs
│   │   │   ├── local.rs
│   │   │   └── mock.rs         # For testing
│   │   ├── processing/         # Image processing
│   │   ├── validation.rs
│   │   └── scanning.rs         # Virus scanning
│   └── Cargo.toml
│
├── fraiseql-auth/              # NEW: Authentication
│   ├── src/
│   │   ├── lib.rs
│   │   ├── providers/          # OAuth providers
│   │   │   ├── mod.rs          # OAuthProvider trait
│   │   │   ├── google.rs
│   │   │   ├── github.rs
│   │   │   └── mock.rs         # For testing
│   │   ├── jwt.rs              # JWT handling
│   │   ├── session.rs          # Session management
│   │   └── rotation.rs         # Token rotation
│   └── Cargo.toml
│
├── fraiseql-observers/         # NEW: Post-mutation observers
│   ├── src/
│   │   ├── lib.rs
│   │   ├── listener.rs         # Event listener with backpressure
│   │   ├── actions/            # Built-in actions
│   │   ├── retry.rs            # Retry logic
│   │   ├── dlq.rs              # Dead letter queue
│   │   └── transaction.rs      # Transaction boundaries
│   └── Cargo.toml
│
├── fraiseql-notifications/     # NEW: Notification sending
│   ├── src/
│   │   ├── lib.rs
│   │   ├── email/              # Email providers
│   │   │   ├── mod.rs          # EmailProvider trait
│   │   │   ├── resend.rs
│   │   │   ├── sendgrid.rs
│   │   │   └── mock.rs         # For testing
│   │   ├── chat/               # Chat providers
│   │   ├── push/               # Push providers
│   │   ├── sms/                # SMS providers
│   │   └── tracking.rs         # Delivery tracking
│   └── Cargo.toml
│
└── fraiseql-integrations/      # NEW: Search, cache, queue
    ├── src/
    │   ├── lib.rs
    │   ├── search/             # Search providers
    │   │   ├── mod.rs          # SearchProvider trait
    │   │   ├── meilisearch.rs
    │   │   ├── postgres_fts.rs
    │   │   └── mock.rs         # For testing
    │   ├── cache/              # Cache backends
    │   │   ├── mod.rs          # CacheProvider trait
    │   │   ├── redis.rs
    │   │   ├── memory.rs
    │   │   └── mock.rs         # For testing
    │   └── queue/              # Queue backends
    │       ├── mod.rs          # QueueProvider trait
    │       ├── postgres.rs
    │       ├── redis.rs
    │       └── mock.rs         # For testing
    └── Cargo.toml
```

## Service Level Objectives (SLOs)

Define SLOs upfront to drive architectural decisions:

| Component | Latency (p99) | Availability | Error Rate |
|-----------|---------------|--------------|------------|
| GraphQL queries | < 100ms | 99.9% | < 0.1% |
| Webhook processing | < 500ms | 99.5% | < 1% |
| File uploads (< 10MB) | < 2s | 99.5% | < 1% |
| Auth (token validation) | < 10ms | 99.99% | < 0.01% |
| Observer actions | < 1s | 99% | < 5% (with retry) |
| Notifications | < 5s | 99% | < 5% (with retry) |
| Search queries | < 200ms | 99.5% | < 1% |
| Cache operations | < 5ms | 99.9% | < 0.1% |

**SLO Monitoring:**
- Prometheus metrics per component
- SLO burn rate alerts
- Error budget tracking dashboard

## Dependency Injection & Testing Strategy

### Testing Seams

Every external dependency uses a trait with a mock implementation:

```rust
// Pattern: Trait + Real + Mock implementations
pub trait StorageBackend: Send + Sync {
    async fn upload(&self, key: &str, data: &[u8]) -> Result<String, FileError>;
    async fn download(&self, key: &str) -> Result<Vec<u8>, FileError>;
    async fn delete(&self, key: &str) -> Result<(), FileError>;
}

// Real implementation
pub struct S3Backend { /* ... */ }
impl StorageBackend for S3Backend { /* ... */ }

// Mock for testing
#[cfg(any(test, feature = "testing"))]
pub struct MockStorageBackend {
    pub uploads: Arc<Mutex<Vec<(String, Vec<u8>)>>>,
    pub should_fail: AtomicBool,
}
```

### Test Infrastructure

```
tests/
├── unit/                   # Per-crate unit tests
├── integration/            # Crate integration tests
│   ├── auth_flow.rs
│   ├── webhook_processing.rs
│   └── file_upload.rs
├── e2e/                    # Full system tests
│   ├── graphql_queries.rs
│   └── complete_flows.rs
├── contract/               # Provider contract tests
│   ├── stripe_webhook.rs
│   ├── github_webhook.rs
│   └── oauth_providers.rs
├── load/                   # Performance tests
│   ├── concurrent_uploads.rs
│   └── webhook_throughput.rs
└── fixtures/               # Shared test data
    ├── webhooks/
    ├── files/
    └── schemas/
```

### Contract Testing

Test against real provider APIs (in CI with secrets):

```rust
#[tokio::test]
#[ignore] // Run only in CI with STRIPE_TEST_SECRET
async fn test_stripe_webhook_signature_real() {
    let secret = std::env::var("STRIPE_TEST_WEBHOOK_SECRET").unwrap();
    let verifier = StripeSignatureVerifier::new(&secret);

    // Use real Stripe test event
    let payload = include_bytes!("../fixtures/webhooks/stripe_checkout_completed.json");
    let signature = generate_real_stripe_signature(payload, &secret);

    assert!(verifier.verify(payload, &signature).is_ok());
}
```

## Compilation vs Runtime Configuration

**Key Question:** How does runtime configuration interact with compiled schemas?

### Schema Compilation Pipeline

```
┌──────────────────┐     ┌──────────────────┐     ┌──────────────────┐
│ Python/TS        │     │ fraiseql-cli     │     │ fraiseql-server  │
│ Schema Authoring │ ──> │ compile          │ ──> │ Runtime          │
│                  │     │                  │     │                  │
│ @fraiseql.type   │     │ Generates:       │     │ Loads:           │
│ class User:      │     │ - SQL templates  │     │ - Compiled schema│
│   id: int        │     │ - Type mappings  │     │ - Runtime config │
└──────────────────┘     │ - Query plans    │     │                  │
                         └──────────────────┘     └──────────────────┘
```

### What's Compiled vs Runtime

| Aspect | Compile Time | Runtime |
|--------|--------------|---------|
| Schema types | ✅ Baked in | ❌ |
| SQL templates | ✅ Optimized | ❌ |
| Type mappings | ✅ | ❌ |
| Database URL | ❌ | ✅ fraiseql.toml |
| Auth providers | ❌ | ✅ fraiseql.toml |
| Storage backends | ❌ | ✅ fraiseql.toml |
| Rate limits | ❌ | ✅ fraiseql.toml |
| Webhook secrets | ❌ | ✅ env vars |

### Runtime Extension Points

The compiled schema defines what's possible. Runtime config enables/disables:

```toml
# fraiseql.toml - Runtime configuration

[server]
port = 8080
compiled_schema = "./schema.compiled.json"  # Compile-time artifact

# Runtime features - extend compiled schema capabilities
[auth]
enabled = true
providers = ["google", "github"]

[webhooks]
enabled = true
providers = ["stripe", "github"]

[files]
enabled = true
storage = "s3"
```

## Feature Flags

Enable compile-time subsystem selection to reduce binary size:

```toml
# Cargo.toml
[features]
default = ["webhooks", "auth", "files", "observers", "notifications"]

# Individual features
webhooks = ["fraiseql-webhooks"]
auth = ["fraiseql-auth"]
files = ["fraiseql-files"]
observers = ["fraiseql-observers"]
notifications = ["fraiseql-notifications"]
integrations = ["fraiseql-integrations"]

# Provider-specific features
auth-google = ["auth", "google-signin"]
auth-github = ["auth"]
storage-s3 = ["files", "aws-sdk-s3"]
storage-gcs = ["files", "google-cloud-storage"]

# All features for development
full = ["webhooks", "auth", "files", "observers", "notifications", "integrations"]
```

**Usage:**
```bash
# Minimal build (just GraphQL)
cargo build --release --no-default-features

# GraphQL + Auth only
cargo build --release --no-default-features --features auth

# Full build
cargo build --release --features full
```

## Timeline Estimate

| Phase | Complexity | Dependencies |
|-------|------------|--------------|
| Phase 1: Foundation | Medium | None |
| Phase 2: Core Runtime | Medium | Phase 1 |
| Phase 3: Webhooks | High | Phase 1-2 |
| Phase 4: Files | High | Phase 1-2 |
| Phase 5: Auth | High | Phase 1-2 |
| Phase 6: Observers | High | Phase 1-2, 7 |
| Phase 7: Notifications | Medium | Phase 1 |
| Phase 8: Advanced | Medium | Phase 1-2 |
| Phase 9: Interceptors | High | Phase 1-6 |
| Phase 10: Polish | Medium | All |

## Key Dependencies (Rust Crates)

```toml
# Core
tokio = { version = "1", features = ["full", "signal"] }
axum = "0.7"
tower = "0.4"
tower-http = "0.5"

# Config
toml = "0.8"
serde = { version = "1", features = ["derive"] }
config = "0.14"

# Crypto
hmac = "0.12"
sha2 = "0.10"
ed25519-dalek = "2"
jsonwebtoken = "9"

# Storage
aws-sdk-s3 = "1"
google-cloud-storage = "0.15"
object_store = "0.9"  # Unified storage abstraction

# HTTP Client
reqwest = { version = "0.12", features = ["json", "rustls-tls"] }

# Database
sqlx = { version = "0.8", features = ["postgres", "runtime-tokio"] }

# Image Processing
image = "0.25"

# Templating
minijinja = "2"

# Observability
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
opentelemetry = "0.22"
opentelemetry-otlp = "0.15"
metrics = "0.22"
metrics-exporter-prometheus = "0.14"

# Testing
mockall = "0.12"
wiremock = "0.6"
testcontainers = "0.15"
criterion = "0.5"
```

## Documents in This Plan

1. **[01-PHASE-1-FOUNDATION.md](01-PHASE-1-FOUNDATION.md)** - Configuration, server, middleware, graceful shutdown
2. **[02-PHASE-2-CORE-RUNTIME.md](02-PHASE-2-CORE-RUNTIME.md)** - Rate limiting, CORS, health, backpressure
3. **[03-PHASE-3-WEBHOOKS.md](03-PHASE-3-WEBHOOKS.md)** - Webhook runtime with transaction boundaries
4. **[04-PHASE-4-FILES.md](04-PHASE-4-FILES.md)** - File upload, storage, scanning
5. **[05-PHASE-5-AUTH.md](05-PHASE-5-AUTH.md)** - OAuth, JWT, sessions, rotation
6. **[06-PHASE-6-OBSERVERS.md](06-PHASE-6-OBSERVERS.md)** - Post-mutation observers with backpressure
7. **[07-PHASE-7-NOTIFICATIONS.md](07-PHASE-7-NOTIFICATIONS.md)** - Email, chat, push, SMS with tracking
8. **[08-PHASE-8-ADVANCED.md](08-PHASE-8-ADVANCED.md)** - Search, cache, queue with SLOs
9. **[09-PHASE-9-INTERCEPTORS.md](09-PHASE-9-INTERCEPTORS.md)** - Custom handlers with WASM security
10. **[10-PHASE-10-POLISH.md](10-PHASE-10-POLISH.md)** - Testing, docs, security, migration

## Cross-Cutting Concerns

### Unified Migration Strategy

All database migrations across crates follow a unified approach:

```
migrations/
├── 00000_bootstrap.sql              # _system schema, extensions
├── 00001_fraiseql_runtime.sql       # Core runtime tables
├── 00002_fraiseql_webhooks.sql      # Webhook idempotency tables
├── 00003_fraiseql_observers.sql     # Observer event log, DLQ
├── 00004_fraiseql_auth.sql          # Session tables (if db-backed)
├── 00005_fraiseql_files.sql         # File metadata tables
├── 00006_fraiseql_integrations.sql  # Queue tables (pg_boss style)
└── README.md                        # Migration conventions
```

**Migration Conventions:**
1. All system tables live in `_system` schema
2. Migrations are idempotent (`IF NOT EXISTS`, `CREATE OR REPLACE`)
3. Each crate owns its own migration files
4. `fraiseql-cli migrate` applies all migrations in order
5. Version tracking via `_system.schema_migrations` table

**Embedded vs External:**
- Migrations are embedded in crate binaries via `include_str!`
- `fraiseql-cli migrate --export` dumps to filesystem for review

### Shared Template Engine

Both `fraiseql-observers` (Phase 6) and `fraiseql-notifications` (Phase 7) require template rendering. To avoid duplication:

```rust
// crates/fraiseql-runtime/src/template/mod.rs
// Shared template engine with minijinja backend
pub struct TemplateEngine {
    env: minijinja::Environment<'static>,
}

impl TemplateEngine {
    pub fn new() -> Self { /* ... */ }

    /// Register a custom filter
    pub fn register_filter<F>(&mut self, name: &str, filter: F)
    where F: Fn(&Value, &[Value]) -> Result<Value, Error> { /* ... */ }

    /// Render a template with context
    pub fn render(&self, template: &str, context: &Value) -> Result<String, TemplateError> { /* ... */ }
}
```

- Lives in `fraiseql-runtime` as a shared utility
- Observers and Notifications depend on `fraiseql-runtime` for templates
- Supports `{{ field }}`, `{{ field | filter }}`, `{{ env.VAR }}`
- Filter registry is extensible via `TemplateEngine::register_filter()`

### Shared Error Types

All crates use unified error types from `fraiseql-error`:

```rust
// crates/fraiseql-error/src/lib.rs

/// Root error type for all FraiseQL operations
#[derive(Debug, thiserror::Error)]
pub enum FraiseQLError {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("Authentication error: {0}")]
    Auth(#[from] AuthError),

    #[error("Webhook error: {0}")]
    Webhook(#[from] WebhookError),

    #[error("File error: {0}")]
    File(#[from] FileError),

    #[error("Notification error: {0}")]
    Notification(#[from] NotificationError),

    #[error("Observer error: {0}")]
    Observer(#[from] ObserverError),

    #[error("Integration error: {0}")]
    Integration(#[from] IntegrationError),
}

impl FraiseQLError {
    /// Get documentation URL for this error
    pub fn docs_url(&self) -> &'static str {
        match self {
            Self::Config(_) => "https://fraiseql.dev/errors/config",
            Self::Auth(_) => "https://fraiseql.dev/errors/auth",
            // ...
        }
    }

    /// Get error code for API responses
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::Config(e) => e.error_code(),
            Self::Auth(e) => e.error_code(),
            // ...
        }
    }
}
```

**Benefits:**
- Consistent error structure across all crates
- Single place for HTTP status code mapping
- Easier error handling in integration code
- Shared error documentation URLs
- Error codes for programmatic handling

### Circuit Breaker Pattern

External API calls (notification providers, webhooks, search) implement circuit breakers:

```rust
// crates/fraiseql-runtime/src/resilience/circuit_breaker.rs

use std::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CircuitState {
    Closed = 0,   // Normal operation
    Open = 1,     // Failing fast
    HalfOpen = 2, // Testing recovery
}

pub struct CircuitBreaker {
    name: String,
    state: AtomicU8,
    failure_count: AtomicU32,
    success_count: AtomicU32,
    last_failure: AtomicU64,
    config: CircuitBreakerConfig,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,    // Failures before opening
    pub reset_timeout: Duration,   // Time before half-open
    pub success_threshold: u32,    // Successes to close from half-open
    pub timeout: Duration,         // Per-call timeout
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            reset_timeout: Duration::from_secs(30),
            success_threshold: 2,
            timeout: Duration::from_secs(10),
        }
    }
}

impl CircuitBreaker {
    pub fn new(name: impl Into<String>, config: CircuitBreakerConfig) -> Self {
        Self {
            name: name.into(),
            state: AtomicU8::new(CircuitState::Closed as u8),
            failure_count: AtomicU32::new(0),
            success_count: AtomicU32::new(0),
            last_failure: AtomicU64::new(0),
            config,
        }
    }

    pub async fn execute<F, T, E>(&self, f: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: Future<Output = Result<T, E>>,
    {
        // Check if circuit is open
        if self.is_open() {
            return Err(CircuitBreakerError::Open {
                name: self.name.clone(),
                retry_after: self.retry_after(),
            });
        }

        // Execute with timeout
        let result = tokio::time::timeout(self.config.timeout, f).await;

        match result {
            Ok(Ok(value)) => {
                self.record_success();
                Ok(value)
            }
            Ok(Err(e)) => {
                self.record_failure();
                Err(CircuitBreakerError::Inner(e))
            }
            Err(_) => {
                self.record_failure();
                Err(CircuitBreakerError::Timeout {
                    name: self.name.clone(),
                    duration: self.config.timeout,
                })
            }
        }
    }

    fn is_open(&self) -> bool { /* ... */ }
    fn record_success(&self) { /* ... */ }
    fn record_failure(&self) { /* ... */ }
    fn retry_after(&self) -> Duration { /* ... */ }
}
```

**Usage in providers:**
```rust
impl EmailProvider for ResendProvider {
    async fn send(&self, message: &EmailMessage) -> Result<EmailResult, NotificationError> {
        self.circuit_breaker.execute(async {
            // actual send logic
            self.client.post(&self.config.api_url)
                .json(&message)
                .send()
                .await?
        }).await
        .map_err(|e| match e {
            CircuitBreakerError::Open { retry_after, .. } => {
                NotificationError::ProviderUnavailable { retry_after }
            }
            CircuitBreakerError::Timeout { .. } => {
                NotificationError::Timeout
            }
            CircuitBreakerError::Inner(e) => e,
        })
    }
}
```

### Backpressure & Admission Control

Prevent system overload with explicit backpressure handling:

```rust
// crates/fraiseql-runtime/src/resilience/backpressure.rs

use tokio::sync::Semaphore;
use std::sync::atomic::{AtomicU64, Ordering};

/// Admission controller for request/event processing
pub struct AdmissionController {
    /// Concurrent request limit
    semaphore: Semaphore,
    /// Current queue depth
    queue_depth: AtomicU64,
    /// Maximum queue depth before rejection
    max_queue_depth: u64,
    /// Metrics
    rejected_count: AtomicU64,
}

impl AdmissionController {
    pub fn new(max_concurrent: usize, max_queue_depth: u64) -> Self {
        Self {
            semaphore: Semaphore::new(max_concurrent),
            queue_depth: AtomicU64::new(0),
            max_queue_depth,
            rejected_count: AtomicU64::new(0),
        }
    }

    /// Try to acquire a permit, returns None if system is overloaded
    pub fn try_acquire(&self) -> Option<AdmissionPermit> {
        let current_depth = self.queue_depth.fetch_add(1, Ordering::SeqCst);

        if current_depth >= self.max_queue_depth {
            self.queue_depth.fetch_sub(1, Ordering::SeqCst);
            self.rejected_count.fetch_add(1, Ordering::SeqCst);
            return None;
        }

        match self.semaphore.try_acquire() {
            Ok(permit) => Some(AdmissionPermit {
                _permit: permit,
                queue_depth: &self.queue_depth,
            }),
            Err(_) => {
                // Semaphore full, but within queue depth - request will wait
                Some(AdmissionPermit {
                    _permit: self.semaphore.acquire().await.unwrap(),
                    queue_depth: &self.queue_depth,
                })
            }
        }
    }

    /// Acquire with timeout
    pub async fn acquire_timeout(&self, timeout: Duration) -> Result<AdmissionPermit, AdmissionError> {
        tokio::time::timeout(timeout, self.acquire())
            .await
            .map_err(|_| AdmissionError::Timeout)?
    }
}

pub struct AdmissionPermit<'a> {
    _permit: SemaphorePermit<'a>,
    queue_depth: &'a AtomicU64,
}

impl Drop for AdmissionPermit<'_> {
    fn drop(&mut self) {
        self.queue_depth.fetch_sub(1, Ordering::SeqCst);
    }
}
```

### Placeholder Implementation Tracking

Features marked as placeholder (not fully implemented) are tracked in each phase doc's acceptance criteria using `[PLACEHOLDER]` tag:

```markdown
- [x] Basic implementation complete
- [ ] [PLACEHOLDER] FCM OAuth2 token generation - use gcp_auth crate
- [ ] [PLACEHOLDER] APNs JWT signing - use jsonwebtoken crate
```

A tracking issue should be created for each placeholder before production release.

---

## Success Criteria

- [ ] All 50+ integrations implemented and tested
- [ ] Configuration-driven (fraiseql.toml)
- [ ] Zero-downtime deployable (graceful shutdown)
- [ ] Comprehensive error handling with error codes
- [ ] Full observability (metrics, tracing, logging)
- [ ] SLO compliance verified via load testing
- [ ] Security audit passed
- [ ] Documentation complete (API, deployment, operations)
- [ ] Migration guide for existing users
- [ ] Performance benchmarks meet SLO targets
- [ ] All `[PLACEHOLDER]` items resolved before v1.0
- [ ] Contract tests passing for all external providers
