# Phase 2: Core Runtime Features

## Objective

Implement the core runtime features that are always enabled: rate limiting with backpressure, CORS, health checks, metrics with SLO tracking, and request tracing. All components should be testable through dependency injection.

---

## 2.1 Rate Limiting with Backpressure

### Task: Define rate limiting configuration

```rust
// crates/fraiseql-runtime/src/config/rate_limiting.rs

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitingConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Default rate limit (e.g., "100/minute")
    pub default: String,

    /// Storage backend: memory, redis
    #[serde(default = "default_backend")]
    pub backend: String,

    /// Redis URL (if using redis storage)
    pub redis_url_env: Option<String>,

    /// Custom rules
    #[serde(default)]
    pub rules: Vec<RateLimitRule>,

    /// Backpressure configuration
    #[serde(default)]
    pub backpressure: BackpressureConfig,
}

fn default_enabled() -> bool { true }
fn default_backend() -> String { "memory".to_string() }

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitRule {
    /// Match by path pattern (e.g., "/auth/*")
    pub path: Option<String>,

    /// Match by mutation name
    pub mutation: Option<String>,

    /// Match by query name
    pub query: Option<String>,

    /// Limit (e.g., "10/minute", "100/hour")
    pub limit: String,

    /// Key extraction: ip, user, api_key, composite
    #[serde(default = "default_key_by")]
    pub by: String,

    /// Burst allowance (requests above limit that can be queued)
    #[serde(default)]
    pub burst: Option<u32>,
}

fn default_key_by() -> String { "ip".to_string() }

#[derive(Debug, Clone, Deserialize)]
pub struct BackpressureConfig {
    /// Enable request queuing when at limit
    #[serde(default)]
    pub queue_enabled: bool,

    /// Maximum queue size per key
    #[serde(default = "default_queue_size")]
    pub max_queue_size: usize,

    /// Maximum time to wait in queue
    #[serde(default = "default_queue_timeout")]
    pub queue_timeout: String,

    /// Shed load when queue is full (503 vs queue)
    #[serde(default = "default_load_shed")]
    pub load_shed: bool,
}

impl Default for BackpressureConfig {
    fn default() -> Self {
        Self {
            queue_enabled: false,
            max_queue_size: 100,
            queue_timeout: "5s".to_string(),
            load_shed: true,
        }
    }
}

fn default_queue_size() -> usize { 100 }
fn default_queue_timeout() -> String { "5s".to_string() }
fn default_load_shed() -> bool { true }
```

### Task: Define rate limiter trait for testability

```rust
// crates/fraiseql-runtime/src/middleware/rate_limit/mod.rs

use std::time::{Duration, Instant, SystemTime};
use async_trait::async_trait;

/// Parsed rate limit
#[derive(Debug, Clone)]
pub struct RateLimit {
    pub requests: u32,
    pub window: Duration,
    pub burst: Option<u32>,
}

impl RateLimit {
    /// Parse "100/minute" format
    pub fn parse(s: &str) -> Result<Self, ParseError> {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() != 2 {
            return Err(ParseError::InvalidFormat { value: s.to_string() });
        }

        let requests: u32 = parts[0].parse()
            .map_err(|_| ParseError::InvalidNumber { value: parts[0].to_string() })?;

        let window = match parts[1].to_lowercase().as_str() {
            "second" | "sec" | "s" => Duration::from_secs(1),
            "minute" | "min" | "m" => Duration::from_secs(60),
            "hour" | "hr" | "h" => Duration::from_secs(3600),
            "day" | "d" => Duration::from_secs(86400),
            _ => return Err(ParseError::InvalidPeriod { value: parts[1].to_string() }),
        };

        Ok(Self { requests, window, burst: None })
    }

    pub fn with_burst(mut self, burst: u32) -> Self {
        self.burst = Some(burst);
        self
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Invalid rate limit format: {value} (expected 'N/period')")]
    InvalidFormat { value: String },

    #[error("Invalid number in rate limit: {value}")]
    InvalidNumber { value: String },

    #[error("Invalid period in rate limit: {value} (expected second/minute/hour/day)")]
    InvalidPeriod { value: String },
}

/// Result of a rate limit check
#[derive(Debug, Clone)]
pub enum RateLimitResult {
    /// Request is allowed
    Allowed {
        remaining: u32,
        limit: u32,
        reset_at: SystemTime,
    },
    /// Request should be queued (backpressure)
    Queued {
        position: usize,
        estimated_wait: Duration,
    },
    /// Request is rate limited
    Limited {
        retry_after: Duration,
        limit: u32,
    },
    /// System is overloaded, shed load
    Overloaded,
}

/// Trait for rate limiter implementations (injectable for testing)
#[async_trait]
pub trait RateLimiter: Send + Sync {
    /// Check if request is allowed
    async fn check(&self, key: &str, limit: &RateLimit) -> RateLimitResult;

    /// Record a request (after processing, for sliding window)
    async fn record(&self, key: &str, limit: &RateLimit);

    /// Get current state for a key (for metrics/debugging)
    async fn get_state(&self, key: &str) -> Option<RateLimitState>;
}

#[derive(Debug, Clone)]
pub struct RateLimitState {
    pub current_count: u32,
    pub window_start: SystemTime,
    pub queue_depth: usize,
}

/// Mock rate limiter for testing
#[cfg(any(test, feature = "testing"))]
pub struct MockRateLimiter {
    pub results: std::sync::Arc<std::sync::Mutex<Vec<RateLimitResult>>>,
    pub calls: std::sync::Arc<std::sync::Mutex<Vec<(String, RateLimit)>>>,
}

#[cfg(any(test, feature = "testing"))]
impl MockRateLimiter {
    pub fn new() -> Self {
        Self {
            results: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    pub fn with_results(results: Vec<RateLimitResult>) -> Self {
        Self {
            results: std::sync::Arc::new(std::sync::Mutex::new(results)),
            calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }
}

#[cfg(any(test, feature = "testing"))]
#[async_trait]
impl RateLimiter for MockRateLimiter {
    async fn check(&self, key: &str, limit: &RateLimit) -> RateLimitResult {
        self.calls.lock().unwrap().push((key.to_string(), limit.clone()));
        self.results.lock().unwrap().pop().unwrap_or(RateLimitResult::Allowed {
            remaining: limit.requests,
            limit: limit.requests,
            reset_at: SystemTime::now() + limit.window,
        })
    }

    async fn record(&self, _key: &str, _limit: &RateLimit) {}

    async fn get_state(&self, _key: &str) -> Option<RateLimitState> {
        None
    }
}
```

