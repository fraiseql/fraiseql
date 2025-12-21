# Phase 12: Security Features & Enterprise Hardening in Rust

**Objective**: Move rate limiting, security headers, audit logging, and advanced security features from Python to Rust for production-grade hardening.

**Current State**: Security features scattered across Python middleware and decorators

**Target State**: Unified Rust security layer with zero-overhead enforcement

---

## Context

**Why This Phase Matters:**
- Rate limiting is critical for DDoS protection
- Security headers prevent common attacks (XSS, CSRF, clickjacking)
- Audit logging is required for compliance (SOC2, HIPAA, GDPR)
- Rust enforcement is 10-50x faster than Python middleware

**Dependencies:**
- Phase 10 (Auth) ✅ Required
- Phase 11 (RBAC) ✅ Required
- UserContext with full auth/RBAC data

**Performance Target:**
- Rate limit check: <0.05ms
- Security header injection: <0.01ms
- Audit log write: <0.5ms (async)
- Total security overhead: <1ms

---

## Files to Modify/Create

### Rust Files (fraiseql_rs/src/security/)
- **mod.rs** (NEW): Security module exports
- **rate_limit.rs** (NEW): Token bucket rate limiting
- **headers.rs** (NEW): Security header enforcement
- **audit.rs** (NEW): Audit logging with async writes
- **validators.rs** (NEW): Input validation (query depth, complexity)
- **csrf.rs** (NEW): CSRF token validation
- **cors.rs** (NEW): CORS policy enforcement

### Integration Files
- **fraiseql_rs/src/lib.rs**: Add security module
- **fraiseql_rs/src/pipeline/unified.rs**: Integrate security checks
- **fraiseql_rs/Cargo.toml**: Add dependencies

