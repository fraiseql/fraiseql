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
- Integration with Phase 11 RBAC cache invalidation

**Performance Target:**

- Rate limit check: <0.05ms
- Security header injection: <0.01ms
- Audit log write: <0.5ms (async)
- Total security overhead: <1ms

---

## Files to Modify/Create

### Rust Files (fraiseql_rs/src/security/)

- **mod.rs** (NEW): Security module exports
- **config.rs** (NEW): Security configuration management
- **errors.rs** (NEW): Security-specific error types
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
            .ok_or_else(|| SecurityError::SecurityConfigError("No rate limit configured".to_string()))?;

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
            Err(SecurityError::RateLimitExceeded {
                retry_after,
                limit: limit.requests,
                window_secs: limit.window_secs,
            })
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
            Err(SecurityError::RateLimitExceeded {
                retry_after,
                limit: limit.requests,
                window_secs: limit.window_secs,
            })
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
            Err(SecurityError::RateLimitExceeded {
                retry_after,
                limit: limit.requests,
                window_secs: limit.window_secs,
            })
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
        let mut consecutive_errors = 0;
        const MAX_CONSECUTIVE_ERRORS: u32 = 10;

        while let Some(event) = rx.recv().await {
            match Self::write_event(&pool, &event).await {
                Ok(_) => {
                    consecutive_errors = 0; // Reset error counter on success
                }
                Err(e) => {
                    consecutive_errors += 1;
                    eprintln!("Failed to write audit log (attempt {}): {}", consecutive_errors, e);

                    // If too many consecutive errors, log to stderr and continue
                    // In production, this might trigger alerts or fallback logging
                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                        eprintln!("WARNING: {} consecutive audit log failures. Check database connectivity.", consecutive_errors);
                        // Could implement circuit breaker pattern here
                    }

                    // For critical events, could retry with backoff
                    if Self::is_critical_event(&event) && consecutive_errors < 3 {
                        // Simple retry logic for critical events
                        tokio::time::sleep(tokio::time::Duration::from_millis(100 * consecutive_errors as u64)).await;
                        if let Ok(_) = Self::write_event(&pool, &event).await {
                            consecutive_errors = 0;
                        }
                    }
                }
            }
        }
    }

    /// Check if event is critical and should be retried
    fn is_critical_event(event: &AuditEvent) -> bool {
        matches!(event.event_type,
            AuditEventType::LoginFailure |
            AuditEventType::PermissionDenied |
            AuditEventType::SuspiciousActivity |
            AuditEventType::RateLimitExceeded
        )
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
            return Err(SecurityError::QueryTooLarge {
                size: query.len(),
                max_size: self.limits.max_query_size,
            });
        }

        // Check depth
        let depth = self.calculate_depth(parsed);
        if depth > self.limits.max_depth {
            return Err(SecurityError::QueryTooDeep {
                depth,
                max_depth: self.limits.max_depth,
            });
        }

        // Check complexity
        let complexity = self.calculate_complexity(parsed);
        if complexity > self.limits.max_complexity {
            return Err(SecurityError::QueryTooComplex {
                complexity,
                max_complexity: self.limits.max_complexity,
            });
        }

        Ok(())
    }

    /// Calculate query depth (max nesting level)
    fn calculate_depth(&self, query: &ParsedQuery) -> usize {
        query.selections.iter()
            .map(|selection| self.calculate_selection_depth(selection))
            .max()
            .unwrap_or(0)
    }

    /// Calculate depth for a single selection
    fn calculate_selection_depth(&self, selection: &FieldSelection) -> usize {
        if selection.nested_fields.is_empty() {
            1
        } else {
            1 + selection.nested_fields.iter()
                .map(|nested| self.calculate_selection_depth(nested))
                .max()
                .unwrap_or(0)
        }
    }

    /// Calculate query complexity (estimated cost)
    fn calculate_complexity(&self, query: &ParsedQuery) -> usize {
        query.selections.iter()
            .map(|selection| self.calculate_selection_complexity(selection))
            .sum()
    }

    /// Calculate complexity for a single selection
    fn calculate_selection_complexity(&self, selection: &FieldSelection) -> usize {
        let mut complexity = 1; // Base cost for this field

        // Add cost for arguments (indicates filtering/complexity)
        complexity += selection.arguments.len() * 2;

        // Add cost for nested fields (recursive)
        for nested in &selection.nested_fields {
            complexity += self.calculate_selection_complexity(nested);
        }

        // Add cost for list fields (pagination/multiplier)
        if self.is_list_field(selection) {
            complexity *= 10; // Assume pagination limits this
        }

        complexity
    }

    /// Check if field returns a list (affects complexity)
    fn is_list_field(&self, selection: &FieldSelection) -> bool {
        // This would need schema introspection to determine if field returns a list
        // For now, use heuristics based on field name
        let list_indicators = ["list", "all", "many", "items", "edges", "nodes"];
        let field_name = selection.name.to_lowercase();

        list_indicators.iter().any(|&indicator| field_name.contains(indicator)) ||
        field_name.ends_with('s') // Plural names often indicate lists
    }
}
```

### Step 5: CSRF Protection (csrf.rs)

```rust
//! Security-specific error types.