### Task: Implement in-memory rate limiter with sliding window

```rust
// crates/fraiseql-runtime/src/middleware/rate_limit/memory.rs

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::{RwLock, Semaphore};
use tokio::time::timeout;

use super::{RateLimiter, RateLimit, RateLimitResult, RateLimitState, BackpressureConfig};

/// In-memory rate limiter using sliding window with backpressure support
pub struct MemoryRateLimiter {
    windows: RwLock<HashMap<String, SlidingWindow>>,
    config: BackpressureConfig,
    /// Cleanup task interval
    cleanup_interval: Duration,
}

struct SlidingWindow {
    /// Timestamps of requests in the window
    requests: Vec<Instant>,
    /// Semaphore for queueing (backpressure)
    queue: Option<Arc<Semaphore>>,
    /// Current queue depth
    queue_depth: std::sync::atomic::AtomicUsize,
}

impl SlidingWindow {
    fn new(config: &BackpressureConfig) -> Self {
        let queue = if config.queue_enabled {
            Some(Arc::new(Semaphore::new(0))) // Start with no permits
        } else {
            None
        };

        Self {
            requests: Vec::new(),
            queue,
            queue_depth: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn count_in_window(&self, window: Duration) -> u32 {
        let now = Instant::now();
        let cutoff = now - window;
        self.requests.iter().filter(|&&t| t > cutoff).count() as u32
    }

    fn cleanup(&mut self, window: Duration) {
        let now = Instant::now();
        let cutoff = now - window;
        self.requests.retain(|&t| t > cutoff);
    }

    fn record(&mut self) {
        self.requests.push(Instant::now());
    }

    fn remaining(&self, limit: u32, window: Duration) -> u32 {
        limit.saturating_sub(self.count_in_window(window))
    }

    fn reset_at(&self, window: Duration) -> SystemTime {
        if let Some(&oldest) = self.requests.first() {
            let reset_instant = oldest + window;
            let now = Instant::now();
            if reset_instant > now {
                SystemTime::now() + (reset_instant - now)
            } else {
                SystemTime::now()
            }
        } else {
            SystemTime::now() + window
        }
    }
}

impl MemoryRateLimiter {
    pub fn new(config: BackpressureConfig) -> Self {
        let limiter = Self {
            windows: RwLock::new(HashMap::new()),
            config,
            cleanup_interval: Duration::from_secs(60),
        };

        // Spawn cleanup task
        let windows = limiter.windows.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                let mut windows = windows.write().await;
                // Remove empty windows
                windows.retain(|_, w| !w.requests.is_empty());
            }
        });

        limiter
    }
}

#[async_trait::async_trait]
impl RateLimiter for MemoryRateLimiter {
    async fn check(&self, key: &str, limit: &RateLimit) -> RateLimitResult {
        let effective_limit = limit.burst.unwrap_or(limit.requests);

        let mut windows = self.windows.write().await;
        let window = windows
            .entry(key.to_string())
            .or_insert_with(|| SlidingWindow::new(&self.config));

        // Cleanup old requests
        window.cleanup(limit.window);

        let current_count = window.count_in_window(limit.window);

        if current_count < effective_limit {
            // Under limit, allow
            RateLimitResult::Allowed {
                remaining: effective_limit - current_count - 1,
                limit: limit.requests,
                reset_at: window.reset_at(limit.window),
            }
        } else if self.config.queue_enabled {
            // At limit but queueing is enabled
            let queue_depth = window.queue_depth.load(std::sync::atomic::Ordering::SeqCst);

            if queue_depth >= self.config.max_queue_size {
                if self.config.load_shed {
                    RateLimitResult::Overloaded
                } else {
                    RateLimitResult::Limited {
                        retry_after: window.reset_at(limit.window)
                            .duration_since(SystemTime::now())
                            .unwrap_or(limit.window),
                        limit: limit.requests,
                    }
                }
            } else {
                // Queue the request
                window.queue_depth.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                RateLimitResult::Queued {
                    position: queue_depth + 1,
                    estimated_wait: Duration::from_millis(
                        (queue_depth as u64 + 1) * (limit.window.as_millis() as u64 / limit.requests as u64)
                    ),
                }
            }
        } else {
            // Rate limited
            RateLimitResult::Limited {
                retry_after: window.reset_at(limit.window)
                    .duration_since(SystemTime::now())
                    .unwrap_or(limit.window),
                limit: limit.requests,
            }
        }
    }

    async fn record(&self, key: &str, _limit: &RateLimit) {
        let mut windows = self.windows.write().await;
        if let Some(window) = windows.get_mut(key) {
            window.record();
        }
    }

    async fn get_state(&self, key: &str) -> Option<RateLimitState> {
        let windows = self.windows.read().await;
        windows.get(key).map(|w| RateLimitState {
            current_count: w.requests.len() as u32,
            window_start: SystemTime::now(), // Simplified
            queue_depth: w.queue_depth.load(std::sync::atomic::Ordering::SeqCst),
        })
    }
}
```

### Task: Implement Redis rate limiter

