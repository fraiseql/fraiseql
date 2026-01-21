# Phase 1: Foundation

## Objective

Build the foundational infrastructure for the endpoint runtime: configuration parsing, HTTP server integration, middleware pipeline, error handling framework, graceful shutdown, and testing infrastructure.

---

## 1.1 Configuration System

### Task: Create `fraiseql-runtime` crate

```bash
cargo new --lib crates/fraiseql-runtime
```

### Task: Define configuration structures

```rust
// crates/fraiseql-runtime/src/config/mod.rs

use serde::Deserialize;
use std::path::PathBuf;
use std::collections::HashMap;

/// Root configuration structure
#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,

    #[serde(default)]
    pub webhooks: HashMap<String, WebhookConfig>,

    #[serde(default)]
    pub files: HashMap<String, FileConfig>,

    #[serde(default)]
    pub auth: Option<AuthConfig>,

    #[serde(default)]
    pub notifications: Option<NotificationsConfig>,

    #[serde(default)]
    pub observers: HashMap<String, ObserverConfig>,

    #[serde(default)]
    pub interceptors: HashMap<String, Vec<String>>,

    #[serde(default)]
    pub rate_limiting: Option<RateLimitingConfig>,

    #[serde(default)]
    pub cors: Option<CorsConfig>,

    #[serde(default)]
    pub metrics: Option<MetricsConfig>,

    #[serde(default)]
    pub tracing: Option<TracingConfig>,

    #[serde(default)]
    pub logging: Option<LoggingConfig>,

    #[serde(default)]
    pub storage: HashMap<String, StorageConfig>,

    #[serde(default)]
    pub search: Option<SearchConfig>,

    #[serde(default)]
    pub cache: Option<CacheConfig>,

    #[serde(default)]
    pub queues: Option<QueueConfig>,

    #[serde(default)]
    pub realtime: Option<RealtimeConfig>,

    #[serde(default)]
    pub custom_endpoints: Option<CustomEndpointsConfig>,

    #[serde(default)]
    pub lifecycle: Option<LifecycleConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_port")]
    pub port: u16,

    #[serde(default = "default_host")]
    pub host: String,

    #[serde(default)]
    pub workers: Option<usize>,

    #[serde(default)]
    pub tls: Option<TlsConfig>,

    #[serde(default)]
    pub limits: Option<ServerLimitsConfig>,
}

fn default_port() -> u16 { 4000 }
fn default_host() -> String { "127.0.0.1".to_string() }

#[derive(Debug, Clone, Deserialize)]
pub struct TlsConfig {
    pub cert_file: PathBuf,
    pub key_file: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerLimitsConfig {
    #[serde(default = "default_max_request_size")]
    pub max_request_size: String,

    #[serde(default = "default_request_timeout")]
    pub request_timeout: String,

    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_requests: usize,

    #[serde(default = "default_max_queue_depth")]
    pub max_queue_depth: usize,
}

fn default_max_request_size() -> String { "10MB".to_string() }
fn default_request_timeout() -> String { "30s".to_string() }
fn default_max_concurrent() -> usize { 1000 }
fn default_max_queue_depth() -> usize { 5000 }

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url_env: String,

    #[serde(default = "default_pool_size")]
    pub pool_size: u32,

    #[serde(default)]
    pub pool_timeout: Option<String>,

    #[serde(default)]
    pub statement_timeout: Option<String>,

    #[serde(default)]
    pub replicas: Vec<ReplicaConfig>,

    #[serde(default)]
    pub health_check_interval: Option<String>,
}

fn default_pool_size() -> u32 { 10 }

#[derive(Debug, Clone, Deserialize)]
pub struct ReplicaConfig {
    pub url_env: String,

    #[serde(default = "default_weight")]
    pub weight: u32,
}

fn default_weight() -> u32 { 1 }

/// Lifecycle configuration for graceful shutdown
#[derive(Debug, Clone, Deserialize)]
pub struct LifecycleConfig {
    /// Time to wait for in-flight requests to complete
    #[serde(default = "default_shutdown_timeout")]
    pub shutdown_timeout: String,

    /// Time to wait before starting shutdown (for load balancer deregistration)
    #[serde(default = "default_shutdown_delay")]
    pub shutdown_delay: String,

    /// Health check endpoint path
    #[serde(default = "default_health_path")]
    pub health_path: String,

    /// Readiness check endpoint path
    #[serde(default = "default_ready_path")]
    pub ready_path: String,
}

fn default_shutdown_timeout() -> String { "30s".to_string() }
fn default_shutdown_delay() -> String { "5s".to_string() }
fn default_health_path() -> String { "/health".to_string() }
fn default_ready_path() -> String { "/ready".to_string() }
```

### Task: Implement comprehensive configuration validation