use std::fmt;

/// Main security error type
#[derive(Debug)]
pub enum SecurityError {
    /// Rate limiting errors
    RateLimitExceeded {
        retry_after: u64,
        limit: usize,
        window_secs: u64,
    },

    /// Query validation errors
    QueryTooDeep {
        depth: usize,
        max_depth: usize,
    },

    QueryTooComplex {
        complexity: usize,
        max_complexity: usize,
    },

    QueryTooLarge {
        size: usize,
        max_size: usize,
    },

    /// CORS errors
    OriginNotAllowed(String),
    MethodNotAllowed(String),
    HeaderNotAllowed(String),

    /// CSRF errors
    InvalidCSRFToken(String),
    CSRFSessionMismatch,

    /// Audit logging errors
    AuditLogFailure(String),

    /// Configuration errors
    SecurityConfigError(String),

    /// General security violations
    SecurityViolation(String),
}

pub type Result<T> = std::result::Result<T, SecurityError>;

impl fmt::Display for SecurityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecurityError::RateLimitExceeded { retry_after, limit, window_secs } => {
                write!(f, "Rate limit exceeded. Limit: {} per {} seconds. Retry after: {} seconds",
                       limit, window_secs, retry_after)
            }
            SecurityError::QueryTooDeep { depth, max_depth } => {
                write!(f, "Query too deep: {} levels (max: {})", depth, max_depth)
            }
            SecurityError::QueryTooComplex { complexity, max_complexity } => {
                write!(f, "Query too complex: {} (max: {})", complexity, max_complexity)
            }
            SecurityError::QueryTooLarge { size, max_size } => {
                write!(f, "Query too large: {} bytes (max: {})", size, max_size)
            }
            SecurityError::OriginNotAllowed(origin) => {
                write!(f, "CORS origin not allowed: {}", origin)
            }
            SecurityError::MethodNotAllowed(method) => {
                write!(f, "CORS method not allowed: {}", method)
            }
            SecurityError::HeaderNotAllowed(header) => {
                write!(f, "CORS header not allowed: {}", header)
            }
            SecurityError::InvalidCSRFToken(reason) => {
                write!(f, "Invalid CSRF token: {}", reason)
            }
            SecurityError::CSRFSessionMismatch => {
                write!(f, "CSRF token session mismatch")
            }
            SecurityError::AuditLogFailure(reason) => {
                write!(f, "Audit logging failed: {}", reason)
            }
            SecurityError::SecurityConfigError(reason) => {
                write!(f, "Security configuration error: {}", reason)
            }
            SecurityError::SecurityViolation(reason) => {
                write!(f, "Security violation: {}", reason)
            }
        }
    }
}

impl std::error::Error for SecurityError {}