```rust
// crates/fraiseql-runtime/src/middleware/rate_limit/redis.rs

use redis::AsyncCommands;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{RateLimiter, RateLimit, RateLimitResult, RateLimitState};
use fraiseql_error::RuntimeError;

pub struct RedisRateLimiter {
    client: redis::Client,
    key_prefix: String,
}

impl RedisRateLimiter {
    pub async fn new(redis_url: &str) -> Result<Self, RuntimeError> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| RuntimeError::Integration(fraiseql_error::IntegrationError::ConnectionFailed {
                service: format!("redis: {}", e),
            }))?;

        // Test connection
        let mut conn = client.get_async_connection().await
            .map_err(|e| RuntimeError::Integration(fraiseql_error::IntegrationError::ConnectionFailed {
                service: format!("redis: {}", e),
            }))?;

        let _: String = redis::cmd("PING").query_async(&mut conn).await
            .map_err(|e| RuntimeError::Integration(fraiseql_error::IntegrationError::ConnectionFailed {
                service: format!("redis ping: {}", e),
            }))?;

        Ok(Self {
            client,
            key_prefix: "fraiseql:ratelimit".to_string(),
        })
    }

    fn make_key(&self, key: &str) -> String {
        format!("{}:{}", self.key_prefix, key)
    }
}

#[async_trait::async_trait]
impl RateLimiter for RedisRateLimiter {
    async fn check(&self, key: &str, limit: &RateLimit) -> RateLimitResult {
        let redis_key = self.make_key(key);
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let window_start_ms = now_ms - limit.window.as_millis() as i64;

        let mut conn = match self.client.get_async_connection().await {
            Ok(c) => c,
            Err(_) => return RateLimitResult::Overloaded, // Fail open or closed based on policy
        };

        // Lua script for atomic sliding window check
        let script = redis::Script::new(r#"
            local key = KEYS[1]
            local now = tonumber(ARGV[1])
            local window_start = tonumber(ARGV[2])
            local limit = tonumber(ARGV[3])
            local window_ms = tonumber(ARGV[4])

            -- Remove old entries
            redis.call('ZREMRANGEBYSCORE', key, '-inf', window_start)

            -- Count current entries
            local count = redis.call('ZCARD', key)

            if count < limit then
                -- Add new entry
                redis.call('ZADD', key, now, now)
                redis.call('PEXPIRE', key, window_ms)
                return {1, limit - count - 1, 0}
            else
                -- Get oldest entry for reset time
                local oldest = redis.call('ZRANGE', key, 0, 0, 'WITHSCORES')
                local reset_at = 0
                if #oldest > 0 then
                    reset_at = oldest[2] + window_ms
                end
                return {0, 0, reset_at}
            end
        "#);

        let result: Vec<i64> = match script
            .key(&redis_key)
            .arg(now_ms)
            .arg(window_start_ms)
            .arg(limit.requests as i64)
            .arg(limit.window.as_millis() as i64)
            .invoke_async(&mut conn)
            .await
        {
            Ok(r) => r,
            Err(_) => return RateLimitResult::Overloaded,
        };

        if result[0] == 1 {
            RateLimitResult::Allowed {
                remaining: result[1] as u32,
                limit: limit.requests,
                reset_at: SystemTime::now() + limit.window,
            }
        } else {
            let reset_at_ms = result[2];
            let retry_after = if reset_at_ms > now_ms {
                Duration::from_millis((reset_at_ms - now_ms) as u64)
            } else {
                limit.window
            };

            RateLimitResult::Limited {
                retry_after,
                limit: limit.requests,
            }
        }
    }

    async fn record(&self, _key: &str, _limit: &RateLimit) {
        // Recording is done atomically in check() for Redis
    }

    async fn get_state(&self, key: &str) -> Option<RateLimitState> {
        let redis_key = self.make_key(key);
        let mut conn = self.client.get_async_connection().await.ok()?;
        let count: i64 = conn.zcard(&redis_key).await.ok()?;

        Some(RateLimitState {
            current_count: count as u32,
            window_start: SystemTime::now(),
            queue_depth: 0, // Redis doesn't track queue depth
        })
    }
}
```

### Task: Implement rate limit middleware

```rust
// crates/fraiseql-runtime/src/middleware/rate_limit/middleware.rs

use std::sync::Arc;
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{Response, IntoResponse},
    http::{StatusCode, HeaderValue},
};

use crate::state::AppState;
use super::{RateLimiter, RateLimit, RateLimitResult};
use fraiseql_error::RuntimeError;

/// Rate limiting middleware
pub async fn rate_limit_middleware(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, RuntimeError> {
    let rate_limiter = match &state.rate_limiter {
        Some(rl) => rl,
        None => return Ok(next.run(req).await),
    };

    // Find matching rule and get limit
    let (limit, rule_name) = find_matching_rule(&state.config.rate_limiting, &req);

    // Extract key for rate limiting
    let key = extract_rate_limit_key(&req, &state.config.rate_limiting);
    let full_key = format!("{}:{}", rule_name, key);

    // Check rate limit
    match rate_limiter.check(&full_key, &limit).await {
        RateLimitResult::Allowed { remaining, limit: max, reset_at } => {
            // Record the request
            rate_limiter.record(&full_key, &limit).await;

            // Record metrics
            metrics::counter!("rate_limit_requests_total", "status" => "allowed", "rule" => rule_name.clone()).increment(1);

            let mut response = next.run(req).await;

            // Add rate limit headers
            add_rate_limit_headers(&mut response, remaining, max, reset_at);

            Ok(response)
        }

        RateLimitResult::Queued { position, estimated_wait } => {
            // Record metrics
            metrics::counter!("rate_limit_requests_total", "status" => "queued", "rule" => rule_name.clone()).increment(1);
            metrics::gauge!("rate_limit_queue_depth", "rule" => rule_name.clone()).set(position as f64);

            // Wait in queue (simplified - real implementation would use semaphore)
            tokio::time::sleep(estimated_wait).await;

            // Try again after waiting
            let result = rate_limiter.check(&full_key, &limit).await;
            match result {
                RateLimitResult::Allowed { remaining, limit: max, reset_at } => {
                    rate_limiter.record(&full_key, &limit).await;
                    let mut response = next.run(req).await;
                    add_rate_limit_headers(&mut response, remaining, max, reset_at);
                    Ok(response)
                }
                _ => {
                    // Still limited after waiting
                    Err(RuntimeError::RateLimited { retry_after: Some(estimated_wait.as_secs()) })
                }
            }
        }

        RateLimitResult::Limited { retry_after, limit: max } => {
            // Record metrics
            metrics::counter!("rate_limit_requests_total", "status" => "limited", "rule" => rule_name.clone()).increment(1);

            Err(RuntimeError::RateLimited { retry_after: Some(retry_after.as_secs()) })
        }

        RateLimitResult::Overloaded => {
            // Record metrics
            metrics::counter!("rate_limit_requests_total", "status" => "overloaded", "rule" => rule_name).increment(1);

            Err(RuntimeError::ServiceUnavailable {
                reason: "System overloaded".to_string(),
                retry_after: Some(5),
            })
        }
    }
}

fn add_rate_limit_headers(response: &mut Response, remaining: u32, limit: u32, reset_at: std::time::SystemTime) {
    let headers = response.headers_mut();

    headers.insert(
        "X-RateLimit-Limit",
        HeaderValue::from_str(&limit.to_string()).unwrap()
    );
    headers.insert(
        "X-RateLimit-Remaining",
        HeaderValue::from_str(&remaining.to_string()).unwrap()
    );

    if let Ok(duration) = reset_at.duration_since(std::time::UNIX_EPOCH) {
        headers.insert(
            "X-RateLimit-Reset",
            HeaderValue::from_str(&duration.as_secs().to_string()).unwrap()
        );
    }
}

fn find_matching_rule(config: &Option<RateLimitingConfig>, req: &Request) -> (RateLimit, String) {
    let config = match config {
        Some(c) => c,
        None => return (RateLimit { requests: 1000, window: std::time::Duration::from_secs(60), burst: None }, "default".to_string()),
    };

    for rule in &config.rules {
        let matches = if let Some(path) = &rule.path {
            glob::Pattern::new(path)
                .map(|p| p.matches(req.uri().path()))
                .unwrap_or(false)
        } else {
            false
        };

        if matches {
            let limit = RateLimit::parse(&rule.limit).unwrap_or(RateLimit {
                requests: 100,
                window: std::time::Duration::from_secs(60),
                burst: rule.burst,
            });
            let name = rule.path.clone().unwrap_or_else(|| "custom".to_string());
            return (limit, name);
        }
    }

    // Return default
    let default = RateLimit::parse(&config.default).unwrap_or(RateLimit {
        requests: 100,
        window: std::time::Duration::from_secs(60),
        burst: None,
    });
    ("default".to_string(), default)
}

fn extract_rate_limit_key(req: &Request, config: &Option<RateLimitingConfig>) -> String {
    // Check X-Forwarded-For header first (for proxied requests)
    if let Some(xff) = req.headers().get("X-Forwarded-For") {
        if let Ok(s) = xff.to_str() {
            // Take first IP in chain
            if let Some(ip) = s.split(',').next() {
                return ip.trim().to_string();
            }
        }
    }

    // Check X-Real-IP
    if let Some(xri) = req.headers().get("X-Real-IP") {
        if let Ok(s) = xri.to_str() {
            return s.to_string();
        }
    }

    // Check for authenticated user (from JWT)
    if let Some(auth) = req.headers().get("Authorization") {
        // In a real implementation, extract user ID from JWT
        if auth.to_str().unwrap_or("").starts_with("Bearer ") {
            return "authenticated_user".to_string(); // Placeholder
        }
    }

    // Check for API key
    if let Some(api_key) = req.headers().get("X-API-Key") {
        if let Ok(s) = api_key.to_str() {
            return format!("apikey:{}", s);
        }
    }

    // Fallback
    "unknown".to_string()
}
```