```rust
// crates/fraiseql-runtime/src/config/validation.rs

use std::env;
use std::collections::HashSet;

use crate::config::RuntimeConfig;
use fraiseql_error::ConfigError;

/// Validation result with all errors collected
pub struct ValidationResult {
    pub errors: Vec<ConfigError>,
    pub warnings: Vec<String>,
}

impl ValidationResult {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn add_error(&mut self, error: ConfigError) {
        self.errors.push(error);
    }

    pub fn add_warning(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }

    pub fn into_result(self) -> Result<Vec<String>, ConfigError> {
        if self.errors.is_empty() {
            Ok(self.warnings)
        } else if self.errors.len() == 1 {
            Err(self.errors.into_iter().next().unwrap())
        } else {
            Err(ConfigError::MultipleErrors { errors: self.errors })
        }
    }
}

/// Comprehensive configuration validator
pub struct ConfigValidator<'a> {
    config: &'a RuntimeConfig,
    result: ValidationResult,
    checked_env_vars: HashSet<String>,
}

impl<'a> ConfigValidator<'a> {
    pub fn new(config: &'a RuntimeConfig) -> Self {
        Self {
            config,
            result: ValidationResult::new(),
            checked_env_vars: HashSet::new(),
        }
    }

    /// Run all validations
    pub fn validate(mut self) -> ValidationResult {
        self.validate_server();
        self.validate_database();
        self.validate_webhooks();
        self.validate_auth();
        self.validate_files();
        self.validate_cross_field();
        self.validate_env_vars();
        self.result
    }

    fn validate_server(&mut self) {
        // Port validation
        if self.config.server.port == 0 {
            self.result.add_error(ConfigError::ValidationError {
                field: "server.port".to_string(),
                message: "Port cannot be 0".to_string(),
            });
        }

        // Limits validation
        if let Some(limits) = &self.config.server.limits {
            if let Err(e) = crate::config::env::parse_size(&limits.max_request_size) {
                self.result.add_error(ConfigError::ValidationError {
                    field: "server.limits.max_request_size".to_string(),
                    message: format!("Invalid size format: {}", e),
                });
            }

            if let Err(e) = crate::config::env::parse_duration(&limits.request_timeout) {
                self.result.add_error(ConfigError::ValidationError {
                    field: "server.limits.request_timeout".to_string(),
                    message: format!("Invalid duration format: {}", e),
                });
            }

            if limits.max_concurrent_requests == 0 {
                self.result.add_error(ConfigError::ValidationError {
                    field: "server.limits.max_concurrent_requests".to_string(),
                    message: "Must be greater than 0".to_string(),
                });
            }
        }

        // TLS validation
        if let Some(tls) = &self.config.server.tls {
            if !tls.cert_file.exists() {
                self.result.add_error(ConfigError::ValidationError {
                    field: "server.tls.cert_file".to_string(),
                    message: format!("Certificate file not found: {:?}", tls.cert_file),
                });
            }
            if !tls.key_file.exists() {
                self.result.add_error(ConfigError::ValidationError {
                    field: "server.tls.key_file".to_string(),
                    message: format!("Key file not found: {:?}", tls.key_file),
                });
            }
        }
    }

    fn validate_database(&mut self) {
        // Required env var
        if self.config.database.url_env.is_empty() {
            self.result.add_error(ConfigError::ValidationError {
                field: "database.url_env".to_string(),
                message: "Database URL environment variable must be specified".to_string(),
            });
        } else {
            self.checked_env_vars.insert(self.config.database.url_env.clone());
        }

        // Pool size
        if self.config.database.pool_size == 0 {
            self.result.add_error(ConfigError::ValidationError {
                field: "database.pool_size".to_string(),
                message: "Pool size must be greater than 0".to_string(),
            });
        }

        // Replica env vars
        for (i, replica) in self.config.database.replicas.iter().enumerate() {
            if replica.url_env.is_empty() {
                self.result.add_error(ConfigError::ValidationError {
                    field: format!("database.replicas[{}].url_env", i),
                    message: "Replica URL environment variable must be specified".to_string(),
                });
            } else {
                self.checked_env_vars.insert(replica.url_env.clone());
            }
        }
    }

    fn validate_webhooks(&mut self) {
        for (name, webhook) in &self.config.webhooks {
            // Secret env var required
            if webhook.secret_env.is_empty() {
                self.result.add_error(ConfigError::ValidationError {
                    field: format!("webhooks.{}.secret_env", name),
                    message: "Webhook secret environment variable must be specified".to_string(),
                });
            } else {
                self.checked_env_vars.insert(webhook.secret_env.clone());
            }

            // Provider must be valid
            let valid_providers = [
                "stripe", "github", "shopify", "twilio", "sendgrid",
                "paddle", "slack", "discord", "linear", "svix",
                "clerk", "supabase", "novu", "resend", "generic_hmac"
            ];
            if !valid_providers.contains(&webhook.provider.as_str()) {
                self.result.add_warning(format!(
                    "Unknown webhook provider '{}' for webhook '{}'. Using generic_hmac.",
                    webhook.provider, name
                ));
            }
        }
    }

    fn validate_auth(&mut self) {
        if let Some(auth) = &self.config.auth {
            // JWT secret required if auth is enabled
            if auth.jwt.secret_env.is_empty() {
                self.result.add_error(ConfigError::ValidationError {
                    field: "auth.jwt.secret_env".to_string(),
                    message: "JWT secret environment variable must be specified".to_string(),
                });
            } else {
                self.checked_env_vars.insert(auth.jwt.secret_env.clone());
            }

            // Validate each provider
            for (name, provider) in &auth.providers {
                self.checked_env_vars.insert(provider.client_id_env.clone());
                self.checked_env_vars.insert(provider.client_secret_env.clone());

                // OIDC providers need issuer URL
                if provider.provider_type == "oidc" && provider.issuer_url.is_none() {
                    self.result.add_error(ConfigError::ValidationError {
                        field: format!("auth.providers.{}.issuer_url", name),
                        message: "OIDC providers require issuer_url".to_string(),
                    });
                }
            }

            // Callback URL required if any OAuth provider is configured
            if !auth.providers.is_empty() && auth.callback_base_url.is_none() {
                self.result.add_error(ConfigError::ValidationError {
                    field: "auth.callback_base_url".to_string(),
                    message: "callback_base_url is required when OAuth providers are configured".to_string(),
                });
            }
        }
    }

    fn validate_files(&mut self) {
        for (name, file_config) in &self.config.files {
            // Storage backend must be defined
            if !self.config.storage.contains_key(&file_config.storage) {
                self.result.add_error(ConfigError::ValidationError {
                    field: format!("files.{}.storage", name),
                    message: format!(
                        "Storage backend '{}' not found in storage configuration",
                        file_config.storage
                    ),
                });
            }

            // Max size validation
            if let Err(e) = crate::config::env::parse_size(&file_config.max_size) {
                self.result.add_error(ConfigError::ValidationError {
                    field: format!("files.{}.max_size", name),
                    message: format!("Invalid size format: {}", e),
                });
            }
        }

        // Validate storage backends
        for (name, storage) in &self.config.storage {
            match storage.backend.as_str() {
                "s3" | "r2" | "gcs" => {
                    if storage.bucket.is_none() {
                        self.result.add_error(ConfigError::ValidationError {
                            field: format!("storage.{}.bucket", name),
                            message: "Bucket name is required for cloud storage".to_string(),
                        });
                    }
                }
                "local" => {
                    if storage.path.is_none() {
                        self.result.add_error(ConfigError::ValidationError {
                            field: format!("storage.{}.path", name),
                            message: "Path is required for local storage".to_string(),
                        });
                    }
                }
                _ => {
                    self.result.add_error(ConfigError::ValidationError {
                        field: format!("storage.{}.backend", name),
                        message: format!("Unknown storage backend: {}", storage.backend),
                    });
                }
            }
        }
    }

    fn validate_cross_field(&mut self) {
        // Observers require notifications for email/slack actions
        for (name, observer) in &self.config.observers {
            for action in &observer.actions {
                match action.action_type.as_str() {
                    "email" | "slack" | "sms" | "push" => {
                        if self.config.notifications.is_none() {
                            self.result.add_error(ConfigError::ValidationError {
                                field: format!("observers.{}.actions", name),
                                message: format!(
                                    "Observer '{}' uses '{}' action but notifications are not configured",
                                    name, action.action_type
                                ),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        // Rate limiting with Redis backend requires cache config
        if let Some(rate_limit) = &self.config.rate_limiting {
            if rate_limit.backend == "redis" && self.config.cache.is_none() {
                self.result.add_error(ConfigError::ValidationError {
                    field: "rate_limiting.backend".to_string(),
                    message: "Redis rate limiting requires cache configuration".to_string(),
                });
            }
        }
    }

    fn validate_env_vars(&mut self) {
        // Check all collected env vars exist
        for var_name in &self.checked_env_vars {
            if env::var(var_name).is_err() {
                self.result.add_error(ConfigError::MissingEnvVar {
                    name: var_name.clone(),
                });
            }
        }
    }
}
```

### Task: Implement configuration loading

```rust
// crates/fraiseql-runtime/src/config/loader.rs

use std::path::Path;
use std::env;

use crate::config::RuntimeConfig;
use crate::config::validation::ConfigValidator;
use fraiseql_error::ConfigError;

impl RuntimeConfig {
    /// Load configuration from file with full validation
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();

        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::ReadError {
                path: path.to_path_buf(),
                source: e
            })?;

        let config: RuntimeConfig = toml::from_str(&content)
            .map_err(|e| ConfigError::ParseError { source: e })?;

        // Run comprehensive validation
        let validation = ConfigValidator::new(&config).validate();
        let warnings = validation.into_result()?;

        // Log warnings
        for warning in warnings {
            tracing::warn!("Configuration warning: {}", warning);
        }

        Ok(config)
    }

    /// Load configuration from default locations
    pub fn load() -> Result<Self, ConfigError> {
        // Check FRAISEQL_CONFIG environment variable
        if let Ok(path) = env::var("FRAISEQL_CONFIG") {
            return Self::from_file(&path);
        }

        // Check current directory
        let local_config = Path::new("./fraiseql.toml");
        if local_config.exists() {
            return Self::from_file(local_config);
        }

        // Check user config directory
        if let Some(config_dir) = dirs::config_dir() {
            let user_config = config_dir.join("fraiseql/config.toml");
            if user_config.exists() {
                return Self::from_file(&user_config);
            }
        }

        Err(ConfigError::NotFound)
    }

    /// Load configuration with optional file path (CLI argument)
    pub fn load_with_path(path: Option<&Path>) -> Result<Self, ConfigError> {
        match path {
            Some(p) => Self::from_file(p),
            None => Self::load(),
        }
    }

    /// Validate configuration without loading env vars (for dry-run/testing)
    pub fn validate_syntax(content: &str) -> Result<(), ConfigError> {
        let _config: RuntimeConfig = toml::from_str(content)
            .map_err(|e| ConfigError::ParseError { source: e })?;
        Ok(())
    }
}
```

### Task: Implement environment variable resolution