#[cfg(feature = "python")]
impl From<SecurityError> for pyo3::PyErr {
    fn from(error: SecurityError) -> Self {
        use pyo3::exceptions::*;

        match error {
            SecurityError::RateLimitExceeded { .. } => PyException::new_err(error.to_string()),
            SecurityError::QueryTooDeep { .. } | SecurityError::QueryTooComplex { .. } | SecurityError::QueryTooLarge { .. } => {
                PyValueError::new_err(error.to_string())
            }
            SecurityError::OriginNotAllowed(_) | SecurityError::MethodNotAllowed(_) | SecurityError::HeaderNotAllowed(_) => {
                PyPermissionError::new_err(error.to_string())
            }
            _ => PyRuntimeError::new_err(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_error_display() {
        let err = SecurityError::RateLimitExceeded {
            retry_after: 30,
            limit: 100,
            window_secs: 60,
        };
        assert!(err.to_string().contains("Rate limit exceeded"));
        assert!(err.to_string().contains("100"));
        assert!(err.to_string().contains("30"));
    }
}
```

### Step 5.2: CSRF Protection (csrf.rs)

```rust
//! CSRF token validation.

use crate::security::errors::{Result, SecurityError};
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
            return Err(SecurityError::InvalidCSRFToken("Invalid token format".to_string()));
        }

        let provided_session = parts[0];
        let nonce = parts[1];
        let provided_hash = parts[2];

        // Verify session matches
        if provided_session != session_id {
            return Err(SecurityError::CSRFSessionMismatch);
        }

        // Verify hash
        let payload = format!("{}:{}", provided_session, nonce);
        let mut hasher = Sha256::new();
        hasher.update(payload.as_bytes());
        hasher.update(self.secret.as_bytes());
        let expected_hash = hex::encode(hasher.finalize());

        if expected_hash != provided_hash {
            return Err(SecurityError::InvalidCSRFToken("Hash verification failed".to_string()));
        }

        Ok(())
    }
}
```

### Step 4.5: Security Configuration (config.rs)

```rust
//! Security configuration management.

use crate::security::{
    rate_limit::{RateLimit, RateLimitStrategy, RateLimiter},
    headers::SecurityHeaders,
    audit::AuditLogger,
    validators::{QueryLimits, QueryValidator},
    csrf::CSRFManager,
    cors::{CORSConfig, CORSHandler},
};
use anyhow::Result;
use std::env;

/// Master security configuration
#[derive(Debug)]
pub struct SecurityConfig {
    pub rate_limiting: RateLimitingConfig,
    pub headers: SecurityHeadersConfig,
    pub audit: AuditConfig,
    pub query_validation: QueryValidationConfig,
    pub csrf: CSRFConfig,
    pub cors: CORSConfig,
}

#[derive(Debug)]
pub struct RateLimitingConfig {
    pub enabled: bool,
    pub default_limit: RateLimit,
    pub endpoint_limits: Vec<(String, RateLimit)>,
}

#[derive(Debug)]
pub struct SecurityHeadersConfig {
    pub enabled: bool,
    pub environment: String, // "development" | "production"
}

#[derive(Debug)]
pub struct AuditConfig {
    pub enabled: bool,
    pub database_url: Option<String>,
}

#[derive(Debug)]
pub struct QueryValidationConfig {
    pub enabled: bool,
    pub limits: QueryLimits,
}

#[derive(Debug)]
pub struct CSRFConfig {
    pub enabled: bool,
    pub secret: Option<String>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            rate_limiting: RateLimitingConfig {
                enabled: true,
                default_limit: RateLimit {
                    requests: 100,
                    window_secs: 60,
                    burst: Some(20),
                    strategy: RateLimitStrategy::TokenBucket,
                },
                endpoint_limits: vec![
                    ("/graphql".to_string(), RateLimit {
                        requests: 1000,
                        window_secs: 60,
                        burst: Some(100),
                        strategy: RateLimitStrategy::TokenBucket,
                    }),
                ],
            },
            headers: SecurityHeadersConfig {
                enabled: true,
                environment: "development".to_string(),
            },
            audit: AuditConfig {
                enabled: true,
                database_url: None,
            },
            query_validation: QueryValidationConfig {
                enabled: true,
                limits: QueryLimits::default(),
            },
            csrf: CSRFConfig {
                enabled: false, // Disabled by default for API-first apps
                secret: None,
            },
            cors: CORSConfig::default(),
        }
    }
}