---

## 2.2 CORS

### Task: Define CORS configuration

```rust
// crates/fraiseql-runtime/src/config/cors.rs

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct CorsConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Allowed origins (supports wildcards like "https://*.example.com")
    #[serde(default = "default_origins")]
    pub origins: Vec<String>,

    /// Allowed HTTP methods
    #[serde(default = "default_methods")]
    pub methods: Vec<String>,

    /// Allowed headers
    #[serde(default = "default_headers")]
    pub headers: Vec<String>,

    /// Allow credentials
    #[serde(default)]
    pub credentials: bool,

    /// Preflight cache duration in seconds
    #[serde(default = "default_max_age")]
    pub max_age: u64,

    /// Exposed headers (returned to browser)
    #[serde(default)]
    pub expose_headers: Vec<String>,

    /// Allow private network access (for localhost development)
    #[serde(default)]
    pub private_network: bool,
}

fn default_enabled() -> bool { true }
fn default_origins() -> Vec<String> { vec!["*".to_string()] }
fn default_methods() -> Vec<String> {
    vec!["GET".to_string(), "POST".to_string(), "OPTIONS".to_string()]
}
fn default_headers() -> Vec<String> {
    vec![
        "Authorization".to_string(),
        "Content-Type".to_string(),
        "X-Request-ID".to_string(),
    ]
}
fn default_max_age() -> u64 { 86400 }
```

### Task: Build CORS layer with validation

```rust
// crates/fraiseql-runtime/src/middleware/cors.rs

use tower_http::cors::{CorsLayer, AllowOrigin, AllowMethods, AllowHeaders, ExposeHeaders};
use axum::http::{HeaderName, HeaderValue, Method};
use std::str::FromStr;

use crate::config::CorsConfig;
use fraiseql_error::ConfigError;

pub fn build_cors_layer(config: &CorsConfig) -> Result<CorsLayer, ConfigError> {
    let mut layer = CorsLayer::new();

    // Validate and set origins
    if config.origins.len() == 1 && config.origins[0] == "*" {
        if config.credentials {
            return Err(ConfigError::ValidationError {
                field: "cors".to_string(),
                message: "Cannot use wildcard origin with credentials=true".to_string(),
            });
        }
        layer = layer.allow_origin(AllowOrigin::any());
    } else {
        // Build predicate for wildcard matching
        let patterns: Vec<WildcardPattern> = config.origins.iter()
            .map(|o| WildcardPattern::new(o))
            .collect();

        layer = layer.allow_origin(AllowOrigin::predicate(move |origin, _| {
            if let Ok(origin_str) = origin.to_str() {
                patterns.iter().any(|p| p.matches(origin_str))
            } else {
                false
            }
        }));
    }

    // Methods
    let methods: Vec<Method> = config.methods.iter()
        .filter_map(|m| Method::from_str(m).ok())
        .collect();
    if methods.is_empty() {
        return Err(ConfigError::ValidationError {
            field: "cors.methods".to_string(),
            message: "At least one valid HTTP method is required".to_string(),
        });
    }
    layer = layer.allow_methods(methods);

    // Headers
    let headers: Vec<HeaderName> = config.headers.iter()
        .filter_map(|h| HeaderName::from_str(h).ok())
        .collect();
    layer = layer.allow_headers(headers);

    // Credentials
    if config.credentials {
        layer = layer.allow_credentials(true);
    }

    // Max age
    layer = layer.max_age(std::time::Duration::from_secs(config.max_age));

    // Expose headers
    if !config.expose_headers.is_empty() {
        let expose: Vec<HeaderName> = config.expose_headers.iter()
            .filter_map(|h| HeaderName::from_str(h).ok())
            .collect();
        layer = layer.expose_headers(expose);
    }

    // Private network access header (for Chrome's Private Network Access)
    if config.private_network {
        layer = layer.allow_private_network(true);
    }

    Ok(layer)
}

/// Simple wildcard pattern matcher
struct WildcardPattern {
    prefix: String,
    suffix: String,
    has_wildcard: bool,
}

impl WildcardPattern {
    fn new(pattern: &str) -> Self {
        if let Some(idx) = pattern.find('*') {
            Self {
                prefix: pattern[..idx].to_string(),
                suffix: pattern[idx+1..].to_string(),
                has_wildcard: true,
            }
        } else {
            Self {
                prefix: pattern.to_string(),
                suffix: String::new(),
                has_wildcard: false,
            }
        }
    }

    fn matches(&self, value: &str) -> bool {
        if self.has_wildcard {
            value.starts_with(&self.prefix) && value.ends_with(&self.suffix)
        } else {
            value == self.prefix
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wildcard_pattern_exact() {
        let pattern = WildcardPattern::new("https://example.com");
        assert!(pattern.matches("https://example.com"));
        assert!(!pattern.matches("https://other.com"));
    }

    #[test]
    fn test_wildcard_pattern_prefix() {
        let pattern = WildcardPattern::new("https://*.example.com");
        assert!(pattern.matches("https://app.example.com"));
        assert!(pattern.matches("https://api.example.com"));
        assert!(!pattern.matches("https://example.com")); // No subdomain
        assert!(!pattern.matches("https://evil.com")); // Different domain
    }

    #[test]
    fn test_cors_credentials_wildcard_error() {
        let config = CorsConfig {
            enabled: true,
            origins: vec!["*".to_string()],
            credentials: true,
            ..Default::default()
        };

        let result = build_cors_layer(&config);
        assert!(result.is_err());
    }
}
```