```rust
// crates/fraiseql-runtime/src/config/env.rs

use std::env;
use std::time::Duration;

/// Resolve a value that may be an environment variable reference
pub fn resolve_env_value(value: &str) -> Result<String, EnvError> {
    if value.starts_with("${") && value.ends_with("}") {
        let var_name = &value[2..value.len()-1];

        // Support default values: ${VAR:-default}
        if let Some((name, default)) = var_name.split_once(":-") {
            return env::var(name).or_else(|_| Ok(default.to_string()));
        }

        // Support required with message: ${VAR:?message}
        if let Some((name, message)) = var_name.split_once(":?") {
            return env::var(name).map_err(|_| EnvError::MissingVarWithMessage {
                name: name.to_string(),
                message: message.to_string(),
            });
        }

        env::var(var_name).map_err(|_| EnvError::MissingVar {
            name: var_name.to_string()
        })
    } else {
        Ok(value.to_string())
    }
}

/// Get value from environment variable name stored in config
pub fn get_env_value(env_var_name: &str) -> Result<String, EnvError> {
    env::var(env_var_name).map_err(|_| EnvError::MissingVar {
        name: env_var_name.to_string()
    })
}

/// Parse size strings like "10MB", "1GB"
pub fn parse_size(s: &str) -> Result<usize, ParseError> {
    let s = s.trim();
    let s_upper = s.to_uppercase();

    let (num_str, multiplier) = if s_upper.ends_with("GB") {
        (&s[..s.len()-2], 1024 * 1024 * 1024)
    } else if s_upper.ends_with("MB") {
        (&s[..s.len()-2], 1024 * 1024)
    } else if s_upper.ends_with("KB") {
        (&s[..s.len()-2], 1024)
    } else if s_upper.ends_with("B") {
        (&s[..s.len()-1], 1)
    } else {
        // Assume bytes if no unit
        (s, 1)
    };

    let num: usize = num_str.trim().parse()
        .map_err(|_| ParseError::InvalidSize {
            value: s.to_string(),
            reason: "Invalid number".to_string(),
        })?;

    num.checked_mul(multiplier)
        .ok_or_else(|| ParseError::InvalidSize {
            value: s.to_string(),
            reason: "Value too large".to_string(),
        })
}

/// Parse duration strings like "30s", "5m", "1h"
pub fn parse_duration(s: &str) -> Result<Duration, ParseError> {
    let s = s.trim().to_lowercase();

    let (num_str, multiplier_ms) = if s.ends_with("ms") {
        (&s[..s.len()-2], 1u64)
    } else if s.ends_with('s') {
        (&s[..s.len()-1], 1000)
    } else if s.ends_with('m') {
        (&s[..s.len()-1], 60 * 1000)
    } else if s.ends_with('h') {
        (&s[..s.len()-1], 60 * 60 * 1000)
    } else if s.ends_with('d') {
        (&s[..s.len()-1], 24 * 60 * 60 * 1000)
    } else {
        return Err(ParseError::InvalidDuration {
            value: s,
            reason: "Missing unit (ms, s, m, h, d)".to_string(),
        });
    };

    let num: u64 = num_str.trim().parse()
        .map_err(|_| ParseError::InvalidDuration {
            value: s.clone(),
            reason: "Invalid number".to_string(),
        })?;

    Ok(Duration::from_millis(num * multiplier_ms))
}

#[derive(Debug, thiserror::Error)]
pub enum EnvError {
    #[error("Missing environment variable: {name}")]
    MissingVar { name: String },

    #[error("Missing environment variable {name}: {message}")]
    MissingVarWithMessage { name: String, message: String },
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Invalid size value '{value}': {reason}")]
    InvalidSize { value: String, reason: String },

    #[error("Invalid duration value '{value}': {reason}")]
    InvalidDuration { value: String, reason: String },
}
```

---

## 1.2 Graceful Shutdown & Lifecycle Management

### Task: Implement graceful shutdown coordinator

```rust
// crates/fraiseql-runtime/src/lifecycle/shutdown.rs

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::{broadcast, watch, Notify};
use tokio::time::timeout;

/// Coordinates graceful shutdown across all components
pub struct ShutdownCoordinator {
    /// Signal that shutdown has been initiated
    shutdown_initiated: AtomicBool,

    /// Sender for shutdown notification
    shutdown_tx: broadcast::Sender<()>,

    /// Watch channel for readiness state
    ready_tx: watch::Sender<bool>,
    ready_rx: watch::Receiver<bool>,

    /// Count of in-flight requests
    in_flight: AtomicU64,

    /// Notification when all requests complete
    drain_complete: Notify,

    /// Configuration
    config: ShutdownConfig,
}

#[derive(Debug, Clone)]
pub struct ShutdownConfig {
    /// Time to wait for in-flight requests to complete
    pub timeout: Duration,

    /// Delay before starting shutdown (for LB deregistration)
    pub delay: Duration,
}

impl Default for ShutdownConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            delay: Duration::from_secs(5),
        }
    }
}

impl ShutdownCoordinator {
    pub fn new(config: ShutdownConfig) -> Arc<Self> {
        let (shutdown_tx, _) = broadcast::channel(1);
        let (ready_tx, ready_rx) = watch::channel(true);

        Arc::new(Self {
            shutdown_initiated: AtomicBool::new(false),
            shutdown_tx,
            ready_tx,
            ready_rx,
            in_flight: AtomicU64::new(0),
            drain_complete: Notify::new(),
            config,
        })
    }

    /// Subscribe to shutdown signal
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    /// Get readiness watch receiver
    pub fn ready_watch(&self) -> watch::Receiver<bool> {
        self.ready_rx.clone()
    }

    /// Check if system is ready to accept requests
    pub fn is_ready(&self) -> bool {
        *self.ready_rx.borrow()
    }

    /// Check if shutdown has been initiated
    pub fn is_shutting_down(&self) -> bool {
        self.shutdown_initiated.load(Ordering::SeqCst)
    }

    /// Track a new in-flight request
    pub fn request_started(&self) -> Option<RequestGuard> {
        if self.is_shutting_down() {
            return None;
        }

        self.in_flight.fetch_add(1, Ordering::SeqCst);
        Some(RequestGuard { coordinator: self })
    }

    /// Get current in-flight request count
    pub fn in_flight_count(&self) -> u64 {
        self.in_flight.load(Ordering::SeqCst)
    }

    fn request_completed(&self) {
        let prev = self.in_flight.fetch_sub(1, Ordering::SeqCst);
        if prev == 1 && self.is_shutting_down() {
            self.drain_complete.notify_waiters();
        }
    }

    /// Initiate graceful shutdown
    pub async fn shutdown(&self) {
        if self.shutdown_initiated.swap(true, Ordering::SeqCst) {
            // Already shutting down
            return;
        }

        tracing::info!("Initiating graceful shutdown");

        // Step 1: Mark as not ready (stop accepting new requests)
        let _ = self.ready_tx.send(false);
        tracing::info!("Marked as not ready, waiting for load balancer deregistration");

        // Step 2: Wait for load balancer deregistration delay
        tokio::time::sleep(self.config.delay).await;

        // Step 3: Notify all components to shut down
        let _ = self.shutdown_tx.send(());
        tracing::info!("Shutdown signal sent to all components");

        // Step 4: Wait for in-flight requests to complete (with timeout)
        let in_flight = self.in_flight.load(Ordering::SeqCst);
        if in_flight > 0 {
            tracing::info!("Waiting for {} in-flight requests to complete", in_flight);

            let drain_result = timeout(
                self.config.timeout,
                self.wait_for_drain()
            ).await;

            match drain_result {
                Ok(()) => {
                    tracing::info!("All in-flight requests completed");
                }
                Err(_) => {
                    let remaining = self.in_flight.load(Ordering::SeqCst);
                    tracing::warn!(
                        "Shutdown timeout reached with {} requests still in-flight",
                        remaining
                    );
                }
            }
        }

        tracing::info!("Graceful shutdown complete");
    }

    async fn wait_for_drain(&self) {
        while self.in_flight.load(Ordering::SeqCst) > 0 {
            self.drain_complete.notified().await;
        }
    }
}

/// RAII guard for tracking in-flight requests
pub struct RequestGuard<'a> {
    coordinator: &'a ShutdownCoordinator,
}

impl Drop for RequestGuard<'_> {
    fn drop(&mut self) {
        self.coordinator.request_completed();
    }
}

/// Create shutdown signal future from OS signals
pub async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received Ctrl+C signal");
        }
        _ = terminate => {
            tracing::info!("Received SIGTERM signal");
        }
    }
}
```

### Task: Implement health and readiness checks