impl SecurityConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self> {
        let mut config = Self::default();

        // Rate limiting
        if let Ok(enabled) = env::var("SECURITY_RATE_LIMITING_ENABLED") {
            config.rate_limiting.enabled = enabled.parse().unwrap_or(true);
        }

        if let Ok(requests) = env::var("SECURITY_RATE_LIMIT_REQUESTS") {
            config.rate_limiting.default_limit.requests = requests.parse().unwrap_or(100);
        }

        if let Ok(window) = env::var("SECURITY_RATE_LIMIT_WINDOW") {
            config.rate_limiting.default_limit.window_secs = window.parse().unwrap_or(60);
        }

        // Security headers
        if let Ok(env) = env::var("SECURITY_HEADERS_ENV") {
            config.headers.environment = env;
        }

        // Audit logging
        if let Ok(enabled) = env::var("SECURITY_AUDIT_ENABLED") {
            config.audit.enabled = enabled.parse().unwrap_or(true);
        }

        // Query validation
        if let Ok(max_depth) = env::var("SECURITY_QUERY_MAX_DEPTH") {
            config.query_validation.limits.max_depth = max_depth.parse().unwrap_or(10);
        }

        if let Ok(max_complexity) = env::var("SECURITY_QUERY_MAX_COMPLEXITY") {
            config.query_validation.limits.max_complexity = max_complexity.parse().unwrap_or(1000);
        }

        // CSRF
        if let Ok(enabled) = env::var("SECURITY_CSRF_ENABLED") {
            config.csrf.enabled = enabled.parse().unwrap_or(false);
        }

        if let Ok(secret) = env::var("SECURITY_CSRF_SECRET") {
            config.csrf.secret = Some(secret);
        }

        // CORS
        if let Ok(origins) = env::var("SECURITY_CORS_ORIGINS") {
            config.cors.allowed_origins = origins.split(',')
                .map(|s| s.trim().to_string())
                .collect();
        }

        Ok(config)
    }

    /// Create production configuration
    pub fn production() -> Self {
        let mut config = Self::default();

        config.rate_limiting.default_limit.requests = 50; // Stricter limits
        config.headers.environment = "production".to_string();
        config.query_validation.limits = QueryLimits::production();
        config.cors = CORSConfig::production();

        config
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        // Validate CSRF secret if enabled
        if self.csrf.enabled && self.csrf.secret.is_none() {
            return Err(anyhow::anyhow!("CSRF secret must be provided when CSRF is enabled"));
        }

        // Validate audit database URL if enabled
        if self.audit.enabled && self.audit.database_url.is_none() {
            return Err(anyhow::anyhow!("Database URL must be provided when audit logging is enabled"));
        }

        // Validate rate limits
        if self.rate_limiting.default_limit.requests == 0 {
            return Err(anyhow::anyhow!("Rate limit requests must be greater than 0"));
        }

        Ok(())
    }
}

/// Security components builder
pub struct SecurityComponents {
    pub rate_limiter: Option<RateLimiter>,
    pub security_headers: SecurityHeaders,
    pub audit_logger: Option<AuditLogger>,
    pub query_validator: QueryValidator,
    pub csrf_manager: Option<CSRFManager>,
    pub cors_handler: CORSHandler,
}