---

## 2.3 Health Checks with Dependency Checks

Health checks were already defined in Phase 1 (lifecycle/health.rs). This section adds additional checks and startup connectivity verification.

### Task: Implement startup health checks

```rust
// crates/fraiseql-runtime/src/lifecycle/startup.rs

use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

use crate::state::AppState;
use fraiseql_error::RuntimeError;

/// Verify all critical dependencies are reachable before accepting traffic
pub async fn verify_dependencies(state: &Arc<AppState>) -> Result<(), RuntimeError> {
    let mut errors = Vec::new();

    // Check database (required)
    match timeout(Duration::from_secs(10), check_database(state)).await {
        Ok(Ok(())) => tracing::info!("Database connection verified"),
        Ok(Err(e)) => errors.push(format!("Database: {}", e)),
        Err(_) => errors.push("Database: connection timeout".to_string()),
    }

    // Check cache (if configured)
    if state.cache.is_some() {
        match timeout(Duration::from_secs(5), check_cache(state)).await {
            Ok(Ok(())) => tracing::info!("Cache connection verified"),
            Ok(Err(e)) => {
                // Cache is optional - warn but don't fail
                tracing::warn!("Cache connection failed: {}", e);
            }
            Err(_) => tracing::warn!("Cache connection timeout"),
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(RuntimeError::Internal {
            message: format!("Startup dependency check failed: {}", errors.join(", ")),
            source: None,
        })
    }
}

async fn check_database(state: &AppState) -> Result<(), RuntimeError> {
    sqlx::query("SELECT 1")
        .fetch_one(&state.db)
        .await
        .map_err(|e| RuntimeError::Database(e))?;
    Ok(())
}

async fn check_cache(state: &AppState) -> Result<(), RuntimeError> {
    if let Some(cache) = &state.cache {
        cache.ping().await?;
    }
    Ok(())
}
```

---

## 2.4 Metrics with SLO Tracking

### Task: Define metrics configuration

```rust
// crates/fraiseql-runtime/src/config/metrics.rs

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct MetricsConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    #[serde(default = "default_path")]
    pub path: Option<String>,

    #[serde(default = "default_format")]
    pub format: String,

    /// SLO configuration
    #[serde(default)]
    pub slos: SloConfig,
}

fn default_enabled() -> bool { true }
fn default_path() -> Option<String> { Some("/metrics".to_string()) }
fn default_format() -> String { "prometheus".to_string() }

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SloConfig {
    /// Target latency percentiles to track
    #[serde(default = "default_latency_percentiles")]
    pub latency_percentiles: Vec<f64>,

    /// Latency SLO targets (p99 < Xms)
    #[serde(default)]
    pub latency_targets: LatencyTargets,

    /// Availability SLO target (e.g., 0.999 for 99.9%)
    #[serde(default = "default_availability_target")]
    pub availability_target: f64,

    /// Error rate SLO target (e.g., 0.01 for 1%)
    #[serde(default = "default_error_rate_target")]
    pub error_rate_target: f64,
}

fn default_latency_percentiles() -> Vec<f64> {
    vec![0.5, 0.9, 0.95, 0.99]
}
fn default_availability_target() -> f64 { 0.999 }
fn default_error_rate_target() -> f64 { 0.01 }

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LatencyTargets {
    #[serde(default = "default_graphql_latency")]
    pub graphql_p99_ms: u64,

    #[serde(default = "default_webhook_latency")]
    pub webhook_p99_ms: u64,

    #[serde(default = "default_auth_latency")]
    pub auth_p99_ms: u64,

    #[serde(default = "default_file_upload_latency")]
    pub file_upload_p99_ms: u64,
}

fn default_graphql_latency() -> u64 { 100 }
fn default_webhook_latency() -> u64 { 500 }
fn default_auth_latency() -> u64 { 10 }
fn default_file_upload_latency() -> u64 { 2000 }
```

### Task: Implement metrics collection with SLO tracking