```rust
// crates/fraiseql-runtime/src/lifecycle/health.rs

use std::sync::Arc;
use axum::{
    extract::State,
    response::{IntoResponse, Response},
    http::StatusCode,
    Json,
};
use serde::Serialize;

use crate::lifecycle::shutdown::ShutdownCoordinator;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: HealthStatus,
    pub checks: Vec<HealthCheck>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Debug, Serialize)]
pub struct HealthCheck {
    pub name: String,
    pub status: HealthStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
}

/// Liveness probe - is the process running?
pub async fn liveness_handler() -> impl IntoResponse {
    StatusCode::OK
}

/// Readiness probe - is the service ready to accept traffic?
pub async fn readiness_handler(
    State(state): State<Arc<AppState>>,
) -> Response {
    // Check if shutting down
    if state.shutdown.is_shutting_down() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: HealthStatus::Unhealthy,
                checks: vec![HealthCheck {
                    name: "shutdown".to_string(),
                    status: HealthStatus::Unhealthy,
                    message: Some("Service is shutting down".to_string()),
                    latency_ms: None,
                }],
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        ).into_response();
    }

    // Perform health checks
    let mut checks = Vec::new();
    let mut overall_status = HealthStatus::Healthy;

    // Database check
    let db_check = check_database(&state).await;
    if db_check.status != HealthStatus::Healthy {
        overall_status = HealthStatus::Degraded;
    }
    checks.push(db_check);

    // Cache check (if configured)
    if state.cache.is_some() {
        let cache_check = check_cache(&state).await;
        if cache_check.status == HealthStatus::Unhealthy {
            overall_status = HealthStatus::Degraded;
        }
        checks.push(cache_check);
    }

    let status_code = match overall_status {
        HealthStatus::Healthy => StatusCode::OK,
        HealthStatus::Degraded => StatusCode::OK, // Still accept traffic
        HealthStatus::Unhealthy => StatusCode::SERVICE_UNAVAILABLE,
    };

    (
        status_code,
        Json(HealthResponse {
            status: overall_status,
            checks,
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }),
    ).into_response()
}

async fn check_database(state: &AppState) -> HealthCheck {
    let start = std::time::Instant::now();

    match sqlx::query("SELECT 1")
        .fetch_one(&state.db)
        .await
    {
        Ok(_) => HealthCheck {
            name: "database".to_string(),
            status: HealthStatus::Healthy,
            message: None,
            latency_ms: Some(start.elapsed().as_millis() as u64),
        },
        Err(e) => HealthCheck {
            name: "database".to_string(),
            status: HealthStatus::Unhealthy,
            message: Some(format!("Connection failed: {}", e)),
            latency_ms: Some(start.elapsed().as_millis() as u64),
        },
    }
}

async fn check_cache(state: &AppState) -> HealthCheck {
    let start = std::time::Instant::now();

    if let Some(cache) = &state.cache {
        match cache.ping().await {
            Ok(_) => HealthCheck {
                name: "cache".to_string(),
                status: HealthStatus::Healthy,
                message: None,
                latency_ms: Some(start.elapsed().as_millis() as u64),
            },
            Err(e) => HealthCheck {
                name: "cache".to_string(),
                status: HealthStatus::Unhealthy,
                message: Some(format!("Connection failed: {}", e)),
                latency_ms: Some(start.elapsed().as_millis() as u64),
            },
        }
    } else {
        HealthCheck {
            name: "cache".to_string(),
            status: HealthStatus::Healthy,
            message: Some("Not configured".to_string()),
            latency_ms: None,
        }
    }
}

/// Startup probe handler for slow-starting services
pub async fn startup_handler(
    State(state): State<Arc<AppState>>,
) -> Response {
    // Check critical dependencies only
    let db_check = check_database(&state).await;

    if db_check.status == HealthStatus::Healthy {
        StatusCode::OK.into_response()
    } else {
        StatusCode::SERVICE_UNAVAILABLE.into_response()
    }
}
```

---

## 1.3 HTTP Server Integration

### Task: Create router builder with dependency injection

```rust
// crates/fraiseql-runtime/src/server/router.rs

use axum::{
    Router,
    routing::{get, post},
    middleware,
    Extension,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

use crate::config::RuntimeConfig;
use crate::state::AppState;
use crate::lifecycle::health::{liveness_handler, readiness_handler, startup_handler};
use crate::middleware::request_tracking::RequestTrackingLayer;

/// Router builder with testable component injection
pub struct RuntimeRouter<S = Arc<AppState>> {
    state: S,
    config: Arc<RuntimeConfig>,
}

impl RuntimeRouter<Arc<AppState>> {
    pub fn new(state: Arc<AppState>) -> Self {
        let config = state.config.clone();
        Self { state, config }
    }
}

impl<S: Clone + Send + Sync + 'static> RuntimeRouter<S> {
    /// Build the complete router with all configured features
    pub fn build(self) -> Router {
        let mut router = Router::new();

        // Lifecycle endpoints (always enabled)
        let lifecycle = &self.config.lifecycle.clone().unwrap_or_default();
        router = router
            .route(&lifecycle.health_path, get(liveness_handler))
            .route(&lifecycle.ready_path, get(readiness_handler))
            .route("/startup", get(startup_handler));

        // Core GraphQL endpoint
        router = router
            .route("/graphql", post(graphql_handler).get(graphql_playground));

        // Metrics endpoint
        if let Some(metrics) = &self.config.metrics {
            if metrics.enabled {
                let path = metrics.path.as_deref().unwrap_or("/metrics");
                router = router.route(path, get(metrics_handler));
            }
        }

        // Webhook routes
        for (name, webhook_config) in &self.config.webhooks {
            let path = webhook_config.path.clone()
                .unwrap_or_else(|| format!("/webhooks/{}", name));

            router = router.route(
                &path,
                post(webhook_handler).layer(Extension(name.clone()))
            );
        }

        // File upload routes
        for (name, file_config) in &self.config.files {
            let path = file_config.path.clone()
                .unwrap_or_else(|| format!("/files/{}", name));

            router = router
                .route(&path, post(file_upload_handler).layer(Extension(name.clone())))
                .route(
                    &format!("{}/:id", path),
                    get(file_get_handler).delete(file_delete_handler)
                );
        }

        // Auth routes
        if self.config.auth.is_some() {
            router = router
                .route("/auth/:provider", get(auth_initiate))
                .route("/auth/:provider/callback", get(auth_callback))
                .route("/auth/refresh", post(auth_refresh))
                .route("/auth/logout", post(auth_logout))
                .route("/auth/me", get(auth_me));
        }

        // Add state
        router.with_state(self.state)
    }
}

/// Builder pattern for testing with mock components
pub struct TestableRouterBuilder {
    config: RuntimeConfig,
}

impl TestableRouterBuilder {
    pub fn new(config: RuntimeConfig) -> Self {
        Self { config }
    }

    pub async fn build_with_mocks(self) -> Router {
        // Build with mock implementations for testing
        // This allows testing routes without real databases/services
        todo!("Implement mock state builder")
    }
}
```

### Task: Create server startup with graceful shutdown

```rust
// crates/fraiseql-runtime/src/server/mod.rs

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::{
    compression::CompressionLayer,
    limit::RequestBodyLimitLayer,
    timeout::TimeoutLayer,
    trace::TraceLayer,
    request_id::{SetRequestIdLayer, MakeRequestUuid},
};

use crate::config::RuntimeConfig;
use crate::config::env::{parse_size, parse_duration};
use crate::state::AppState;
use crate::lifecycle::shutdown::{ShutdownCoordinator, ShutdownConfig, shutdown_signal};
use crate::middleware::admission::AdmissionLayer;

pub mod router;

pub struct RuntimeServer {
    config: RuntimeConfig,
}

impl RuntimeServer {
    pub fn new(config: RuntimeConfig) -> Self {
        Self { config }
    }

    pub async fn run(self) -> Result<(), ServerError> {
        // Initialize tracing
        if let Some(tracing_config) = &self.config.tracing {
            init_tracing(tracing_config)?;
        }

        // Initialize metrics
        if let Some(metrics_config) = &self.config.metrics {
            init_metrics(metrics_config)?;
        }

        // Build shutdown coordinator
        let shutdown_config = ShutdownConfig {
            timeout: self.config.lifecycle.as_ref()
                .map(|l| parse_duration(&l.shutdown_timeout).unwrap_or_default())
                .unwrap_or(std::time::Duration::from_secs(30)),
            delay: self.config.lifecycle.as_ref()
                .map(|l| parse_duration(&l.shutdown_delay).unwrap_or_default())
                .unwrap_or(std::time::Duration::from_secs(5)),
        };
        let shutdown = ShutdownCoordinator::new(shutdown_config);

        // Build application state
        let state = Arc::new(AppState::new(self.config.clone(), shutdown.clone()).await?);

        // Build router
        let router = router::RuntimeRouter::new(state.clone()).build();

        // Apply middleware stack
        let app = self.apply_middleware(router, &state);

        // Create listener
        let addr = SocketAddr::new(
            self.config.server.host.parse()?,
            self.config.server.port
        );

        let listener = TcpListener::bind(addr).await?;
        tracing::info!("FraiseQL runtime listening on {}", addr);

        // Spawn shutdown signal handler
        let shutdown_coordinator = shutdown.clone();
        tokio::spawn(async move {
            shutdown_signal().await;
            shutdown_coordinator.shutdown().await;
        });

        // Run server with graceful shutdown
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                shutdown.subscribe().recv().await.ok();
            })
            .await?;

        Ok(())
    }

    fn apply_middleware(&self, router: Router, state: &Arc<AppState>) -> Router {
        let mut app = router;

        // CORS (outermost - must be first)
        if let Some(cors_config) = &self.config.cors {
            app = app.layer(build_cors_layer(cors_config));
        }

        // Request tracing
        app = app.layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &http::Request<_>| {
                    tracing::info_span!(
                        "http_request",
                        method = %request.method(),
                        uri = %request.uri(),
                        request_id = tracing::field::Empty,
                    )
                })
        );

        // Request ID
        app = app.layer(SetRequestIdLayer::new(MakeRequestUuid));

        // Admission control (backpressure)
        if let Some(limits) = &self.config.server.limits {
            app = app.layer(AdmissionLayer::new(
                limits.max_concurrent_requests,
                limits.max_queue_depth,
                state.shutdown.clone(),
            ));
        }

        // Rate limiting
        if let Some(rate_limit_config) = &self.config.rate_limiting {
            app = app.layer(build_rate_limit_layer(rate_limit_config, state.clone()));
        }

        // Compression
        app = app.layer(CompressionLayer::new());

        // Request size limit
        if let Some(limits) = &self.config.server.limits {
            let max_size = parse_size(&limits.max_request_size).unwrap_or(10 * 1024 * 1024);
            app = app.layer(RequestBodyLimitLayer::new(max_size));
        }

        // Timeout
        if let Some(limits) = &self.config.server.limits {
            let timeout = parse_duration(&limits.request_timeout)
                .unwrap_or(std::time::Duration::from_secs(30));
            app = app.layer(TimeoutLayer::new(timeout));
        }

        app
    }
}

fn init_tracing(config: &TracingConfig) -> Result<(), ServerError> {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.level));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(true);

    let fmt_layer = if config.format == "json" {
        fmt_layer.json().flatten_event(true).boxed()
    } else {
        fmt_layer.boxed()
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .init();

    Ok(())
}

fn init_metrics(config: &MetricsConfig) -> Result<(), ServerError> {
    // Initialize Prometheus metrics exporter
    let builder = metrics_exporter_prometheus::PrometheusBuilder::new();
    builder.install_recorder()?;
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("Failed to bind to address: {0}")]
    Bind(#[from] std::io::Error),

    #[error("Failed to parse address: {0}")]
    AddressParse(#[from] std::net::AddrParseError),

    #[error("Server error: {0}")]
    Server(String),

    #[error("Configuration error: {0}")]
    Config(#[from] fraiseql_error::ConfigError),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}
```