impl SecurityComponents {
    /// Build security components from configuration
    pub async fn from_config(config: &SecurityConfig, pool: Option<sqlx::PgPool>) -> Result<Self> {
        // Rate limiter
        let rate_limiter = if config.rate_limiting.enabled {
            let mut limiter = RateLimiter::new();

            // Add default limit
            limiter.add_rule("*".to_string(), config.rate_limiting.default_limit.clone());

            // Add endpoint-specific limits
            for (endpoint, limit) in &config.rate_limiting.endpoint_limits {
                limiter.add_rule(endpoint.clone(), limit.clone());
            }

            Some(limiter)
        } else {
            None
        };

        // Security headers
        let security_headers = if config.headers.environment == "production" {
            SecurityHeaders::production()
        } else {
            SecurityHeaders::default()
        };

        // Audit logger
        let audit_logger = if config.audit.enabled {
            pool.map(AuditLogger::new)
        } else {
            None
        };

        // Query validator
        let query_validator = if config.query_validation.enabled {
            QueryValidator::new(config.query_validation.limits.clone())
        } else {
            QueryValidator::new(QueryLimits::default())
        };

        // CSRF manager
        let csrf_manager = if config.csrf.enabled {
            config.csrf.secret.as_ref().map(|secret| CSRFManager::new(secret.clone()))
        } else {
            None
        };

        // CORS handler
        let cors_handler = CORSHandler::new(config.cors.clone());

        Ok(Self {
            rate_limiter,
            security_headers,
            audit_logger,
            query_validator,
            csrf_manager,
            cors_handler,
        })
    }
}
```

### Step 5.5: CORS Policy Enforcement (cors.rs)

```rust
//! CORS (Cross-Origin Resource Sharing) policy enforcement.

use anyhow::{Result, anyhow};
use std::collections::HashSet;

/// CORS configuration
#[derive(Debug, Clone)]
pub struct CORSConfig {
    /// Allowed origins (exact matches or patterns)
    pub allowed_origins: HashSet<String>,
    /// Allowed HTTP methods
    pub allowed_methods: HashSet<String>,
    /// Allowed headers
    pub allowed_headers: HashSet<String>,
    /// Headers exposed to browser
    pub exposed_headers: HashSet<String>,
    /// Whether credentials are allowed
    pub allow_credentials: bool,
    /// Max age for preflight cache (seconds)
    pub max_age: u32,
}

impl Default for CORSConfig {
    fn default() -> Self {
        let mut allowed_origins = HashSet::new();
        allowed_origins.insert("http://localhost:3000".to_string());
        allowed_origins.insert("http://localhost:3001".to_string());

        let mut allowed_methods = HashSet::new();
        allowed_methods.insert("GET".to_string());
        allowed_methods.insert("POST".to_string());
        allowed_methods.insert("OPTIONS".to_string());

        let mut allowed_headers = HashSet::new();
        allowed_headers.insert("Content-Type".to_string());
        allowed_headers.insert("Authorization".to_string());
        allowed_headers.insert("X-Requested-With".to_string());

        let mut exposed_headers = HashSet::new();
        exposed_headers.insert("X-Total-Count".to_string());
        exposed_headers.insert("X-Rate-Limit-Remaining".to_string());

        Self {
            allowed_origins,
            allowed_methods,
            allowed_headers,
            exposed_headers,
            allow_credentials: false,
            max_age: 86400, // 24 hours
        }
    }
}

impl CORSConfig {
    /// Create production CORS config
    pub fn production() -> Self {
        let mut config = Self::default();
        config.allowed_origins.clear(); // Must be explicitly configured
        config.allow_credentials = true;
        config
    }

    /// Check if origin is allowed
    pub fn is_origin_allowed(&self, origin: &str) -> bool {
        self.allowed_origins.contains(origin) ||
        self.allowed_origins.contains("*")
    }

    /// Check if method is allowed
    pub fn is_method_allowed(&self, method: &str) -> bool {
        self.allowed_methods.contains(method) ||
        self.allowed_methods.contains("*")
    }

    /// Check if header is allowed
    pub fn is_header_allowed(&self, header: &str) -> bool {
        self.allowed_headers.contains(header) ||
        self.allowed_headers.contains("*")
    }
}

/// CORS policy enforcer
pub struct CORSHandler {
    config: CORSConfig,
}

impl CORSHandler {
    pub fn new(config: CORSConfig) -> Self {
        Self { config }
    }