### Python Migration Files
- **src/fraiseql/security/rust_security.py** (NEW): Python wrapper
- **src/fraiseql/security/**: Deprecate Python implementations

### Test Files
- **tests/test_rust_security.py** (NEW): Integration tests
- **tests/unit/security/test_rate_limiting.rs** (NEW): Rust tests

---

## Implementation Steps

### Step 1: Rate Limiting (rate_limit.rs)

```rust
//! Token bucket rate limiting with Redis backend.

use anyhow::{Result, anyhow};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use std::collections::HashMap;

/// Rate limit strategy
#[derive(Debug, Clone, Copy)]
pub enum RateLimitStrategy {
    FixedWindow,
    SlidingWindow,
    TokenBucket,
}

/// Rate limit configuration
#[derive(Debug, Clone)]
pub struct RateLimit {
    pub requests: usize,
    pub window_secs: u64,
    pub burst: Option<usize>,
    pub strategy: RateLimitStrategy,
}

/// Rate limiter with token bucket algorithm
pub struct RateLimiter {
    limits: HashMap<String, RateLimit>,  // path -> limit
    store: Arc<Mutex<RateLimitStore>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            limits: HashMap::new(),
            store: Arc::new(Mutex::new(RateLimitStore::new())),
        }
    }

    /// Add rate limit rule for path pattern
    pub fn add_rule(&mut self, path_pattern: String, limit: RateLimit) {
        self.limits.insert(path_pattern, limit);
    }

    /// Check if request is allowed (returns Ok or rate limit error)
    pub async fn check(&self, key: &str, path: &str) -> Result<()> {
        // Find matching limit
        let limit = self.limits.get(path)
            .or_else(|| self.limits.get("*"))  // Default limit
            .ok_or_else(|| anyhow!("No rate limit configured"))?;

        let mut store = self.store.lock().await;

        match limit.strategy {
            RateLimitStrategy::TokenBucket => {
                self.check_token_bucket(&mut store, key, limit).await
            }
            RateLimitStrategy::FixedWindow => {
                self.check_fixed_window(&mut store, key, limit).await
            }
            RateLimitStrategy::SlidingWindow => {
                self.check_sliding_window(&mut store, key, limit).await
            }
        }
    }

    /// Token bucket algorithm (recommended)
    async fn check_token_bucket(
        &self,
        store: &mut RateLimitStore,
        key: &str,
        limit: &RateLimit,
    ) -> Result<()> {
        let now = current_timestamp();
        let bucket = store.get_bucket(key, limit.requests, limit.window_secs);

        // Refill tokens based on time elapsed
        let elapsed = now - bucket.last_refill;
        let refill_rate = limit.requests as f64 / limit.window_secs as f64;
        let tokens_to_add = (elapsed as f64 * refill_rate) as usize;

        bucket.tokens = (bucket.tokens + tokens_to_add).min(limit.requests);
        bucket.last_refill = now;

        // Check if token available
        if bucket.tokens > 0 {
            bucket.tokens -= 1;
            Ok(())
        } else {
            let retry_after = (1.0 / refill_rate) as u64;
            Err(anyhow!("Rate limit exceeded. Retry after {} seconds", retry_after))
        }
    }

    /// Fixed window algorithm
    async fn check_fixed_window(
        &self,
        store: &mut RateLimitStore,
        key: &str,
        limit: &RateLimit,
    ) -> Result<()> {
        let now = current_timestamp();
        let window = store.get_window(key);

        // Reset if window expired
        if now - window.start >= limit.window_secs {
            window.start = now;
            window.count = 0;
        }

        // Check limit
        if window.count < limit.requests {
            window.count += 1;
            Ok(())
        } else {
            let retry_after = limit.window_secs - (now - window.start);
            Err(anyhow!("Rate limit exceeded. Retry after {} seconds", retry_after))
        }
    }

    /// Sliding window algorithm
    async fn check_sliding_window(
        &self,
        store: &mut RateLimitStore,
        key: &str,
        limit: &RateLimit,
    ) -> Result<()> {
        let now = current_timestamp();
        let requests = store.get_requests(key);

        // Remove old requests outside window
        requests.retain(|&ts| now - ts < limit.window_secs);

        // Check limit
        if requests.len() < limit.requests {
            requests.push(now);
            Ok(())
        } else {
            let oldest = requests[0];
            let retry_after = limit.window_secs - (now - oldest);
            Err(anyhow!("Rate limit exceeded. Retry after {} seconds", retry_after))
        }
    }
}

/// In-memory rate limit store (production would use Redis)
struct RateLimitStore {
    buckets: HashMap<String, TokenBucket>,
    windows: HashMap<String, FixedWindow>,
    requests: HashMap<String, Vec<u64>>,
}

impl RateLimitStore {
    fn new() -> Self {
        Self {
            buckets: HashMap::new(),
            windows: HashMap::new(),
            requests: HashMap::new(),
        }
    }

    fn get_bucket(&mut self, key: &str, capacity: usize, window: u64) -> &mut TokenBucket {
        self.buckets.entry(key.to_string()).or_insert_with(|| TokenBucket {
            tokens: capacity,
            capacity,
            last_refill: current_timestamp(),
        })
    }

    fn get_window(&mut self, key: &str) -> &mut FixedWindow {
        self.windows.entry(key.to_string()).or_insert_with(|| FixedWindow {
            start: current_timestamp(),
            count: 0,
        })
    }

    fn get_requests(&mut self, key: &str) -> &mut Vec<u64> {
        self.requests.entry(key.to_string()).or_insert_with(Vec::new)
    }
}

#[derive(Debug)]
struct TokenBucket {
    tokens: usize,
    capacity: usize,
    last_refill: u64,
}

#[derive(Debug)]
struct FixedWindow {
    start: u64,
    count: usize,
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
```

### Step 2: Security Headers (headers.rs)

```rust
//! Security header enforcement.

use std::collections::HashMap;

/// Security headers configuration
pub struct SecurityHeaders {
    headers: HashMap<String, String>,
}

impl SecurityHeaders {
    /// Create default security headers
    pub fn default() -> Self {
        let mut headers = HashMap::new();

        // Prevent XSS
        headers.insert(
            "X-XSS-Protection".to_string(),
            "1; mode=block".to_string(),
        );

        // Prevent MIME sniffing
        headers.insert(
            "X-Content-Type-Options".to_string(),
            "nosniff".to_string(),
        );

        // Prevent clickjacking
        headers.insert(
            "X-Frame-Options".to_string(),
            "DENY".to_string(),
        );

        // HSTS (HTTPS only)
        headers.insert(
            "Strict-Transport-Security".to_string(),
            "max-age=31536000; includeSubDomains".to_string(),
        );

        // CSP (Content Security Policy)
        headers.insert(
            "Content-Security-Policy".to_string(),
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'".to_string(),
        );

        // Referrer policy
        headers.insert(
            "Referrer-Policy".to_string(),
            "strict-origin-when-cross-origin".to_string(),
        );

        // Permissions policy
        headers.insert(
            "Permissions-Policy".to_string(),
            "geolocation=(), microphone=(), camera=()".to_string(),
        );

        Self { headers }
    }

    /// Create production-grade security headers
    pub fn production() -> Self {
        let mut headers = Self::default().headers;

        // Stricter CSP for production
        headers.insert(
            "Content-Security-Policy".to_string(),
            "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data: https:; font-src 'self'; connect-src 'self'; frame-ancestors 'none'".to_string(),
        );

        // HSTS with preload
        headers.insert(
            "Strict-Transport-Security".to_string(),
            "max-age=63072000; includeSubDomains; preload".to_string(),
        );

        Self { headers }
    }

    /// Get headers as Vec for HTTP response
    pub fn to_vec(&self) -> Vec<(String, String)> {
        self.headers.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Add custom header
    pub fn add(&mut self, name: String, value: String) {
        self.headers.insert(name, value);
    }

    /// Remove header
    pub fn remove(&mut self, name: &str) {
        self.headers.remove(name);
    }
}
```

### Step 3: Audit Logging (audit.rs)

```rust
//! Async audit logging for security events.

use anyhow::Result;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use sqlx::PgPool;
use tokio::sync::mpsc;

/// Audit event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    // Authentication
    LoginSuccess,
    LoginFailure,
    Logout,
    TokenRefresh,
    TokenRevoke,

    // Authorization
    PermissionGranted,
    PermissionDenied,
    RoleAssigned,
    RoleRevoked,

    // Data access
    DataRead,
    DataWrite,
    DataDelete,

    // Security
    RateLimitExceeded,
    InvalidToken,
    SuspiciousActivity,
}

/// Audit event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: Uuid,
    pub event_type: AuditEventType,
    pub user_id: Option<Uuid>,
    pub tenant_id: Option<Uuid>,
    pub resource: Option<String>,
    pub action: Option<String>,
    pub status: String,  // "success" or "failure"
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub timestamp: DateTime<Utc>,
}

impl AuditEvent {
    pub fn new(event_type: AuditEventType) -> Self {
        Self {
            id: Uuid::new_v4(),
            event_type,
            user_id: None,
            tenant_id: None,
            resource: None,
            action: None,
            status: "success".to_string(),
            ip_address: None,
            user_agent: None,
            metadata: None,
            timestamp: Utc::now(),
        }
    }

    pub fn with_user(mut self, user_id: Uuid) -> Self {
        self.user_id = Some(user_id);
        self
    }

    pub fn with_tenant(mut self, tenant_id: Uuid) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }

    pub fn with_resource(mut self, resource: String, action: String) -> Self {
        self.resource = Some(resource);
        self.action = Some(action);
        self
    }

    pub fn with_status(mut self, status: String) -> Self {
        self.status = status;
        self
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Async audit logger with buffered writes
pub struct AuditLogger {
    tx: mpsc::UnboundedSender<AuditEvent>,
}

impl AuditLogger {
    /// Create audit logger with async worker
    pub fn new(pool: PgPool) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        // Spawn async worker to write audit logs
        tokio::spawn(async move {
            Self::audit_worker(pool, rx).await;
        });

        Self { tx }
    }

    /// Log audit event (non-blocking)
    pub fn log(&self, event: AuditEvent) {
        // Fire and forget - if channel is closed, event is lost
        // Production would use reliable queue (Kafka, RabbitMQ)
        let _ = self.tx.send(event);
    }

    /// Async worker to write audit logs to database
    async fn audit_worker(
        pool: PgPool,
        mut rx: mpsc::UnboundedReceiver<AuditEvent>,
    ) {
        while let Some(event) = rx.recv().await {
            if let Err(e) = Self::write_event(&pool, &event).await {
                eprintln!("Failed to write audit log: {}", e);
            }
        }
    }

    /// Write single event to database
    async fn write_event(pool: &PgPool, event: &AuditEvent) -> Result<()> {
        let sql = r#"
            INSERT INTO audit_logs (
                id, event_type, user_id, tenant_id, resource, action,
                status, ip_address, user_agent, metadata, timestamp
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        "#;

        sqlx::query(sql)
            .bind(&event.id)
            .bind(serde_json::to_string(&event.event_type)?)
            .bind(&event.user_id)
            .bind(&event.tenant_id)
            .bind(&event.resource)
            .bind(&event.action)
            .bind(&event.status)
            .bind(&event.ip_address)
            .bind(&event.user_agent)
            .bind(&event.metadata)
            .bind(&event.timestamp)
            .execute(pool)
            .await?;

        Ok(())
    }
}

impl Clone for AuditLogger {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}
```

### Step 4: Query Validators (validators.rs)

```rust
//! Query validation (depth, complexity, size limits).

use anyhow::{Result, anyhow};
use crate::graphql::types::ParsedQuery;

/// Query validation limits
#[derive(Debug, Clone)]
pub struct QueryLimits {
    pub max_depth: usize,
    pub max_complexity: usize,
    pub max_query_size: usize,
    pub max_list_size: usize,
}

impl Default for QueryLimits {
    fn default() -> Self {
        Self {
            max_depth: 10,
            max_complexity: 1000,
            max_query_size: 100_000,  // 100KB
            max_list_size: 1000,
        }
    }
}

impl QueryLimits {
    pub fn production() -> Self {
        Self {
            max_depth: 7,
            max_complexity: 500,
            max_query_size: 50_000,
            max_list_size: 500,
        }
    }
}

/// Query validator
pub struct QueryValidator {
    limits: QueryLimits,
}

impl QueryValidator {
    pub fn new(limits: QueryLimits) -> Self {
        Self { limits }
    }

    /// Validate query against all limits
    pub fn validate(&self, query: &str, parsed: &ParsedQuery) -> Result<()> {
        // Check query size
        if query.len() > self.limits.max_query_size {
            return Err(anyhow!(
                "Query too large: {} bytes (max: {})",
                query.len(),
                self.limits.max_query_size
            ));
        }

        // Check depth
        let depth = self.calculate_depth(parsed);
        if depth > self.limits.max_depth {
            return Err(anyhow!(
                "Query too deep: {} levels (max: {})",
                depth,
                self.limits.max_depth
            ));
        }

        // Check complexity
        let complexity = self.calculate_complexity(parsed);
        if complexity > self.limits.max_complexity {
            return Err(anyhow!(
                "Query too complex: {} (max: {})",
                complexity,
                self.limits.max_complexity
            ));
        }

        Ok(())
    }

    /// Calculate query depth (max nesting level)
    fn calculate_depth(&self, query: &ParsedQuery) -> usize {
        // Recursive depth calculation
        // Simplified for phase plan
        query.selections.len()
    }

    /// Calculate query complexity (estimated cost)
    fn calculate_complexity(&self, query: &ParsedQuery) -> usize {
        // Complexity = fields * estimated rows
        // Simplified for phase plan
        query.selections.len() * 10
    }
}
```

### Step 5: CSRF Protection (csrf.rs)

```rust
//! CSRF token validation.

use anyhow::{Result, anyhow};
use sha2::{Sha256, Digest};
use rand::Rng;

/// CSRF token manager
pub struct CSRFManager {
    secret: String,
}

impl CSRFManager {
    pub fn new(secret: String) -> Self {
        Self { secret }
    }

    /// Generate CSRF token for session
    pub fn generate_token(&self, session_id: &str) -> String {
        let nonce: [u8; 32] = rand::thread_rng().gen();
        let payload = format!("{}:{}", session_id, hex::encode(nonce));

        let mut hasher = Sha256::new();
        hasher.update(payload.as_bytes());
        hasher.update(self.secret.as_bytes());

        format!("{}:{}", payload, hex::encode(hasher.finalize()))
    }

    /// Validate CSRF token
    pub fn validate_token(&self, session_id: &str, token: &str) -> Result<()> {
        let parts: Vec<&str> = token.split(':').collect();
        if parts.len() != 3 {
            return Err(anyhow!("Invalid CSRF token format"));
        }

        let provided_session = parts[0];
        let nonce = parts[1];
        let provided_hash = parts[2];

        // Verify session matches
        if provided_session != session_id {
            return Err(anyhow!("CSRF token session mismatch"));
        }

        // Verify hash
        let payload = format!("{}:{}", provided_session, nonce);
        let mut hasher = Sha256::new();
        hasher.update(payload.as_bytes());
        hasher.update(self.secret.as_bytes());
        let expected_hash = hex::encode(hasher.finalize());

        if expected_hash != provided_hash {
            return Err(anyhow!("Invalid CSRF token"));
        }

        Ok(())
    }
}
```

### Step 6: Integration with Pipeline (unified.rs)

```rust
// Add security layer to execute_sync()

use crate::security::{
    rate_limit::RateLimiter,
    headers::SecurityHeaders,
    audit::{AuditLogger, AuditEvent, AuditEventType},
    validators::QueryValidator,
};

pub struct GraphQLPipeline {
    schema: SchemaMetadata,
    cache: Arc<QueryPlanCache>,
    rbac_resolver: Option<Arc<PermissionResolver>>,
    rate_limiter: Option<Arc<RateLimiter>>,  // NEW
    audit_logger: Option<AuditLogger>,  // NEW
    query_validator: QueryValidator,  // NEW
}

impl GraphQLPipeline {
    pub fn with_security(
        mut self,
        rate_limiter: RateLimiter,
        audit_logger: AuditLogger,
        query_limits: QueryLimits,
    ) -> Self {
        self.rate_limiter = Some(Arc::new(rate_limiter));
        self.audit_logger = Some(audit_logger);
        self.query_validator = QueryValidator::new(query_limits);
        self
    }

    pub async fn execute_with_security(
        &self,
        query_string: &str,
        variables: HashMap<String, JsonValue>,
        user_context: UserContext,
        request_info: RequestInfo,  // IP, user agent, etc.
    ) -> Result<(Vec<u8>, Vec<(String, String)>)> {  // (response, headers)
        // Phase 12: Rate limiting
        if let Some(limiter) = &self.rate_limiter {
            let rate_key = format!("user:{}", user_context.user_id.as_ref().unwrap_or(&"anonymous".to_string()));
            if let Err(e) = limiter.check(&rate_key, "/graphql").await {
                // Log rate limit event
                if let Some(logger) = &self.audit_logger {
                    logger.log(
                        AuditEvent::new(AuditEventType::RateLimitExceeded)
                            .with_user(Uuid::parse_str(user_context.user_id.as_ref().unwrap())?)
                            .with_status("failure".to_string())
                    );
                }
                return Err(e);
            }
        }

        // Parse query
        let parsed_query = crate::graphql::parser::parse_query(query_string)?;

        // Phase 12: Query validation
        self.query_validator.validate(query_string, &parsed_query)?;

        // Execute pipeline (auth, RBAC, SQL, etc.)
        let response = self.execute_sync(query_string, variables, user_context, true)?;

        // Phase 12: Add security headers
        let headers = SecurityHeaders::production().to_vec();

        // Audit log successful query
        if let Some(logger) = &self.audit_logger {
            logger.log(
                AuditEvent::new(AuditEventType::DataRead)
                    .with_resource("graphql".to_string(), "query".to_string())
                    .with_status("success".to_string())
            );
        }

        Ok((response, headers))
    }
}

/// Request metadata for security checks
pub struct RequestInfo {
    pub ip_address: String,
    pub user_agent: String,
    pub referer: Option<String>,
}
```

### Step 7: Python Wrapper (rust_security.py)

```python
"""Rust-based security features (Python wrapper)."""

from fraiseql._fraiseql_rs import (
    PyRateLimiter,
    PySecurityHeaders,
    PyAuditLogger,
    PyQueryValidator,
)


class RustRateLimiter:
    """Rate limiter using Rust implementation."""

    def __init__(self):
        self._rust_limiter = PyRateLimiter()

    def add_rule(self, path: str, requests: int, window_secs: int):
        """Add rate limit rule."""
        self._rust_limiter.add_rule(path, requests, window_secs)

    async def check(self, key: str, path: str) -> bool:
        """Check if request is allowed."""
        return await self._rust_limiter.check(key, path)


class RustSecurityHeaders:
    """Security headers using Rust implementation."""

    @staticmethod
    def production() -> dict[str, str]:
        """Get production security headers."""
        return PySecurityHeaders.production()


class RustAuditLogger:
    """Audit logger using Rust implementation."""

    def __init__(self, pool):
        self._rust_logger = PyAuditLogger(pool)

    def log(self, event_type: str, **kwargs):
        """Log audit event."""
        self._rust_logger.log(event_type, **kwargs)
```

---

## Verification Commands

### Build and Test
```bash
# Build
cargo build --release
maturin develop --release

# Run security tests
pytest tests/test_rust_security.py -xvs
pytest tests/integration/security/ -xvs

# Performance tests
pytest tests/performance/test_security_performance.py -xvs
```

### Expected Performance
```
Rate Limit Check: <0.05ms
Security Headers: <0.01ms
Audit Log (async): <0.5ms
Query Validation: <0.1ms

Total Security Overhead: <1ms
```

---

## Acceptance Criteria

**Functionality:**
- ✅ Token bucket rate limiting
- ✅ Security header enforcement
- ✅ Async audit logging
- ✅ Query validation (depth, complexity, size)
- ✅ CSRF protection
- ✅ All existing security tests pass

**Performance:**
- ✅ Security overhead <1ms total
- ✅ 10-50x faster than Python
- ✅ Async audit logging (non-blocking)

**Testing:**
- ✅ Integration tests pass
- ✅ Performance benchmarks
- ✅ Security hardening tests

---

## DO NOT

❌ **DO NOT** implement DDoS mitigation (use external WAF)
❌ **DO NOT** add encryption (use TLS)
❌ **DO NOT** implement IP allowlisting (config-based)
❌ **DO NOT** add complex threat detection (use SIEM)

---

## Dependencies (Cargo.toml)

```toml
[dependencies]
# Existing...

# Security dependencies (Phase 12)
tokio = { version = "1.35", features = ["sync", "time"] }
rand = "0.8"
hex = "0.4"
```

---

## Migration Strategy

**Week 1: Core Security**
- Rate limiting
- Security headers
- Query validation

**Week 2: Audit Logging**
- Async audit logger
- Event types
- PostgreSQL integration

**Week 3: Production**
- Gradual rollout
- Monitor performance
- Deprecate Python security

---

## Summary

**Phase 12 completes the enterprise security layer:**
- ✅ Rate limiting (DDoS protection)
- ✅ Security headers (XSS, CSRF, clickjacking prevention)
- ✅ Audit logging (compliance)
- ✅ Query validation (resource protection)
- ✅ All security features in Rust for maximum performance

**Combined with Phases 10-11:**
- Complete auth/RBAC/security stack in Rust
- Sub-millisecond security overhead
- Production-ready enterprise hardening