---

## 1.4 Shared Error Crate

### Task: Create `fraiseql-error` crate

```bash
cargo new --lib crates/fraiseql-error
```

### Task: Define core error types with error codes

```rust
// crates/fraiseql-error/src/lib.rs

//! Unified error types for FraiseQL runtime crates.
//!
//! All runtime crates depend on this crate for error handling.

mod config;
mod auth;
mod webhook;
mod file;
mod notification;
mod observer;
mod integration;
mod http;

pub use config::ConfigError;
pub use auth::AuthError;
pub use webhook::WebhookError;
pub use file::FileError;
pub use notification::NotificationError;
pub use observer::ObserverError;
pub use integration::IntegrationError;

// Re-export for convenience
pub use http::{IntoHttpResponse, ErrorResponse};

/// Unified error type wrapping all domain errors
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error(transparent)]
    Auth(#[from] AuthError),

    #[error(transparent)]
    Webhook(#[from] WebhookError),

    #[error(transparent)]
    File(#[from] FileError),

    #[error(transparent)]
    Notification(#[from] NotificationError),

    #[error(transparent)]
    Observer(#[from] ObserverError),

    #[error(transparent)]
    Integration(#[from] IntegrationError),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Rate limit exceeded")]
    RateLimited { retry_after: Option<u64> },

    #[error("Service unavailable: {reason}")]
    ServiceUnavailable { reason: String, retry_after: Option<u64> },

    #[error("Resource not found: {resource}")]
    NotFound { resource: String },

    #[error("Internal error: {message}")]
    Internal { message: String, #[source] source: Option<Box<dyn std::error::Error + Send + Sync>> },
}

impl RuntimeError {
    /// Get the error code for this error
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::Config(e) => e.error_code(),
            Self::Auth(e) => e.error_code(),
            Self::Webhook(e) => e.error_code(),
            Self::File(e) => e.error_code(),
            Self::Notification(e) => e.error_code(),
            Self::Observer(e) => e.error_code(),
            Self::Integration(e) => e.error_code(),
            Self::Database(_) => "database_error",
            Self::RateLimited { .. } => "rate_limited",
            Self::ServiceUnavailable { .. } => "service_unavailable",
            Self::NotFound { .. } => "not_found",
            Self::Internal { .. } => "internal_error",
        }
    }

    /// Get documentation URL for this error
    pub fn docs_url(&self) -> String {
        format!("https://docs.fraiseql.dev/errors#{}", self.error_code())
    }
}
```

### Task: Define domain-specific error types with error codes

```rust
// crates/fraiseql-error/src/config.rs

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Configuration file not found")]
    NotFound,

    #[error("Failed to read configuration file {path}: {source}")]
    ReadError {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to parse configuration: {source}")]
    ParseError {
        #[from]
        source: toml::de::Error,
    },

    #[error("Validation error in {field}: {message}")]
    ValidationError {
        field: String,
        message: String,
    },

    #[error("Missing required environment variable: {name}")]
    MissingEnvVar { name: String },

    #[error("Multiple configuration errors")]
    MultipleErrors { errors: Vec<ConfigError> },
}

impl ConfigError {
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::NotFound => "config_not_found",
            Self::ReadError { .. } => "config_read_error",
            Self::ParseError { .. } => "config_parse_error",
            Self::ValidationError { .. } => "config_validation_error",
            Self::MissingEnvVar { .. } => "config_missing_env",
            Self::MultipleErrors { .. } => "config_multiple_errors",
        }
    }
}
```

```rust
// crates/fraiseql-error/src/auth.rs

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Token expired")]
    TokenExpired,

    #[error("Invalid token: {reason}")]
    InvalidToken { reason: String },

    #[error("Provider error: {provider} - {message}")]
    ProviderError { provider: String, message: String },

    #[error("Invalid OAuth state")]
    InvalidState,

    #[error("User denied authorization")]
    UserDenied,

    #[error("Session not found")]
    SessionNotFound,

    #[error("Session expired")]
    SessionExpired,

    #[error("Insufficient permissions: requires {required}")]
    InsufficientPermissions { required: String },

    #[error("Refresh token invalid or expired")]
    RefreshTokenInvalid,

    #[error("Account locked: {reason}")]
    AccountLocked { reason: String },
}

impl AuthError {
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::InvalidCredentials => "invalid_credentials",
            Self::TokenExpired => "token_expired",
            Self::InvalidToken { .. } => "invalid_token",
            Self::ProviderError { .. } => "auth_provider_error",
            Self::InvalidState => "invalid_oauth_state",
            Self::UserDenied => "user_denied",
            Self::SessionNotFound => "session_not_found",
            Self::SessionExpired => "session_expired",
            Self::InsufficientPermissions { .. } => "insufficient_permissions",
            Self::RefreshTokenInvalid => "refresh_token_invalid",
            Self::AccountLocked { .. } => "account_locked",
        }
    }
}
```

```rust
// crates/fraiseql-error/src/webhook.rs

#[derive(Debug, thiserror::Error)]
pub enum WebhookError {
    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Missing signature header: {header}")]
    MissingSignature { header: String },

    #[error("Timestamp too old: {age_seconds}s (max: {max_seconds}s)")]
    TimestampExpired { age_seconds: u64, max_seconds: u64 },

    #[error("Timestamp in future: {future_seconds}s")]
    TimestampFuture { future_seconds: u64 },

    #[error("Duplicate event: {event_id}")]
    DuplicateEvent { event_id: String },

    #[error("Unknown event type: {event_type}")]
    UnknownEvent { event_type: String },

    #[error("Provider not configured: {provider}")]
    ProviderNotConfigured { provider: String },

    #[error("Payload parse error: {message}")]
    PayloadError { message: String },

    #[error("Idempotency check failed: {message}")]
    IdempotencyError { message: String },
}

impl WebhookError {
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::InvalidSignature => "webhook_invalid_signature",
            Self::MissingSignature { .. } => "webhook_missing_signature",
            Self::TimestampExpired { .. } => "webhook_timestamp_expired",
            Self::TimestampFuture { .. } => "webhook_timestamp_future",
            Self::DuplicateEvent { .. } => "webhook_duplicate_event",
            Self::UnknownEvent { .. } => "webhook_unknown_event",
            Self::ProviderNotConfigured { .. } => "webhook_provider_not_configured",
            Self::PayloadError { .. } => "webhook_payload_error",
            Self::IdempotencyError { .. } => "webhook_idempotency_error",
        }
    }
}
```