```rust
// crates/fraiseql-runtime/src/observability/metrics.rs

use metrics::{counter, histogram, gauge, describe_counter, describe_histogram, describe_gauge};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::time::Instant;
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};

use crate::config::MetricsConfig;
use fraiseql_error::RuntimeError;

/// Initialize metrics exporter with SLO buckets
pub fn init_metrics(config: &MetricsConfig) -> Result<PrometheusHandle, RuntimeError> {
    let builder = PrometheusBuilder::new();

    // Configure histogram buckets for latency SLOs
    let slo_buckets = vec![
        0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0
    ];

    let handle = builder
        .set_buckets(&slo_buckets)
        .map_err(|e| RuntimeError::Internal {
            message: format!("Failed to configure metric buckets: {}", e),
            source: None,
        })?
        .install_recorder()
        .map_err(|e| RuntimeError::Internal {
            message: format!("Failed to install metrics: {}", e),
            source: None,
        })?;

    // Register standard metrics with descriptions
    describe_metrics();

    // Initialize SLO tracking metrics
    init_slo_metrics(&config.slos);

    Ok(handle)
}

fn describe_metrics() {
    // HTTP metrics
    describe_counter!(
        "http_requests_total",
        "Total number of HTTP requests"
    );
    describe_histogram!(
        "http_request_duration_seconds",
        "HTTP request duration in seconds"
    );

    // GraphQL metrics
    describe_counter!(
        "graphql_operations_total",
        "Total number of GraphQL operations"
    );
    describe_histogram!(
        "graphql_operation_duration_seconds",
        "GraphQL operation duration in seconds"
    );
    describe_counter!(
        "graphql_errors_total",
        "Total number of GraphQL errors"
    );

    // Webhook metrics
    describe_counter!(
        "webhook_events_total",
        "Total number of webhook events received"
    );
    describe_histogram!(
        "webhook_processing_duration_seconds",
        "Webhook processing duration in seconds"
    );

    // Auth metrics
    describe_counter!(
        "auth_operations_total",
        "Total number of authentication operations"
    );
    describe_histogram!(
        "auth_operation_duration_seconds",
        "Authentication operation duration in seconds"
    );

    // File metrics
    describe_counter!(
        "file_operations_total",
        "Total number of file operations"
    );
    describe_histogram!(
        "file_upload_duration_seconds",
        "File upload duration in seconds"
    );
    describe_histogram!(
        "file_size_bytes",
        "File size in bytes"
    );

    // Notification metrics
    describe_counter!(
        "notifications_total",
        "Total number of notifications sent"
    );
    describe_histogram!(
        "notification_duration_seconds",
        "Notification send duration in seconds"
    );

    // Observer metrics
    describe_counter!(
        "observer_events_total",
        "Total number of observer events processed"
    );
    describe_histogram!(
        "observer_action_duration_seconds",
        "Observer action duration in seconds"
    );

    // Database metrics
    describe_gauge!(
        "db_pool_connections_active",
        "Number of active database connections"
    );
    describe_gauge!(
        "db_pool_connections_idle",
        "Number of idle database connections"
    );
    describe_histogram!(
        "db_query_duration_seconds",
        "Database query duration in seconds"
    );

    // Rate limiting metrics
    describe_counter!(
        "rate_limit_requests_total",
        "Total rate limit decisions"
    );
    describe_gauge!(
        "rate_limit_queue_depth",
        "Current rate limit queue depth"
    );

    // Circuit breaker metrics
    describe_counter!(
        "circuit_breaker_state_changes_total",
        "Circuit breaker state changes"
    );
    describe_gauge!(
        "circuit_breaker_state",
        "Current circuit breaker state (0=closed, 1=open, 2=half-open)"
    );
}

fn init_slo_metrics(config: &SloConfig) {
    // SLO compliance metrics
    describe_gauge!(
        "slo_latency_target_seconds",
        "SLO latency target in seconds"
    );
    describe_counter!(
        "slo_latency_violations_total",
        "Total SLO latency violations"
    );
    describe_gauge!(
        "slo_error_budget_remaining",
        "Remaining SLO error budget (0-1)"
    );

    // Set initial targets
    gauge!("slo_latency_target_seconds", "component" => "graphql")
        .set(config.latency_targets.graphql_p99_ms as f64 / 1000.0);
    gauge!("slo_latency_target_seconds", "component" => "webhook")
        .set(config.latency_targets.webhook_p99_ms as f64 / 1000.0);
    gauge!("slo_latency_target_seconds", "component" => "auth")
        .set(config.latency_targets.auth_p99_ms as f64 / 1000.0);
    gauge!("slo_latency_target_seconds", "component" => "file_upload")
        .set(config.latency_targets.file_upload_p99_ms as f64 / 1000.0);
}

/// Middleware to record HTTP request metrics
pub async fn metrics_middleware(
    req: Request,
    next: Next,
) -> Response {
    let start = Instant::now();
    let method = req.method().to_string();
    let path = normalize_path(req.uri().path());

    let response = next.run(req).await;

    let status = response.status().as_u16().to_string();
    let status_class = format!("{}xx", response.status().as_u16() / 100);
    let duration = start.elapsed().as_secs_f64();

    counter!(
        "http_requests_total",
        "method" => method.clone(),
        "path" => path.clone(),
        "status" => status.clone(),
        "status_class" => status_class
    ).increment(1);

    histogram!(
        "http_request_duration_seconds",
        "method" => method,
        "path" => path
    ).record(duration);

    response
}

/// Normalize path for metrics (replace IDs with placeholders)
fn normalize_path(path: &str) -> String {
    // Replace UUIDs
    let uuid_regex = regex::Regex::new(
        r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}"
    ).unwrap();
    let path = uuid_regex.replace_all(path, ":id");

    // Replace numeric IDs
    let numeric_regex = regex::Regex::new(r"/\d+").unwrap();
    let path = numeric_regex.replace_all(&path, "/:id");

    path.to_string()
}

/// Record operation metrics with SLO tracking
pub struct OperationMetrics {
    component: &'static str,
    operation: String,
    start: Instant,
    slo_target_ms: u64,
}

impl OperationMetrics {
    pub fn new(component: &'static str, operation: impl Into<String>, slo_target_ms: u64) -> Self {
        Self {
            component,
            operation: operation.into(),
            start: Instant::now(),
            slo_target_ms,
        }
    }

    pub fn success(self) {
        self.record("success");
    }

    pub fn failure(self, error_type: &str) {
        self.record(error_type);
    }

    fn record(self, status: &str) {
        let duration = self.start.elapsed();
        let duration_secs = duration.as_secs_f64();

        // Record duration histogram
        let histogram_name = format!("{}_operation_duration_seconds", self.component);
        histogram!(
            histogram_name,
            "operation" => self.operation.clone(),
            "status" => status.to_string()
        ).record(duration_secs);

        // Record total counter
        let counter_name = format!("{}_operations_total", self.component);
        counter!(
            counter_name,
            "operation" => self.operation.clone(),
            "status" => status.to_string()
        ).increment(1);

        // Check SLO violation
        if duration.as_millis() as u64 > self.slo_target_ms {
            counter!(
                "slo_latency_violations_total",
                "component" => self.component.to_string(),
                "operation" => self.operation
            ).increment(1);
        }
    }
}

/// Metrics endpoint handler
pub async fn metrics_handler(
    State(handle): State<PrometheusHandle>,
) -> String {
    handle.render()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path() {
        assert_eq!(
            normalize_path("/users/123"),
            "/users/:id"
        );
        assert_eq!(
            normalize_path("/files/550e8400-e29b-41d4-a716-446655440000"),
            "/files/:id"
        );
        assert_eq!(
            normalize_path("/api/v1/users/123/posts/456"),
            "/api/v1/users/:id/posts/:id"
        );
    }
}
```