    /// Handle CORS preflight request
    pub fn handle_preflight(
        &self,
        origin: Option<&str>,
        method: Option<&str>,
        headers: Option<&str>,
    ) -> Result<Vec<(String, String)>> {
        let mut response_headers = Vec::new();

        // Validate origin
        if let Some(origin) = origin {
            if !self.config.is_origin_allowed(origin) {
                return Err(SecurityError::OriginNotAllowed(origin.to_string()));
            }
            response_headers.push(("Access-Control-Allow-Origin".to_string(), origin.to_string()));
        }

        // Validate method
        if let Some(method) = method {
            if !self.config.is_method_allowed(method) {
                return Err(SecurityError::MethodNotAllowed(method.to_string()));
            }
            response_headers.push(("Access-Control-Allow-Methods".to_string(),
                                 self.config.allowed_methods.iter().cloned().collect::<Vec<_>>().join(", ")));
        }

        // Validate headers
        if let Some(request_headers) = headers {
            let requested_headers: Vec<&str> = request_headers.split(',').map(|s| s.trim()).collect();
            for header in &requested_headers {
                if !self.config.is_header_allowed(header) {
                    return Err(SecurityError::HeaderNotAllowed(header.to_string()));
                }
            }
            response_headers.push(("Access-Control-Allow-Headers".to_string(), request_headers.to_string()));
        }

        // Add other CORS headers
        if self.config.allow_credentials {
            response_headers.push(("Access-Control-Allow-Credentials".to_string(), "true".to_string()));
        }

        response_headers.push(("Access-Control-Max-Age".to_string(), self.config.max_age.to_string()));

        Ok(response_headers)
    }

    /// Add CORS headers to response
    pub fn add_cors_headers(
        &self,
        origin: Option<&str>,
        mut headers: Vec<(String, String)>,
    ) -> Vec<(String, String)> {
        if let Some(origin) = origin {
            if self.config.is_origin_allowed(origin) {
                headers.push(("Access-Control-Allow-Origin".to_string(), origin.to_string()));

                if self.config.allow_credentials {
                    headers.push(("Access-Control-Allow-Credentials".to_string(), "true".to_string()));
                }

                if !self.config.exposed_headers.is_empty() {
                    headers.push(("Access-Control-Expose-Headers".to_string(),
                                self.config.exposed_headers.iter().cloned().collect::<Vec<_>>().join(", ")));
                }
            }
        }

        headers
    }

    /// Check if request is a CORS preflight
    pub fn is_preflight_request(method: &str, headers: &http::HeaderMap) -> bool {
        method == "OPTIONS" &&
        headers.contains_key("origin") &&
        (headers.contains_key("access-control-request-method") ||
         headers.contains_key("access-control-request-headers"))
    }
}
```

### Step 5.8: RBAC Cache Integration

```rust
// Integration with Phase 11 RBAC cache invalidation

use crate::rbac::cache::{PermissionCache, CacheInvalidation};

/// Security event handler that triggers RBAC cache invalidation
pub struct SecurityEventHandler {
    rbac_cache: Arc<PermissionCache>,
}

impl SecurityEventHandler {
    pub fn new(rbac_cache: Arc<PermissionCache>) -> Self {
        Self { rbac_cache }
    }

    /// Handle security events that may affect RBAC caching
    pub fn handle_security_event(&self, event: &AuditEvent) {
        match event.event_type {
            AuditEventType::RoleAssigned | AuditEventType::RoleRevoked => {
                // User role changed - invalidate their permission cache
                if let Some(user_id) = event.user_id {
                    CacheInvalidation::on_user_role_change(&self.rbac_cache, user_id);
                }
            }
            AuditEventType::PermissionGranted | AuditEventType::PermissionDenied => {
                // Permission changed - invalidate affected caches
                if let Some(user_id) = event.user_id {
                    CacheInvalidation::on_user_role_change(&self.rbac_cache, user_id);
                }
                // Could also invalidate by role/permission if we had reverse index
            }
            AuditEventType::LoginSuccess => {
                // User logged in - ensure fresh permissions on next request
                if let Some(user_id) = event.user_id {
                    // Optional: pre-warm cache or just let it load on demand
                }
            }
            _ => {
                // Other events don't affect RBAC caching
            }
        }
    }
}