```rust
// crates/fraiseql-error/src/file.rs

#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error("File too large: {size} bytes (max: {max} bytes)")]
    TooLarge { size: usize, max: usize },

    #[error("Invalid file type: {got} (allowed: {allowed:?})")]
    InvalidType { got: String, allowed: Vec<String> },

    #[error("MIME type mismatch: declared {declared}, detected {detected}")]
    MimeMismatch { declared: String, detected: String },

    #[error("Storage error: {message}")]
    Storage { message: String, #[source] source: Option<Box<dyn std::error::Error + Send + Sync>> },

    #[error("Processing error: {message}")]
    Processing { message: String },

    #[error("File not found: {id}")]
    NotFound { id: String },

    #[error("Virus detected: {details}")]
    VirusDetected { details: String },

    #[error("Upload quota exceeded")]
    QuotaExceeded,
}

impl FileError {
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::TooLarge { .. } => "file_too_large",
            Self::InvalidType { .. } => "file_invalid_type",
            Self::MimeMismatch { .. } => "file_mime_mismatch",
            Self::Storage { .. } => "file_storage_error",
            Self::Processing { .. } => "file_processing_error",
            Self::NotFound { .. } => "file_not_found",
            Self::VirusDetected { .. } => "file_virus_detected",
            Self::QuotaExceeded => "file_quota_exceeded",
        }
    }
}
```

```rust
// crates/fraiseql-error/src/notification.rs

use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum NotificationError {
    #[error("Configuration error: {message}")]
    Configuration { message: String },

    #[error("Provider error: {provider} - {message}")]
    Provider { provider: String, message: String },

    #[error("Provider unavailable: {provider}")]
    ProviderUnavailable { provider: String, retry_after: Option<Duration> },

    #[error("Invalid input: {message}")]
    InvalidInput { message: String },

    #[error("Template error: {message}")]
    Template { message: String },

    #[error("Rate limited by provider: retry after {seconds} seconds")]
    ProviderRateLimited { provider: String, seconds: u64 },

    #[error("Circuit breaker open for provider: {provider}")]
    CircuitOpen { provider: String, retry_after: Duration },

    #[error("Timeout sending notification")]
    Timeout,
}

impl NotificationError {
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::Configuration { .. } => "notification_config_error",
            Self::Provider { .. } => "notification_provider_error",
            Self::ProviderUnavailable { .. } => "notification_provider_unavailable",
            Self::InvalidInput { .. } => "notification_invalid_input",
            Self::Template { .. } => "notification_template_error",
            Self::ProviderRateLimited { .. } => "notification_rate_limited",
            Self::CircuitOpen { .. } => "notification_circuit_open",
            Self::Timeout => "notification_timeout",
        }
    }
}
```

```rust
// crates/fraiseql-error/src/observer.rs

#[derive(Debug, thiserror::Error)]
pub enum ObserverError {
    #[error("Invalid condition: {message}")]
    InvalidCondition { message: String },

    #[error("Template error: {message}")]
    TemplateError { message: String },

    #[error("Action failed: {action} - {message}")]
    ActionFailed { action: String, message: String },

    #[error("Invalid configuration: {message}")]
    InvalidConfig { message: String },

    #[error("Event processing failed: {message}")]
    ProcessingFailed { message: String },

    #[error("Max retries exceeded for event {event_id}")]
    MaxRetriesExceeded { event_id: String },

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}

impl ObserverError {
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::InvalidCondition { .. } => "observer_invalid_condition",
            Self::TemplateError { .. } => "observer_template_error",
            Self::ActionFailed { .. } => "observer_action_failed",
            Self::InvalidConfig { .. } => "observer_invalid_config",
            Self::ProcessingFailed { .. } => "observer_processing_failed",
            Self::MaxRetriesExceeded { .. } => "observer_max_retries",
            Self::Database(_) => "observer_database_error",
        }
    }
}
```

```rust
// crates/fraiseql-error/src/integration.rs

#[derive(Debug, thiserror::Error)]
pub enum IntegrationError {
    #[error("Search provider error: {provider} - {message}")]
    Search { provider: String, message: String },

    #[error("Cache error: {message}")]
    Cache { message: String },

    #[error("Queue error: {message}")]
    Queue { message: String },

    #[error("Connection failed: {service}")]
    ConnectionFailed { service: String },

    #[error("Timeout: {operation}")]
    Timeout { operation: String },
}

impl IntegrationError {
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::Search { .. } => "integration_search_error",
            Self::Cache { .. } => "integration_cache_error",
            Self::Queue { .. } => "integration_queue_error",
            Self::ConnectionFailed { .. } => "integration_connection_failed",
            Self::Timeout { .. } => "integration_timeout",
        }
    }
}
```

### Task: Implement HTTP response conversion

```rust
// crates/fraiseql-error/src/http.rs

use axum::{
    response::{IntoResponse, Response},
    http::StatusCode,
    Json,
};
use serde::Serialize;

use crate::{RuntimeError, AuthError, WebhookError, FileError};

/// Error response format (consistent across all endpoints)
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub error_description: String,
    pub error_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<u64>,
}

impl ErrorResponse {
    pub fn new(error: impl Into<String>, description: impl Into<String>, code: impl Into<String>) -> Self {
        let code = code.into();
        Self {
            error: error.into(),
            error_description: description.into(),
            error_uri: Some(format!("https://docs.fraiseql.dev/errors#{}", code)),
            error_code: code,
            details: None,
            retry_after: None,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn with_retry_after(mut self, seconds: u64) -> Self {
        self.retry_after = Some(seconds);
        self
    }
}

impl IntoResponse for RuntimeError {
    fn into_response(self) -> Response {
        let error_code = self.error_code();

        let (status, mut response) = match &self {
            RuntimeError::Config(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorResponse::new("configuration_error", self.to_string(), error_code)
            ),

            RuntimeError::Auth(e) => {
                let status = match e {
                    AuthError::InsufficientPermissions { .. } => StatusCode::FORBIDDEN,
                    AuthError::AccountLocked { .. } => StatusCode::FORBIDDEN,
                    _ => StatusCode::UNAUTHORIZED,
                };
                (status, ErrorResponse::new("authentication_error", self.to_string(), error_code))
            },

            RuntimeError::Webhook(e) => {
                let status = match e {
                    WebhookError::InvalidSignature => StatusCode::UNAUTHORIZED,
                    WebhookError::MissingSignature { .. } => StatusCode::BAD_REQUEST,
                    WebhookError::DuplicateEvent { .. } => StatusCode::OK,
                    _ => StatusCode::BAD_REQUEST,
                };
                (status, ErrorResponse::new("webhook_error", self.to_string(), error_code))
            },

            RuntimeError::File(e) => {
                let status = match e {
                    FileError::TooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
                    FileError::InvalidType { .. } | FileError::MimeMismatch { .. } => {
                        StatusCode::UNSUPPORTED_MEDIA_TYPE
                    }
                    FileError::NotFound { .. } => StatusCode::NOT_FOUND,
                    FileError::VirusDetected { .. } => StatusCode::UNPROCESSABLE_ENTITY,
                    FileError::QuotaExceeded => StatusCode::INSUFFICIENT_STORAGE,
                    _ => StatusCode::BAD_REQUEST,
                };
                (status, ErrorResponse::new("file_error", self.to_string(), error_code))
            },

            RuntimeError::Notification(e) => {
                use crate::NotificationError::*;
                let status = match e {
                    CircuitOpen { .. } | ProviderUnavailable { .. } => {
                        StatusCode::SERVICE_UNAVAILABLE
                    }
                    ProviderRateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
                    InvalidInput { .. } => StatusCode::BAD_REQUEST,
                    _ => StatusCode::INTERNAL_SERVER_ERROR,
                };
                (status, ErrorResponse::new("notification_error", self.to_string(), error_code))
            },

            RuntimeError::RateLimited { retry_after } => {
                let mut resp = ErrorResponse::new(
                    "rate_limited",
                    "Rate limit exceeded",
                    error_code
                );
                if let Some(secs) = retry_after {
                    resp = resp.with_retry_after(*secs);
                }
                (StatusCode::TOO_MANY_REQUESTS, resp)
            },

            RuntimeError::ServiceUnavailable { retry_after, .. } => {
                let mut resp = ErrorResponse::new(
                    "service_unavailable",
                    self.to_string(),
                    error_code
                );
                if let Some(secs) = retry_after {
                    resp = resp.with_retry_after(*secs);
                }
                (StatusCode::SERVICE_UNAVAILABLE, resp)
            },

            RuntimeError::NotFound { .. } => (
                StatusCode::NOT_FOUND,
                ErrorResponse::new("not_found", self.to_string(), error_code)
            ),

            RuntimeError::Database(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorResponse::new("database_error", "A database error occurred", error_code)
            ),

            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorResponse::new("internal_error", "An internal error occurred", error_code)
            ),
        };

        // Add Retry-After header for rate limits
        let mut resp = (status, Json(response)).into_response();
        if let Some(retry_after) = self.retry_after_header() {
            resp.headers_mut().insert(
                "Retry-After",
                retry_after.parse().unwrap()
            );
        }

        resp
    }
}

impl RuntimeError {
    fn retry_after_header(&self) -> Option<String> {
        match self {
            Self::RateLimited { retry_after: Some(secs) } => Some(secs.to_string()),
            Self::ServiceUnavailable { retry_after: Some(secs), .. } => Some(secs.to_string()),
            _ => None,
        }
    }
}
```