---

## 2.5 Request Tracing

### Task: Define tracing configuration

```rust
// crates/fraiseql-runtime/src/config/tracing.rs

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct TracingConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Exporter: otlp, jaeger, zipkin, stdout
    #[serde(default = "default_exporter")]
    pub exporter: String,

    /// Endpoint (env var name)
    pub endpoint_env: Option<String>,

    /// Service name
    #[serde(default = "default_service_name")]
    pub service_name: String,

    /// Sample rate (0.0 - 1.0)
    #[serde(default = "default_sample_rate")]
    pub sample_rate: f64,

    /// Log level filter
    #[serde(default = "default_level")]
    pub level: String,

    /// Log format: json, pretty
    #[serde(default = "default_format")]
    pub format: String,

    /// Propagation format: w3c, b3, jaeger
    #[serde(default = "default_propagation")]
    pub propagation: String,
}

fn default_enabled() -> bool { true }
fn default_exporter() -> String { "otlp".to_string() }
fn default_service_name() -> String { "fraiseql".to_string() }
fn default_sample_rate() -> f64 { 0.1 }
fn default_level() -> String { "info".to_string() }
fn default_format() -> String { "json".to_string() }
fn default_propagation() -> String { "w3c".to_string() }
```

### Task: Initialize tracing with propagation

```rust
// crates/fraiseql-runtime/src/observability/tracing.rs

use tracing_subscriber::{
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
    Layer,
};
use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::propagation::TraceContextPropagator;

use crate::config::TracingConfig;
use fraiseql_error::RuntimeError;

pub fn init_tracing(config: &TracingConfig) -> Result<(), RuntimeError> {
    // Set up propagation
    match config.propagation.as_str() {
        "w3c" => opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new()),
        "b3" => {
            use opentelemetry_sdk::propagation::BaggagePropagator;
            opentelemetry::global::set_text_map_propagator(BaggagePropagator::new());
        }
        _ => opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new()),
    }

    // Create filter from RUST_LOG or config
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.level));

    // Create fmt layer
    let fmt_layer = if config.format == "json" {
        tracing_subscriber::fmt::layer()
            .json()
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true)
            .boxed()
    } else {
        tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_thread_ids(true)
            .boxed()
    };

    // Create OpenTelemetry layer if enabled
    if config.enabled && config.exporter != "stdout" {
        let tracer = init_otlp_tracer(config)?;
        let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .with(otel_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .init();
    }

    Ok(())
}

fn init_otlp_tracer(config: &TracingConfig) -> Result<opentelemetry_sdk::trace::Tracer, RuntimeError> {
    let endpoint = config.endpoint_env.as_ref()
        .and_then(|env| std::env::var(env).ok())
        .unwrap_or_else(|| "http://localhost:4317".to_string());

    let exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint(&endpoint);

    let trace_config = opentelemetry_sdk::trace::Config::default()
        .with_sampler(opentelemetry_sdk::trace::Sampler::TraceIdRatioBased(config.sample_rate))
        .with_resource(opentelemetry_sdk::Resource::new(vec![
            opentelemetry::KeyValue::new("service.name", config.service_name.clone()),
            opentelemetry::KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
        ]));

    let provider = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(exporter)
        .with_trace_config(trace_config)
        .install_batch(opentelemetry_sdk::runtime::Tokio)
        .map_err(|e| RuntimeError::Internal {
            message: format!("Failed to initialize OTLP tracer: {}", e),
            source: None,
        })?;

    Ok(provider.tracer("fraiseql"))
}

/// Shutdown tracing and flush pending spans
pub fn shutdown_tracing() {
    opentelemetry::global::shutdown_tracer_provider();
}

/// Extract trace context from incoming request headers
pub fn extract_trace_context<'a>(
    headers: impl Iterator<Item = (&'a str, &'a str)>,
) -> opentelemetry::Context {
    use opentelemetry::propagation::TextMapPropagator;

    let propagator = TraceContextPropagator::new();
    let extractor = HeaderExtractor { headers: headers.collect() };
    propagator.extract(&extractor)
}

struct HeaderExtractor<'a> {
    headers: Vec<(&'a str, &'a str)>,
}

impl opentelemetry::propagation::Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.headers.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| *v)
    }

    fn keys(&self) -> Vec<&str> {
        self.headers.iter().map(|(k, _)| *k).collect()
    }
}
```

---

## 2.6 Request ID Propagation

### Task: Implement request ID middleware

```rust
// crates/fraiseql-runtime/src/middleware/request_id.rs

use axum::{
    extract::Request,
    middleware::Next,
    response::Response,
    http::{HeaderName, HeaderValue},
};
use uuid::Uuid;

const REQUEST_ID_HEADER: &str = "X-Request-ID";

/// Middleware to ensure every request has a unique ID
pub async fn request_id_middleware(
    mut req: Request,
    next: Next,
) -> Response {
    // Check for existing request ID
    let request_id = req.headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    // Add request ID to request extensions
    req.extensions_mut().insert(RequestId(request_id.clone()));

    // Update tracing span with request ID
    tracing::Span::current().record("request_id", &request_id);

    let mut response = next.run(req).await;

    // Add request ID to response headers
    response.headers_mut().insert(
        HeaderName::from_static("x-request-id"),
        HeaderValue::from_str(&request_id).unwrap(),
    );

    response
}

/// Request ID extension for extracting in handlers
#[derive(Clone, Debug)]
pub struct RequestId(pub String);

/// Extractor for request ID
impl<S> axum::extract::FromRequestParts<S> for RequestId
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(parts.extensions
            .get::<RequestId>()
            .cloned()
            .unwrap_or_else(|| RequestId(Uuid::new_v4().to_string())))
    }
}
```

---

## 2.7 Tests

### Task: Rate limiter unit tests