// In AuditLogger, integrate with RBAC cache
impl AuditLogger {
    pub fn with_rbac_cache(mut self, rbac_cache: Arc<PermissionCache>) -> Self {
        self.rbac_event_handler = Some(SecurityEventHandler::new(rbac_cache));
        self
    }

    pub fn log(&self, event: AuditEvent) {
        // Handle RBAC cache invalidation for security events
        if let Some(handler) = &self.rbac_event_handler {
            handler.handle_security_event(&event);
        }

        // Continue with normal async logging
        let _ = self.tx.send(event);
    }
}
```

### Step 6: Integration with Pipeline (unified.rs)

```rust
// Add security layer to execute_sync()

use crate::security::{
    config::SecurityComponents,
    audit::{AuditEvent, AuditEventType},
};

pub struct GraphQLPipeline {
    schema: SchemaMetadata,
    cache: Arc<QueryPlanCache>,
    rbac_resolver: Option<Arc<PermissionResolver>>,
    security: Option<Arc<SecurityComponents>>,  // NEW: Unified security components
}

impl GraphQLPipeline {
    pub fn with_security(mut self, security: SecurityComponents) -> Self {
        self.security = Some(Arc::new(security));
        self
    }

    pub async fn execute_with_security(
        &self,
        query_string: &str,
        variables: HashMap<String, JsonValue>,
        user_context: UserContext,
        request_info: RequestInfo,  // IP, user agent, etc.
    ) -> Result<(Vec<u8>, Vec<(String, String)>)> {  // (response, headers)
        let security = self.security.as_ref()
            .ok_or("Security components not configured")?;

        // Phase 12: Rate limiting
        if let Some(limiter) = &security.rate_limiter {
            let rate_key = format!("user:{}", user_context.user_id.as_ref().unwrap_or(&"anonymous".to_string()));
            if let Err(e) = limiter.check(&rate_key, "/graphql").await {
                // Log rate limit event
                if let Some(logger) = &security.audit_logger {
                    let user_id = user_context.user_id.as_ref()
                        .and_then(|id| Uuid::parse_str(id).ok());

                    let mut event = AuditEvent::new(AuditEventType::RateLimitExceeded)
                        .with_status("failure".to_string());

                    if let Some(user_id) = user_id {
                        event = event.with_user(user_id);
                    }

                    event = event.with_resource("rate_limit".to_string(), "/graphql".to_string());
                    event = event.with_metadata(serde_json::json!({
                        "ip_address": request_info.ip_address,
                        "user_agent": request_info.user_agent,
                    }));

                    logger.log(event);
                }
                return Err(e);
            }
        }

        // Parse query
        let parsed_query = crate::graphql::parser::parse_query(query_string)?;

        // Phase 12: Query validation
        security.query_validator.validate(query_string, &parsed_query)?;

        // Execute pipeline (auth, RBAC, SQL, etc.)
        let response = self.execute_sync(query_string, variables, user_context, true)?;

        // Phase 12: Add security headers
        let mut headers = security.security_headers.to_vec();

        // Add CORS headers if applicable
        headers = security.cors_handler.add_cors_headers(
            request_info.referer.as_ref().map(|s| s.as_str()),
            headers
        );

        // Audit log successful query
        if let Some(logger) = &security.audit_logger {
            let user_id = user_context.user_id.as_ref()
                .and_then(|id| Uuid::parse_str(id).ok());

            let mut event = AuditEvent::new(AuditEventType::DataRead)
                .with_resource("graphql".to_string(), "query".to_string())
                .with_metadata(serde_json::json!({
                    "query_complexity": security.query_validator.calculate_complexity(&parsed_query),
                    "query_depth": security.query_validator.calculate_depth(&parsed_query),
                }))
                .with_status("success".to_string());

            if let Some(user_id) = user_id {
                event = event.with_user(user_id);
            }

            logger.log(event);
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
sha2 = "0.10"  # For CSRF token hashing
uuid = { version = "1.6", features = ["v4", "serde"] }  # For audit events
chrono = { version = "0.4", features = ["serde"] }  # For timestamps
serde_json = "1.0"  # For audit metadata
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