---

## 1.5 Application State with Dependency Injection

### Task: Create application state with injectable components

```rust
// crates/fraiseql-runtime/src/state.rs

use std::sync::Arc;
use sqlx::PgPool;
use tokio::sync::RwLock;

use crate::config::RuntimeConfig;
use crate::lifecycle::shutdown::ShutdownCoordinator;
use fraiseql_error::RuntimeError;

/// Shared application state with injectable components
pub struct AppState {
    /// Configuration
    pub config: Arc<RuntimeConfig>,

    /// Database connection pool
    pub db: PgPool,

    /// Read replica pools (for load balancing)
    pub replicas: Vec<PgPool>,

    /// Cache client (optional, injectable)
    pub cache: Option<Arc<dyn CacheClient>>,

    /// Rate limiter state
    pub rate_limiter: Option<Arc<dyn RateLimiter>>,

    /// Webhook idempotency store (injectable)
    pub idempotency: Arc<dyn IdempotencyStore>,

    /// Template engine for notifications
    pub templates: Arc<TemplateEngine>,

    /// Shutdown coordinator
    pub shutdown: Arc<ShutdownCoordinator>,
}

/// Trait for cache operations (injectable for testing)
#[async_trait::async_trait]
pub trait CacheClient: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, RuntimeError>;
    async fn set(&self, key: &str, value: &[u8], ttl: Option<std::time::Duration>) -> Result<(), RuntimeError>;
    async fn delete(&self, key: &str) -> Result<(), RuntimeError>;
    async fn ping(&self) -> Result<(), RuntimeError>;
}

/// Trait for rate limiting (injectable for testing)
#[async_trait::async_trait]
pub trait RateLimiter: Send + Sync {
    async fn check(&self, key: &str, limit: u32, window: std::time::Duration) -> Result<RateLimitResult, RuntimeError>;
}

pub struct RateLimitResult {
    pub allowed: bool,
    pub remaining: u32,
    pub reset_at: std::time::SystemTime,
}

/// Trait for idempotency checking (injectable for testing)
#[async_trait::async_trait]
pub trait IdempotencyStore: Send + Sync {
    async fn check_and_store(&self, key: &str, ttl: std::time::Duration) -> Result<bool, RuntimeError>;
    async fn get_result(&self, key: &str) -> Result<Option<serde_json::Value>, RuntimeError>;
    async fn store_result(&self, key: &str, result: &serde_json::Value) -> Result<(), RuntimeError>;
}

impl AppState {
    pub async fn new(
        config: RuntimeConfig,
        shutdown: Arc<ShutdownCoordinator>,
    ) -> Result<Self, RuntimeError> {
        // Connect to database
        let db_url = std::env::var(&config.database.url_env)?;
        let db = PgPool::connect(&db_url).await?;

        // Connect to replicas
        let mut replicas = Vec::new();
        for replica in &config.database.replicas {
            let url = std::env::var(&replica.url_env)?;
            replicas.push(PgPool::connect(&url).await?);
        }

        // Initialize cache
        let cache: Option<Arc<dyn CacheClient>> = if let Some(cache_config) = &config.cache {
            Some(Arc::new(RedisCacheClient::new(cache_config).await?))
        } else {
            None
        };

        // Initialize rate limiter
        let rate_limiter: Option<Arc<dyn RateLimiter>> = if let Some(rate_config) = &config.rate_limiting {
            Some(build_rate_limiter(rate_config, cache.clone())?)
        } else {
            None
        };

        // Initialize idempotency store
        let idempotency: Arc<dyn IdempotencyStore> = Arc::new(
            PostgresIdempotencyStore::new(db.clone())
        );

        // Initialize template engine
        let templates = Arc::new(TemplateEngine::new()?);

        Ok(Self {
            config: Arc::new(config),
            db,
            replicas,
            cache,
            rate_limiter,
            idempotency,
            templates,
            shutdown,
        })
    }

    /// Create state with mock components for testing
    #[cfg(any(test, feature = "testing"))]
    pub fn with_mocks(
        config: RuntimeConfig,
        db: PgPool,
        cache: Option<Arc<dyn CacheClient>>,
        rate_limiter: Option<Arc<dyn RateLimiter>>,
        idempotency: Arc<dyn IdempotencyStore>,
    ) -> Self {
        let shutdown = ShutdownCoordinator::new(Default::default());
        Self {
            config: Arc::new(config),
            db,
            replicas: Vec::new(),
            cache,
            rate_limiter,
            idempotency,
            templates: Arc::new(TemplateEngine::new().unwrap()),
            shutdown,
        }
    }

    /// Get a database connection for reads (load-balanced across replicas)
    pub fn read_connection(&self) -> &PgPool {
        if self.replicas.is_empty() {
            &self.db
        } else {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let idx = COUNTER.fetch_add(1, Ordering::Relaxed) % self.replicas.len();
            &self.replicas[idx]
        }
    }

    /// Get primary database connection (for writes)
    pub fn write_connection(&self) -> &PgPool {
        &self.db
    }
}
```

---

## 1.6 Admission Control Middleware

### Task: Implement admission control for backpressure

```rust
// crates/fraiseql-runtime/src/middleware/admission.rs

use std::sync::Arc;
use std::task::{Context, Poll};
use axum::{
    body::Body,
    http::{Request, Response, StatusCode},
    response::IntoResponse,
};
use tower::{Layer, Service};
use pin_project::pin_project;
use std::future::Future;
use std::pin::Pin;

use crate::lifecycle::shutdown::ShutdownCoordinator;
use crate::resilience::backpressure::AdmissionController;

/// Layer for admission control
#[derive(Clone)]
pub struct AdmissionLayer {
    controller: Arc<AdmissionController>,
    shutdown: Arc<ShutdownCoordinator>,
}

impl AdmissionLayer {
    pub fn new(
        max_concurrent: usize,
        max_queue_depth: usize,
        shutdown: Arc<ShutdownCoordinator>,
    ) -> Self {
        Self {
            controller: Arc::new(AdmissionController::new(max_concurrent, max_queue_depth as u64)),
            shutdown,
        }
    }
}

impl<S> Layer<S> for AdmissionLayer {
    type Service = AdmissionService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AdmissionService {
            inner,
            controller: self.controller.clone(),
            shutdown: self.shutdown.clone(),
        }
    }
}

/// Service wrapper for admission control
#[derive(Clone)]
pub struct AdmissionService<S> {
    inner: S,
    controller: Arc<AdmissionController>,
    shutdown: Arc<ShutdownCoordinator>,
}

impl<S, ReqBody> Service<Request<ReqBody>> for AdmissionService<S>
where
    S: Service<Request<ReqBody>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send,
    ReqBody: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = AdmissionFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        // Check if shutting down
        if self.shutdown.is_shutting_down() {
            return AdmissionFuture::Rejected(service_unavailable_response());
        }

        // Try to acquire admission permit
        match self.controller.try_acquire() {
            Some(permit) => {
                // Track request in shutdown coordinator
                let request_guard = self.shutdown.request_started();

                if request_guard.is_none() {
                    // Shutdown started between checks
                    return AdmissionFuture::Rejected(service_unavailable_response());
                }

                AdmissionFuture::Permitted {
                    future: self.inner.call(req),
                    _permit: permit,
                    _request_guard: request_guard,
                }
            }
            None => {
                // System overloaded
                AdmissionFuture::Rejected(overloaded_response())
            }
        }
    }
}

#[pin_project(project = AdmissionFutureProj)]
pub enum AdmissionFuture<F> {
    Permitted {
        #[pin]
        future: F,
        _permit: crate::resilience::backpressure::AdmissionPermit<'static>,
        _request_guard: Option<crate::lifecycle::shutdown::RequestGuard<'static>>,
    },
    Rejected(Response<Body>),
}

impl<F, E> Future for AdmissionFuture<F>
where
    F: Future<Output = Result<Response<Body>, E>>,
{
    type Output = Result<Response<Body>, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project() {
            AdmissionFutureProj::Permitted { future, .. } => future.poll(cx),
            AdmissionFutureProj::Rejected(response) => {
                Poll::Ready(Ok(std::mem::take(response)))
            }
        }
    }
}

fn service_unavailable_response() -> Response<Body> {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [("Retry-After", "5")],
        "Service is shutting down"
    ).into_response()
}

fn overloaded_response() -> Response<Body> {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [("Retry-After", "1")],
        "Server is overloaded, please retry"
    ).into_response()
}
```

---

## 1.7 Tests