```rust
// crates/fraiseql-runtime/src/middleware/rate_limit/tests.rs

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_rate_limit_parse() {
        let limit = RateLimit::parse("100/minute").unwrap();
        assert_eq!(limit.requests, 100);
        assert_eq!(limit.window, Duration::from_secs(60));

        let limit = RateLimit::parse("10/second").unwrap();
        assert_eq!(limit.requests, 10);
        assert_eq!(limit.window, Duration::from_secs(1));

        let limit = RateLimit::parse("1000/hour").unwrap();
        assert_eq!(limit.requests, 1000);
        assert_eq!(limit.window, Duration::from_secs(3600));
    }

    #[test]
    fn test_rate_limit_parse_invalid() {
        assert!(RateLimit::parse("invalid").is_err());
        assert!(RateLimit::parse("abc/minute").is_err());
        assert!(RateLimit::parse("100/lightyear").is_err());
    }

    #[tokio::test]
    async fn test_memory_rate_limiter_allows_under_limit() {
        let config = BackpressureConfig::default();
        let limiter = MemoryRateLimiter::new(config);
        let limit = RateLimit::parse("10/minute").unwrap();

        for _ in 0..10 {
            let result = limiter.check("test_key", &limit).await;
            assert!(matches!(result, RateLimitResult::Allowed { .. }));
            limiter.record("test_key", &limit).await;
        }
    }

    #[tokio::test]
    async fn test_memory_rate_limiter_blocks_over_limit() {
        let config = BackpressureConfig::default();
        let limiter = MemoryRateLimiter::new(config);
        let limit = RateLimit::parse("5/minute").unwrap();

        // Use up the limit
        for _ in 0..5 {
            let result = limiter.check("test_key", &limit).await;
            assert!(matches!(result, RateLimitResult::Allowed { .. }));
            limiter.record("test_key", &limit).await;
        }

        // Should be limited
        let result = limiter.check("test_key", &limit).await;
        assert!(matches!(result, RateLimitResult::Limited { .. }));
    }

    #[tokio::test]
    async fn test_mock_rate_limiter() {
        let mock = MockRateLimiter::with_results(vec![
            RateLimitResult::Allowed {
                remaining: 5,
                limit: 10,
                reset_at: std::time::SystemTime::now(),
            },
            RateLimitResult::Limited {
                retry_after: Duration::from_secs(60),
                limit: 10,
            },
        ]);

        let limit = RateLimit::parse("10/minute").unwrap();

        // First call returns Limited (LIFO)
        let result = mock.check("key", &limit).await;
        assert!(matches!(result, RateLimitResult::Limited { .. }));

        // Second call returns Allowed
        let result = mock.check("key", &limit).await;
        assert!(matches!(result, RateLimitResult::Allowed { .. }));

        // Verify calls were recorded
        let calls = mock.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
    }
}
```

### Task: CORS tests

```rust
// crates/fraiseql-runtime/src/middleware/cors/tests.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wildcard_pattern_matching() {
        let patterns = vec![
            ("https://example.com", "https://example.com", true),
            ("https://example.com", "https://other.com", false),
            ("https://*.example.com", "https://app.example.com", true),
            ("https://*.example.com", "https://api.example.com", true),
            ("https://*.example.com", "https://example.com", false),
            ("http://localhost:*", "http://localhost:3000", true),
            ("http://localhost:*", "http://localhost:8080", true),
        ];

        for (pattern, value, expected) in patterns {
            let p = WildcardPattern::new(pattern);
            assert_eq!(
                p.matches(value),
                expected,
                "Pattern '{}' matching '{}' should be {}",
                pattern, value, expected
            );
        }
    }

    #[test]
    fn test_cors_config_validation() {
        // Valid config
        let config = CorsConfig {
            enabled: true,
            origins: vec!["https://example.com".to_string()],
            credentials: true,
            ..Default::default()
        };
        assert!(build_cors_layer(&config).is_ok());

        // Invalid: wildcard + credentials
        let config = CorsConfig {
            enabled: true,
            origins: vec!["*".to_string()],
            credentials: true,
            ..Default::default()
        };
        assert!(build_cors_layer(&config).is_err());
    }
}
```

---

## Acceptance Criteria

- [ ] Rate limiting works with memory storage
- [ ] Rate limiting works with Redis storage
- [ ] Rate limit headers are included in responses (X-RateLimit-*)
- [ ] Backpressure queuing works when enabled
- [ ] Load shedding returns 503 when system is overloaded
- [ ] CORS handles wildcard origins correctly
- [ ] CORS handles preflight requests
- [ ] CORS validation rejects credentials + wildcard combination
- [ ] Health endpoint returns server status with component checks
- [ ] Readiness endpoint returns 503 during shutdown
- [ ] Startup dependency checks verify database connectivity
- [ ] Prometheus metrics are exposed at /metrics
- [ ] SLO metrics track latency violations
- [ ] Path normalization works for metrics (UUIDs/IDs replaced)
- [ ] Distributed tracing works with OTLP exporter
- [ ] Request IDs are propagated through the system
- [ ] Trace context is extracted from incoming headers
- [ ] Mock implementations exist for all rate limiter interfaces
- [ ] Unit tests cover rate limiting edge cases
- [ ] Unit tests cover CORS pattern matching

---

## Files to Create/Modify

```
crates/fraiseql-runtime/src/
├── config/
│   ├── rate_limiting.rs    # Rate limiting + backpressure config
│   ├── cors.rs             # CORS config
│   ├── metrics.rs          # Metrics + SLO config
│   └── tracing.rs          # Tracing config
├── middleware/
│   ├── mod.rs
│   ├── rate_limit/
│   │   ├── mod.rs          # RateLimiter trait + types
│   │   ├── memory.rs       # In-memory implementation
│   │   ├── redis.rs        # Redis implementation
│   │   ├── middleware.rs   # Axum middleware
│   │   └── tests.rs
│   ├── cors.rs             # CORS layer builder
│   └── request_id.rs       # Request ID middleware
├── lifecycle/
│   └── startup.rs          # Startup dependency checks
└── observability/
    ├── mod.rs
    ├── metrics.rs          # Metrics with SLO tracking
    └── tracing.rs          # Distributed tracing
```

---

## DO NOT

- Do not implement GraphQL-specific rate limiting yet (requires GraphQL parsing)
- Do not add complex observability dashboards
- Do not implement custom tracing exporters
- Do not add alerting - that's external to the runtime
- Do not skip writing tests for rate limiting logic
- Do not hardcode SLO targets - make them configurable