### Task: Unit tests for configuration

```rust
// crates/fraiseql-runtime/src/config/tests.rs

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_parse_minimal_config() {
        let toml = r#"
            [server]
            port = 4000

            [database]
            url_env = "DATABASE_URL"
        "#;

        std::env::set_var("DATABASE_URL", "postgres://localhost/test");

        let config: RuntimeConfig = toml::from_str(toml).unwrap();

        assert_eq!(config.server.port, 4000);
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.database.url_env, "DATABASE_URL");
        assert_eq!(config.database.pool_size, 10);
    }

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("10MB").unwrap(), 10 * 1024 * 1024);
        assert_eq!(parse_size("1GB").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_size("500KB").unwrap(), 500 * 1024);
        assert_eq!(parse_size("1000").unwrap(), 1000);
        assert_eq!(parse_size("100B").unwrap(), 100);
    }

    #[test]
    fn test_parse_size_invalid() {
        assert!(parse_size("abc").is_err());
        assert!(parse_size("-10MB").is_err());
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
        assert_eq!(parse_duration("100ms").unwrap(), Duration::from_millis(100));
        assert_eq!(parse_duration("1d").unwrap(), Duration::from_secs(86400));
    }

    #[test]
    fn test_parse_duration_invalid() {
        assert!(parse_duration("30").is_err()); // Missing unit
        assert!(parse_duration("abc").is_err());
    }

    #[test]
    fn test_env_resolution_with_default() {
        std::env::remove_var("NONEXISTENT_VAR");
        let result = resolve_env_value("${NONEXISTENT_VAR:-default_value}").unwrap();
        assert_eq!(result, "default_value");
    }

    #[test]
    fn test_env_resolution_without_default() {
        std::env::set_var("EXISTING_VAR", "actual_value");
        let result = resolve_env_value("${EXISTING_VAR:-default}").unwrap();
        assert_eq!(result, "actual_value");
    }

    #[test]
    fn test_validation_missing_env_var() {
        let toml = r#"
            [server]
            port = 4000

            [database]
            url_env = "NONEXISTENT_DB_URL"
        "#;

        std::env::remove_var("NONEXISTENT_DB_URL");

        let config: RuntimeConfig = toml::from_str(toml).unwrap();
        let result = ConfigValidator::new(&config).validate();

        assert!(!result.is_ok());
        assert!(result.errors.iter().any(|e| matches!(e, ConfigError::MissingEnvVar { .. })));
    }

    #[test]
    fn test_validation_cross_field() {
        let toml = r#"
            [server]
            port = 4000

            [database]
            url_env = "DATABASE_URL"

            [observers.test]
            entity = "users"
            events = ["insert"]

            [[observers.test.actions]]
            type = "email"
            template = "welcome"
        "#;

        std::env::set_var("DATABASE_URL", "postgres://localhost/test");

        let config: RuntimeConfig = toml::from_str(toml).unwrap();
        let result = ConfigValidator::new(&config).validate();

        // Should fail because email action requires notifications config
        assert!(!result.is_ok());
    }
}
```

### Task: Integration tests for graceful shutdown

```rust
// crates/fraiseql-runtime/tests/shutdown_test.rs

use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

use fraiseql_runtime::lifecycle::shutdown::{ShutdownCoordinator, ShutdownConfig};

#[tokio::test]
async fn test_graceful_shutdown_waits_for_requests() {
    let config = ShutdownConfig {
        timeout: Duration::from_secs(5),
        delay: Duration::from_millis(100),
    };
    let coordinator = ShutdownCoordinator::new(config);

    // Simulate an in-flight request
    let guard = coordinator.request_started().unwrap();
    assert_eq!(coordinator.in_flight_count(), 1);

    // Start shutdown in background
    let shutdown_coordinator = coordinator.clone();
    let shutdown_handle = tokio::spawn(async move {
        shutdown_coordinator.shutdown().await;
    });

    // Wait a bit for shutdown to start
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Should be shutting down
    assert!(coordinator.is_shutting_down());

    // Complete the request
    drop(guard);
    assert_eq!(coordinator.in_flight_count(), 0);

    // Shutdown should complete
    timeout(Duration::from_secs(1), shutdown_handle)
        .await
        .expect("Shutdown should complete")
        .expect("Shutdown task should not panic");
}

#[tokio::test]
async fn test_shutdown_rejects_new_requests() {
    let config = ShutdownConfig {
        timeout: Duration::from_secs(1),
        delay: Duration::from_millis(0),
    };
    let coordinator = ShutdownCoordinator::new(config);

    // Start shutdown
    let shutdown_coordinator = coordinator.clone();
    tokio::spawn(async move {
        shutdown_coordinator.shutdown().await;
    });

    // Wait for shutdown to initiate
    tokio::time::sleep(Duration::from_millis(50)).await;

    // New requests should be rejected
    assert!(coordinator.request_started().is_none());
}

#[tokio::test]
async fn test_readiness_changes_on_shutdown() {
    let config = ShutdownConfig::default();
    let coordinator = ShutdownCoordinator::new(config);

    assert!(coordinator.is_ready());

    let shutdown_coordinator = coordinator.clone();
    tokio::spawn(async move {
        shutdown_coordinator.shutdown().await;
    });

    // Wait for readiness to change
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(!coordinator.is_ready());
}
```

---

## Acceptance Criteria

- [ ] `RuntimeConfig` struct parses all TOML configuration options
- [ ] Environment variable resolution works for `*_env` fields and `${VAR}` syntax
- [ ] Environment variable defaults work (`${VAR:-default}`)
- [ ] Size and duration parsing works correctly with all units
- [ ] Configuration validation catches:
  - [ ] Missing required fields
  - [ ] Invalid field values
  - [ ] Missing environment variables
  - [ ] Cross-field validation errors (e.g., OAuth without callback URL)
- [ ] HTTP server starts and binds to configured port
- [ ] Middleware pipeline applies in correct order
- [ ] Error types convert to appropriate HTTP responses with error codes
- [ ] Error responses include documentation URLs
- [ ] Health check endpoint returns 200 with component status
- [ ] Readiness endpoint returns 503 during shutdown
- [ ] Graceful shutdown:
  - [ ] Waits for in-flight requests
  - [ ] Times out after configured duration
  - [ ] Sends SIGTERM/SIGINT handling
  - [ ] Marks service as not ready before draining
- [ ] Admission control rejects requests when overloaded
- [ ] All traits have mock implementations for testing
- [ ] Unit tests pass for all configuration parsing
- [ ] Integration tests pass for shutdown behavior

---

## Files to Create

```
crates/fraiseql-error/
├── Cargo.toml
└── src/
    ├── lib.rs              # Re-exports all error types
    ├── config.rs           # ConfigError
    ├── auth.rs             # AuthError
    ├── webhook.rs          # WebhookError
    ├── file.rs             # FileError
    ├── notification.rs     # NotificationError
    ├── observer.rs         # ObserverError
    ├── integration.rs      # IntegrationError
    └── http.rs             # IntoResponse impls

crates/fraiseql-runtime/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── config/
│   │   ├── mod.rs
│   │   ├── loader.rs
│   │   ├── validation.rs   # NEW: Comprehensive validation
│   │   ├── env.rs
│   │   ├── webhooks.rs
│   │   ├── files.rs
│   │   ├── auth.rs
│   │   ├── notifications.rs
│   │   ├── observers.rs
│   │   └── tests.rs
│   ├── server/
│   │   ├── mod.rs
│   │   ├── router.rs
│   │   └── handlers.rs
│   ├── lifecycle/          # NEW: Lifecycle management
│   │   ├── mod.rs
│   │   ├── shutdown.rs     # Graceful shutdown coordinator
│   │   └── health.rs       # Health/readiness checks
│   ├── middleware/
│   │   ├── mod.rs
│   │   └── admission.rs    # NEW: Admission control
│   ├── template/
│   │   ├── mod.rs
│   │   ├── filters.rs
│   │   └── tests.rs
│   ├── resilience/
│   │   ├── mod.rs
│   │   ├── circuit_breaker.rs
│   │   ├── retry.rs
│   │   └── backpressure.rs # NEW: Backpressure handling
│   ├── state.rs
│   └── error.rs
└── tests/
    ├── shutdown_test.rs    # NEW: Shutdown integration tests
    ├── config_test.rs
    └── fixtures/
        └── full_config.toml
```

---

## DO NOT

- Do not implement actual webhook verification yet (Phase 3)
- Do not implement actual file upload yet (Phase 4)
- Do not implement actual OAuth yet (Phase 5)
- Do not add provider-specific code yet
- Do not optimize prematurely - focus on correctness
- Do not skip writing tests for new functionality
- Do not use `unwrap()` in production code paths
