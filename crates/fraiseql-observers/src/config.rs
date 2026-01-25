//! Configuration structures for the observer system.
//!
//! This module provides configuration for:
//! - Observer runtime (channel capacity, concurrency, etc.)
//! - Transport selection (PostgreSQL LISTEN/NOTIFY, NATS, in-memory)
//! - NATS JetStream settings (retention, deduplication, etc.)
//! - Bridge configuration (PostgreSQL → NATS)
//!
//! # Configuration Sources
//!
//! Configuration can be loaded from:
//! 1. TOML files (base configuration)
//! 2. Environment variables (overrides)
//!
//! Environment variable precedence: `FRAISEQL_*` > TOML > defaults

use std::{collections::HashMap, env};

use serde::{Deserialize, Serialize};

use crate::error::{ObserverError, Result};

// ============================================================================
// Transport Configuration
// ============================================================================

/// Transport type for event sourcing
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    /// PostgreSQL LISTEN/NOTIFY (default, existing behavior)
    #[default]
    Postgres,
    /// NATS JetStream (distributed, scalable)
    Nats,
    /// In-memory (testing only)
    InMemory,
}

impl TransportKind {
    /// Load from environment variable `FRAISEQL_OBSERVER_TRANSPORT`
    #[must_use]
    pub fn from_env() -> Option<Self> {
        env::var("FRAISEQL_OBSERVER_TRANSPORT")
            .ok()
            .and_then(|v| match v.to_lowercase().as_str() {
                "postgres" | "postgresql" => Some(Self::Postgres),
                "nats" => Some(Self::Nats),
                "in_memory" | "inmemory" | "memory" => Some(Self::InMemory),
                _ => None,
            })
    }
}

/// Top-level transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    /// Transport type (postgres, nats, in_memory)
    #[serde(default)]
    pub transport: TransportKind,

    /// Run the PostgreSQL → NATS bridge in-process (default: false)
    #[serde(default)]
    pub run_bridge: bool,

    /// Run observer executors in-process (default: true)
    #[serde(default = "default_true")]
    pub run_executors: bool,

    /// NATS-specific configuration (only used when transport = nats)
    #[serde(default)]
    pub nats: NatsTransportConfig,

    /// Bridge-specific configuration (only used when run_bridge = true)
    #[serde(default)]
    pub bridge: BridgeTransportConfig,
}

const fn default_true() -> bool {
    true
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            transport:     TransportKind::default(),
            run_bridge:    false,
            run_executors: true,
            nats:          NatsTransportConfig::default(),
            bridge:        BridgeTransportConfig::default(),
        }
    }
}

impl TransportConfig {
    /// Apply environment variable overrides
    #[must_use]
    pub fn with_env_overrides(mut self) -> Self {
        if let Some(transport) = TransportKind::from_env() {
            self.transport = transport;
        }
        if let Ok(v) = env::var("FRAISEQL_NATS_ENABLE_BRIDGE") {
            self.run_bridge = v.eq_ignore_ascii_case("true") || v == "1";
        }
        if let Ok(v) = env::var("FRAISEQL_NATS_RUN_EXECUTORS") {
            self.run_executors = v.eq_ignore_ascii_case("true") || v == "1";
        }
        self.nats = self.nats.with_env_overrides();
        self.bridge = self.bridge.with_env_overrides();
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        // NATS transport requires NATS URL
        if self.transport == TransportKind::Nats && self.nats.url.is_empty() {
            return Err(ObserverError::InvalidConfig {
                message: "NATS transport requires nats.url to be set".to_string(),
            });
        }

        // Bridge requires NATS transport
        if self.run_bridge && self.transport != TransportKind::Nats {
            return Err(ObserverError::InvalidConfig {
                message: "run_bridge=true requires transport=nats".to_string(),
            });
        }

        self.nats.validate()?;
        self.bridge.validate()?;
        Ok(())
    }
}

// ============================================================================
// NATS Transport Configuration
// ============================================================================

/// NATS JetStream transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatsTransportConfig {
    /// NATS server URL (e.g., "nats://localhost:4222")
    /// Supports multiple servers: "nats://nats-1:4222,nats://nats-2:4222"
    #[serde(default = "default_nats_url")]
    pub url: String,

    /// Subject prefix for entity change events (default: "fraiseql.mutation")
    #[serde(default = "default_subject_prefix")]
    pub subject_prefix: String,

    /// Durable consumer name for this observer instance
    /// Workers with same name compete for messages (load balancing)
    #[serde(default = "default_consumer_name")]
    pub consumer_name: String,

    /// JetStream stream name (default: "fraiseql_events")
    #[serde(default = "default_stream_name")]
    pub stream_name: String,

    /// JetStream configuration
    #[serde(default)]
    pub jetstream: JetStreamConfig,
}

fn default_nats_url() -> String {
    env::var("FRAISEQL_NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string())
}

fn default_subject_prefix() -> String {
    env::var("FRAISEQL_NATS_SUBJECT_PREFIX").unwrap_or_else(|_| "fraiseql.mutation".to_string())
}

fn default_consumer_name() -> String {
    env::var("FRAISEQL_NATS_CONSUMER_NAME")
        .unwrap_or_else(|_| "fraiseql_observer_worker".to_string())
}

fn default_stream_name() -> String {
    env::var("FRAISEQL_NATS_STREAM_NAME").unwrap_or_else(|_| "fraiseql_events".to_string())
}

impl Default for NatsTransportConfig {
    fn default() -> Self {
        Self {
            url:            default_nats_url(),
            subject_prefix: default_subject_prefix(),
            consumer_name:  default_consumer_name(),
            stream_name:    default_stream_name(),
            jetstream:      JetStreamConfig::default(),
        }
    }
}

impl NatsTransportConfig {
    /// Apply environment variable overrides
    #[must_use]
    pub fn with_env_overrides(mut self) -> Self {
        if let Ok(url) = env::var("FRAISEQL_NATS_URL") {
            self.url = url;
        }
        if let Ok(prefix) = env::var("FRAISEQL_NATS_SUBJECT_PREFIX") {
            self.subject_prefix = prefix;
        }
        if let Ok(name) = env::var("FRAISEQL_NATS_CONSUMER_NAME") {
            self.consumer_name = name;
        }
        if let Ok(name) = env::var("FRAISEQL_NATS_STREAM_NAME") {
            self.stream_name = name;
        }
        self.jetstream = self.jetstream.with_env_overrides();
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.subject_prefix.is_empty() {
            return Err(ObserverError::InvalidConfig {
                message: "nats.subject_prefix cannot be empty".to_string(),
            });
        }
        if self.consumer_name.is_empty() {
            return Err(ObserverError::InvalidConfig {
                message: "nats.consumer_name cannot be empty".to_string(),
            });
        }
        self.jetstream.validate()?;
        Ok(())
    }
}

/// JetStream stream and consumer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JetStreamConfig {
    /// Message deduplication window in minutes (default: 5, recommended: 2-10)
    #[serde(default = "default_dedup_window_minutes")]
    pub dedup_window_minutes: u64,

    /// Maximum message age in days (default: 7)
    #[serde(default = "default_max_age_days")]
    pub max_age_days: u64,

    /// Maximum number of messages in stream (default: 10_000_000)
    #[serde(default = "default_max_msgs")]
    pub max_msgs: i64,

    /// Maximum stream size in bytes (default: 10GB)
    #[serde(default = "default_max_bytes")]
    pub max_bytes: i64,

    /// Message acknowledgment timeout in seconds (default: 30)
    #[serde(default = "default_ack_wait_secs")]
    pub ack_wait_secs: u64,

    /// Maximum redelivery attempts before giving up (default: 3)
    #[serde(default = "default_max_deliver")]
    pub max_deliver: i64,
}

const fn default_dedup_window_minutes() -> u64 {
    5
}

const fn default_max_age_days() -> u64 {
    7
}

const fn default_max_msgs() -> i64 {
    10_000_000
}

const fn default_max_bytes() -> i64 {
    10 * 1024 * 1024 * 1024 // 10 GB
}

const fn default_ack_wait_secs() -> u64 {
    30
}

const fn default_max_deliver() -> i64 {
    3
}

impl Default for JetStreamConfig {
    fn default() -> Self {
        Self {
            dedup_window_minutes: default_dedup_window_minutes(),
            max_age_days:         default_max_age_days(),
            max_msgs:             default_max_msgs(),
            max_bytes:            default_max_bytes(),
            ack_wait_secs:        default_ack_wait_secs(),
            max_deliver:          default_max_deliver(),
        }
    }
}

impl JetStreamConfig {
    /// Apply environment variable overrides
    #[must_use]
    pub fn with_env_overrides(mut self) -> Self {
        if let Ok(v) = env::var("FRAISEQL_NATS_DEDUP_WINDOW_MINUTES") {
            if let Ok(mins) = v.parse() {
                self.dedup_window_minutes = mins;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_NATS_MAX_AGE_DAYS") {
            if let Ok(days) = v.parse() {
                self.max_age_days = days;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_NATS_MAX_MSGS") {
            if let Ok(msgs) = v.parse() {
                self.max_msgs = msgs;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_NATS_MAX_BYTES") {
            if let Ok(bytes) = v.parse() {
                self.max_bytes = bytes;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_NATS_ACK_WAIT_SECS") {
            if let Ok(secs) = v.parse() {
                self.ack_wait_secs = secs;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_NATS_MAX_DELIVER") {
            if let Ok(max) = v.parse() {
                self.max_deliver = max;
            }
        }
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.dedup_window_minutes == 0 || self.dedup_window_minutes > 60 {
            return Err(ObserverError::InvalidConfig {
                message: "jetstream.dedup_window_minutes must be between 1 and 60".to_string(),
            });
        }
        if self.ack_wait_secs == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "jetstream.ack_wait_secs must be > 0".to_string(),
            });
        }
        if self.max_deliver <= 0 {
            return Err(ObserverError::InvalidConfig {
                message: "jetstream.max_deliver must be > 0".to_string(),
            });
        }
        Ok(())
    }
}

// ============================================================================
// Bridge Configuration
// ============================================================================

/// PostgreSQL → NATS bridge configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeTransportConfig {
    /// Transport name for checkpoint storage (default: "pg_to_nats")
    #[serde(default = "default_bridge_transport_name")]
    pub transport_name: String,

    /// Batch size for fetching change log entries (default: 100)
    #[serde(default = "default_bridge_batch_size")]
    pub batch_size: usize,

    /// Poll interval in seconds when no NOTIFY received (default: 1)
    #[serde(default = "default_bridge_poll_interval_secs")]
    pub poll_interval_secs: u64,

    /// PostgreSQL NOTIFY channel name (default: "fraiseql_events")
    #[serde(default = "default_bridge_notify_channel")]
    pub notify_channel: String,
}

fn default_bridge_transport_name() -> String {
    env::var("FRAISEQL_BRIDGE_TRANSPORT_NAME").unwrap_or_else(|_| "pg_to_nats".to_string())
}

const fn default_bridge_batch_size() -> usize {
    100
}

const fn default_bridge_poll_interval_secs() -> u64 {
    1
}

fn default_bridge_notify_channel() -> String {
    env::var("FRAISEQL_BRIDGE_NOTIFY_CHANNEL").unwrap_or_else(|_| "fraiseql_events".to_string())
}

impl Default for BridgeTransportConfig {
    fn default() -> Self {
        Self {
            transport_name:     default_bridge_transport_name(),
            batch_size:         default_bridge_batch_size(),
            poll_interval_secs: default_bridge_poll_interval_secs(),
            notify_channel:     default_bridge_notify_channel(),
        }
    }
}

impl BridgeTransportConfig {
    /// Apply environment variable overrides
    #[must_use]
    pub fn with_env_overrides(mut self) -> Self {
        if let Ok(name) = env::var("FRAISEQL_BRIDGE_TRANSPORT_NAME") {
            self.transport_name = name;
        }
        if let Ok(v) = env::var("FRAISEQL_BRIDGE_BATCH_SIZE") {
            if let Ok(size) = v.parse() {
                self.batch_size = size;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_BRIDGE_POLL_INTERVAL_SECS") {
            if let Ok(secs) = v.parse() {
                self.poll_interval_secs = secs;
            }
        }
        if let Ok(channel) = env::var("FRAISEQL_BRIDGE_NOTIFY_CHANNEL") {
            self.notify_channel = channel;
        }
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.transport_name.is_empty() {
            return Err(ObserverError::InvalidConfig {
                message: "bridge.transport_name cannot be empty".to_string(),
            });
        }
        if self.batch_size == 0 || self.batch_size > 10000 {
            return Err(ObserverError::InvalidConfig {
                message: "bridge.batch_size must be between 1 and 10000".to_string(),
            });
        }
        if self.poll_interval_secs == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "bridge.poll_interval_secs must be > 0".to_string(),
            });
        }
        Ok(())
    }

    /// Convert to `BridgeConfig` for use with `PostgresNatsBridge`
    #[cfg(feature = "nats")]
    #[must_use]
    pub fn to_bridge_config(&self) -> crate::transport::BridgeConfig {
        crate::transport::BridgeConfig {
            transport_name:     self.transport_name.clone(),
            batch_size:         self.batch_size,
            poll_interval_secs: self.poll_interval_secs,
            notify_channel:     self.notify_channel.clone(),
        }
    }
}

// ============================================================================
// Redis Configuration (Phase 8: Deduplication + Caching)
// ============================================================================

/// Redis configuration for deduplication and caching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisConfig {
    /// Redis connection URL (e.g., "redis://localhost:6379")
    #[serde(default = "default_redis_url")]
    pub url: String,

    /// Maximum number of connections in pool (default: 10)
    #[serde(default = "default_redis_pool_size")]
    pub pool_size: usize,

    /// Connection timeout in seconds (default: 5)
    #[serde(default = "default_redis_connect_timeout_secs")]
    pub connect_timeout_secs: u64,

    /// Command timeout in seconds (default: 2)
    #[serde(default = "default_redis_command_timeout_secs")]
    pub command_timeout_secs: u64,

    /// Deduplication window in seconds (default: 300 = 5 minutes)
    #[serde(default = "default_dedup_window_secs")]
    pub dedup_window_secs: u64,

    /// Cache TTL in seconds (default: 60)
    #[serde(default = "default_cache_ttl_secs")]
    pub cache_ttl_secs: u64,
}

fn default_redis_url() -> String {
    env::var("FRAISEQL_REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string())
}

const fn default_redis_pool_size() -> usize {
    10
}

const fn default_redis_connect_timeout_secs() -> u64 {
    5
}

const fn default_redis_command_timeout_secs() -> u64 {
    2
}

const fn default_dedup_window_secs() -> u64 {
    300 // 5 minutes
}

const fn default_cache_ttl_secs() -> u64 {
    60 // 1 minute
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url:                  default_redis_url(),
            pool_size:            default_redis_pool_size(),
            connect_timeout_secs: default_redis_connect_timeout_secs(),
            command_timeout_secs: default_redis_command_timeout_secs(),
            dedup_window_secs:    default_dedup_window_secs(),
            cache_ttl_secs:       default_cache_ttl_secs(),
        }
    }
}

impl RedisConfig {
    /// Apply environment variable overrides
    #[must_use]
    pub fn with_env_overrides(mut self) -> Self {
        if let Ok(url) = env::var("FRAISEQL_REDIS_URL") {
            self.url = url;
        }
        if let Ok(v) = env::var("FRAISEQL_REDIS_POOL_SIZE") {
            if let Ok(size) = v.parse() {
                self.pool_size = size;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_REDIS_CONNECT_TIMEOUT_SECS") {
            if let Ok(secs) = v.parse() {
                self.connect_timeout_secs = secs;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_REDIS_COMMAND_TIMEOUT_SECS") {
            if let Ok(secs) = v.parse() {
                self.command_timeout_secs = secs;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_REDIS_DEDUP_WINDOW_SECS") {
            if let Ok(secs) = v.parse() {
                self.dedup_window_secs = secs;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_REDIS_CACHE_TTL_SECS") {
            if let Ok(secs) = v.parse() {
                self.cache_ttl_secs = secs;
            }
        }
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.url.is_empty() {
            return Err(ObserverError::InvalidConfig {
                message: "redis.url cannot be empty".to_string(),
            });
        }
        if self.pool_size == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "redis.pool_size must be > 0".to_string(),
            });
        }
        if self.connect_timeout_secs == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "redis.connect_timeout_secs must be > 0".to_string(),
            });
        }
        if self.command_timeout_secs == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "redis.command_timeout_secs must be > 0".to_string(),
            });
        }
        if self.dedup_window_secs == 0 || self.dedup_window_secs > 3600 {
            return Err(ObserverError::InvalidConfig {
                message: "redis.dedup_window_secs must be between 1 and 3600".to_string(),
            });
        }
        if self.cache_ttl_secs == 0 || self.cache_ttl_secs > 3600 {
            return Err(ObserverError::InvalidConfig {
                message: "redis.cache_ttl_secs must be between 1 and 3600".to_string(),
            });
        }
        Ok(())
    }
}

// ============================================================================
// Job Queue Configuration (Phase 8.6)
// ============================================================================

/// Job queue configuration for asynchronous action execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobQueueConfig {
    /// Redis URL for job queue backend (e.g., "redis://localhost:6379")
    /// If not specified, uses the main redis config URL
    #[serde(default = "default_job_queue_url")]
    pub url: String,

    /// Number of jobs to fetch per batch (default: 100)
    #[serde(default = "default_job_queue_batch_size")]
    pub batch_size: usize,

    /// Batch timeout in seconds (how long to wait before flushing partial batch)
    #[serde(default = "default_job_queue_batch_timeout_secs")]
    pub batch_timeout_secs: u64,

    /// Maximum number of retry attempts per job (default: 5)
    #[serde(default = "default_job_queue_max_retries")]
    pub max_retries: u32,

    /// Worker concurrency (number of jobs to execute in parallel)
    #[serde(default = "default_job_queue_worker_concurrency")]
    pub worker_concurrency: usize,

    /// Poll interval when queue is empty, in milliseconds (default: 1000)
    #[serde(default = "default_job_queue_poll_interval_ms")]
    pub poll_interval_ms: u64,

    /// Initial retry delay in milliseconds (default: 100)
    #[serde(default = "default_job_queue_initial_delay_ms")]
    pub initial_delay_ms: u64,

    /// Maximum retry delay in milliseconds (default: 30000)
    #[serde(default = "default_job_queue_max_delay_ms")]
    pub max_delay_ms: u64,
}

fn default_job_queue_url() -> String {
    env::var("FRAISEQL_JOB_QUEUE_URL")
        .unwrap_or_else(|_| "redis://localhost:6379".to_string())
}

const fn default_job_queue_batch_size() -> usize {
    100
}

const fn default_job_queue_batch_timeout_secs() -> u64 {
    5
}

const fn default_job_queue_max_retries() -> u32 {
    5
}

const fn default_job_queue_worker_concurrency() -> usize {
    10
}

const fn default_job_queue_poll_interval_ms() -> u64 {
    1000
}

const fn default_job_queue_initial_delay_ms() -> u64 {
    100
}

const fn default_job_queue_max_delay_ms() -> u64 {
    30000
}

impl Default for JobQueueConfig {
    fn default() -> Self {
        Self {
            url: default_job_queue_url(),
            batch_size: default_job_queue_batch_size(),
            batch_timeout_secs: default_job_queue_batch_timeout_secs(),
            max_retries: default_job_queue_max_retries(),
            worker_concurrency: default_job_queue_worker_concurrency(),
            poll_interval_ms: default_job_queue_poll_interval_ms(),
            initial_delay_ms: default_job_queue_initial_delay_ms(),
            max_delay_ms: default_job_queue_max_delay_ms(),
        }
    }
}

impl JobQueueConfig {
    /// Apply environment variable overrides
    #[must_use]
    pub fn with_env_overrides(mut self) -> Self {
        if let Ok(url) = env::var("FRAISEQL_JOB_QUEUE_URL") {
            self.url = url;
        }
        if let Ok(v) = env::var("FRAISEQL_JOB_QUEUE_BATCH_SIZE") {
            if let Ok(size) = v.parse() {
                self.batch_size = size;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_JOB_QUEUE_BATCH_TIMEOUT_SECS") {
            if let Ok(secs) = v.parse() {
                self.batch_timeout_secs = secs;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_JOB_QUEUE_MAX_RETRIES") {
            if let Ok(retries) = v.parse() {
                self.max_retries = retries;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_JOB_QUEUE_WORKER_CONCURRENCY") {
            if let Ok(concurrency) = v.parse() {
                self.worker_concurrency = concurrency;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_JOB_QUEUE_POLL_INTERVAL_MS") {
            if let Ok(ms) = v.parse() {
                self.poll_interval_ms = ms;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_JOB_QUEUE_INITIAL_DELAY_MS") {
            if let Ok(ms) = v.parse() {
                self.initial_delay_ms = ms;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_JOB_QUEUE_MAX_DELAY_MS") {
            if let Ok(ms) = v.parse() {
                self.max_delay_ms = ms;
            }
        }
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.url.is_empty() {
            return Err(ObserverError::InvalidConfig {
                message: "job_queue.url cannot be empty".to_string(),
            });
        }
        if self.batch_size == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "job_queue.batch_size must be > 0".to_string(),
            });
        }
        if self.batch_timeout_secs == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "job_queue.batch_timeout_secs must be > 0".to_string(),
            });
        }
        if self.max_retries == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "job_queue.max_retries must be > 0".to_string(),
            });
        }
        if self.worker_concurrency == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "job_queue.worker_concurrency must be > 0".to_string(),
            });
        }
        Ok(())
    }
}

// ============================================================================
// Performance Configuration (Phase 8: Feature Toggles)
// ============================================================================

/// Performance optimization features
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Enable Redis-based event deduplication (requires redis config)
    #[serde(default)]
    pub enable_dedup: bool,

    /// Enable Redis-based action result caching (requires redis config)
    #[serde(default)]
    pub enable_caching: bool,

    /// Enable concurrent action execution within observers
    #[serde(default = "default_true")]
    pub enable_concurrent: bool,

    /// Maximum concurrent actions per observer (default: 10)
    #[serde(default = "default_max_concurrent_actions")]
    pub max_concurrent_actions: usize,

    /// Concurrent execution timeout in milliseconds (default: 30000)
    #[serde(default = "default_concurrent_timeout_ms")]
    pub concurrent_timeout_ms: u64,
}

const fn default_max_concurrent_actions() -> usize {
    10
}

const fn default_concurrent_timeout_ms() -> u64 {
    30000 // 30 seconds
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            enable_dedup:           false,
            enable_caching:         false,
            enable_concurrent:      true,
            max_concurrent_actions: default_max_concurrent_actions(),
            concurrent_timeout_ms:  default_concurrent_timeout_ms(),
        }
    }
}

impl PerformanceConfig {
    /// Apply environment variable overrides
    #[must_use]
    pub fn with_env_overrides(mut self) -> Self {
        if let Ok(v) = env::var("FRAISEQL_ENABLE_DEDUP") {
            self.enable_dedup = v.eq_ignore_ascii_case("true") || v == "1";
        }
        if let Ok(v) = env::var("FRAISEQL_ENABLE_CACHING") {
            self.enable_caching = v.eq_ignore_ascii_case("true") || v == "1";
        }
        if let Ok(v) = env::var("FRAISEQL_ENABLE_CONCURRENT") {
            self.enable_concurrent = v.eq_ignore_ascii_case("true") || v == "1";
        }
        if let Ok(v) = env::var("FRAISEQL_MAX_CONCURRENT_ACTIONS") {
            if let Ok(max) = v.parse() {
                self.max_concurrent_actions = max;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_CONCURRENT_TIMEOUT_MS") {
            if let Ok(ms) = v.parse() {
                self.concurrent_timeout_ms = ms;
            }
        }
        self
    }

    /// Validate the configuration
    pub fn validate(&self, redis_configured: bool) -> Result<()> {
        // Dedup requires Redis
        if self.enable_dedup && !redis_configured {
            return Err(ObserverError::InvalidConfig {
                message: "performance.enable_dedup=true requires redis configuration".to_string(),
            });
        }
        // Caching requires Redis
        if self.enable_caching && !redis_configured {
            return Err(ObserverError::InvalidConfig {
                message: "performance.enable_caching=true requires redis configuration".to_string(),
            });
        }
        if self.max_concurrent_actions == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "performance.max_concurrent_actions must be > 0".to_string(),
            });
        }
        if self.concurrent_timeout_ms == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "performance.concurrent_timeout_ms must be > 0".to_string(),
            });
        }
        Ok(())
    }
}

// ============================================================================
// ClickHouse Configuration (Phase 9.4: Analytics Sink)
// ============================================================================

/// ClickHouse sink configuration for analytics events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClickHouseConfig {
    /// ClickHouse HTTP endpoint (default: "http://localhost:8123")
    #[serde(default = "default_clickhouse_url")]
    pub url: String,

    /// Database name (default: "default")
    #[serde(default = "default_clickhouse_database")]
    pub database: String,

    /// Table name (default: "fraiseql_events")
    #[serde(default = "default_clickhouse_table")]
    pub table: String,

    /// Batch size before flushing (default: 10000)
    #[serde(default = "default_clickhouse_batch_size")]
    pub batch_size: usize,

    /// Batch timeout in seconds (default: 5)
    #[serde(default = "default_clickhouse_batch_timeout_secs")]
    pub batch_timeout_secs: u64,

    /// Maximum number of retries for transient errors (default: 3)
    #[serde(default = "default_clickhouse_max_retries")]
    pub max_retries: usize,
}

fn default_clickhouse_url() -> String {
    env::var("FRAISEQL_CLICKHOUSE_URL")
        .unwrap_or_else(|_| "http://localhost:8123".to_string())
}

fn default_clickhouse_database() -> String {
    env::var("FRAISEQL_CLICKHOUSE_DATABASE")
        .unwrap_or_else(|_| "default".to_string())
}

fn default_clickhouse_table() -> String {
    env::var("FRAISEQL_CLICKHOUSE_TABLE")
        .unwrap_or_else(|_| "fraiseql_events".to_string())
}

const fn default_clickhouse_batch_size() -> usize {
    10_000
}

const fn default_clickhouse_batch_timeout_secs() -> u64 {
    5
}

const fn default_clickhouse_max_retries() -> usize {
    3
}

impl Default for ClickHouseConfig {
    fn default() -> Self {
        Self {
            url: default_clickhouse_url(),
            database: default_clickhouse_database(),
            table: default_clickhouse_table(),
            batch_size: default_clickhouse_batch_size(),
            batch_timeout_secs: default_clickhouse_batch_timeout_secs(),
            max_retries: default_clickhouse_max_retries(),
        }
    }
}

impl ClickHouseConfig {
    /// Apply environment variable overrides
    #[must_use]
    pub fn with_env_overrides(mut self) -> Self {
        if let Ok(url) = env::var("FRAISEQL_CLICKHOUSE_URL") {
            self.url = url;
        }
        if let Ok(database) = env::var("FRAISEQL_CLICKHOUSE_DATABASE") {
            self.database = database;
        }
        if let Ok(table) = env::var("FRAISEQL_CLICKHOUSE_TABLE") {
            self.table = table;
        }
        if let Ok(batch_size) = env::var("FRAISEQL_CLICKHOUSE_BATCH_SIZE") {
            if let Ok(size) = batch_size.parse() {
                self.batch_size = size;
            }
        }
        if let Ok(timeout) = env::var("FRAISEQL_CLICKHOUSE_BATCH_TIMEOUT_SECS") {
            if let Ok(secs) = timeout.parse() {
                self.batch_timeout_secs = secs;
            }
        }
        if let Ok(retries) = env::var("FRAISEQL_CLICKHOUSE_MAX_RETRIES") {
            if let Ok(count) = retries.parse() {
                self.max_retries = count;
            }
        }
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.url.is_empty() {
            return Err(ObserverError::InvalidConfig {
                message: "clickhouse.url cannot be empty".to_string(),
            });
        }
        if self.database.is_empty() {
            return Err(ObserverError::InvalidConfig {
                message: "clickhouse.database cannot be empty".to_string(),
            });
        }
        if self.table.is_empty() {
            return Err(ObserverError::InvalidConfig {
                message: "clickhouse.table cannot be empty".to_string(),
            });
        }
        if self.batch_size == 0 || self.batch_size > 100_000 {
            return Err(ObserverError::InvalidConfig {
                message: "clickhouse.batch_size must be between 1 and 100,000".to_string(),
            });
        }
        if self.batch_timeout_secs == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "clickhouse.batch_timeout_secs must be greater than 0".to_string(),
            });
        }
        Ok(())
    }
}

// ============================================================================
// Observer Runtime Configuration
// ============================================================================

/// Observer runtime configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObserverRuntimeConfig {
    /// Transport configuration (postgres, nats, in_memory)
    #[serde(default)]
    pub transport: TransportConfig,

    /// Redis configuration (for dedup + caching)
    #[serde(default)]
    pub redis: Option<RedisConfig>,

    /// ClickHouse configuration (for analytics sink, Phase 9.4)
    #[serde(default)]
    pub clickhouse: Option<ClickHouseConfig>,

    /// Job queue configuration (for async action execution, Phase 8.6)
    #[serde(default)]
    pub job_queue: Option<JobQueueConfig>,

    /// Performance optimization features (Phase 8)
    #[serde(default)]
    pub performance: PerformanceConfig,

    /// Channel buffer size for incoming events (default: 1000)
    #[serde(default = "default_channel_capacity")]
    pub channel_capacity: usize,

    /// Maximum concurrent action executions (default: 50)
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: usize,

    /// What to do when channel is full
    #[serde(default)]
    pub overflow_policy: OverflowPolicy,

    /// Backlog threshold for alerts (default: 500)
    #[serde(default = "default_backlog_threshold")]
    pub backlog_alert_threshold: usize,

    /// Graceful shutdown timeout (default: "30s")
    #[serde(default = "default_shutdown_timeout")]
    pub shutdown_timeout: String,

    /// Observer definitions
    #[serde(default)]
    pub observers: HashMap<String, ObserverDefinition>,
}

const fn default_channel_capacity() -> usize {
    1000
}

const fn default_max_concurrency() -> usize {
    50
}

const fn default_backlog_threshold() -> usize {
    500
}

fn default_shutdown_timeout() -> String {
    "30s".to_string()
}

/// What to do when the event channel is full
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverflowPolicy {
    /// Drop new events when channel is full (default)
    #[default]
    Drop,
    /// Block sender (can cause issues with PG listener)
    Block,
    /// Drop oldest events to make room
    DropOldest,
}

/// Single observer definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObserverDefinition {
    /// Event type this observer watches (INSERT, UPDATE, DELETE, CUSTOM)
    pub event_type: String,

    /// Entity type this observer watches (e.g., "Order", "User")
    pub entity: String,

    /// Optional condition that must be true for actions to execute
    #[serde(default)]
    pub condition: Option<String>,

    /// Actions to execute when observer is triggered
    pub actions: Vec<ActionConfig>,

    /// Retry configuration
    #[serde(default)]
    pub retry: RetryConfig,

    /// Failure handling policy
    #[serde(default)]
    pub on_failure: FailurePolicy,
}

/// Retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (default: 3)
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,

    /// Initial retry delay in milliseconds (default: 100)
    #[serde(default = "default_initial_delay")]
    pub initial_delay_ms: u64,

    /// Maximum retry delay in milliseconds (default: 30000)
    #[serde(default = "default_max_delay")]
    pub max_delay_ms: u64,

    /// Backoff strategy
    #[serde(default)]
    pub backoff_strategy: BackoffStrategy,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts:     default_max_attempts(),
            initial_delay_ms: default_initial_delay(),
            max_delay_ms:     default_max_delay(),
            backoff_strategy: BackoffStrategy::default(),
        }
    }
}

const fn default_max_attempts() -> u32 {
    3
}

const fn default_initial_delay() -> u64 {
    100
}

const fn default_max_delay() -> u64 {
    30000
}

/// Backoff strategy for retries
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackoffStrategy {
    /// Exponential backoff (2^attempt * `initial_delay`)
    #[default]
    Exponential,
    /// Linear backoff (attempt * `initial_delay`)
    Linear,
    /// Fixed delay between retries
    Fixed,
}

/// What to do when an action fails permanently
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailurePolicy {
    /// Log the error (default)
    #[default]
    Log,
    /// Send an alert
    Alert,
    /// Move to dead letter queue for manual retry
    Dlq,
}

/// Action configuration (tagged union)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionConfig {
    /// HTTP POST webhook to external URL
    Webhook {
        /// URL to POST to
        url:           Option<String>,
        /// Environment variable containing the URL
        url_env:       Option<String>,
        /// Optional HTTP headers
        #[serde(default)]
        headers:       HashMap<String, String>,
        /// Template for request body
        #[serde(default)]
        body_template: Option<String>,
    },

    /// Send message to Slack webhook
    Slack {
        /// Slack webhook URL
        webhook_url:      Option<String>,
        /// Environment variable containing webhook URL
        webhook_url_env:  Option<String>,
        /// Channel to send to (if not in webhook URL)
        #[serde(default)]
        channel:          Option<String>,
        /// Message template
        #[serde(default)]
        message_template: Option<String>,
    },

    /// Send email via SMTP
    Email {
        /// Recipient email address
        to:               Option<String>,
        /// Template for recipient (e.g., "{{ data.email }}")
        to_template:      Option<String>,
        /// Email subject
        subject:          Option<String>,
        /// Subject template
        subject_template: Option<String>,
        /// Email body template
        body_template:    Option<String>,
        /// Reply-to address
        #[serde(default)]
        reply_to:         Option<String>,
    },

    /// Send SMS (stub for Phase 6, full implementation later)
    Sms {
        /// Phone number to send to
        phone:            Option<String>,
        /// Template for phone number
        phone_template:   Option<String>,
        /// Message template
        message_template: Option<String>,
    },

    /// Send push notification (stub for Phase 6)
    Push {
        /// Device token
        device_token:   Option<String>,
        /// Title template
        title_template: Option<String>,
        /// Body template
        body_template:  Option<String>,
    },

    /// Update search index (stub for Phase 6)
    Search {
        /// Index name
        index:       String,
        /// Document ID template
        id_template: Option<String>,
    },

    /// Invalidate cache (stub for Phase 6)
    Cache {
        /// Cache key pattern
        key_pattern: String,
        /// Action: "invalidate" or "refresh"
        action:      String,
    },
}

impl ActionConfig {
    /// Get the action type name
    #[must_use]
    pub const fn action_type(&self) -> &'static str {
        match self {
            ActionConfig::Webhook { .. } => "webhook",
            ActionConfig::Slack { .. } => "slack",
            ActionConfig::Email { .. } => "email",
            ActionConfig::Sms { .. } => "sms",
            ActionConfig::Push { .. } => "push",
            ActionConfig::Search { .. } => "search",
            ActionConfig::Cache { .. } => "cache",
        }
    }

    /// Validate the action configuration
    pub fn validate(&self) -> Result<()> {
        match self {
            ActionConfig::Webhook {
                url,
                url_env,
                body_template,
                ..
            } => {
                if url.is_none() && url_env.is_none() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Webhook action requires 'url' or 'url_env'".to_string(),
                    });
                }
                if body_template.as_ref().is_some_and(std::string::String::is_empty) {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Webhook body_template cannot be empty".to_string(),
                    });
                }
                Ok(())
            },
            ActionConfig::Slack {
                webhook_url,
                webhook_url_env,
                ..
            } => {
                if webhook_url.is_none() && webhook_url_env.is_none() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Slack action requires 'webhook_url' or 'webhook_url_env'"
                            .to_string(),
                    });
                }
                Ok(())
            },
            ActionConfig::Email {
                to,
                to_template,
                subject,
                body_template,
                ..
            } => {
                if to.is_none() && to_template.is_none() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Email action requires 'to' or 'to_template'".to_string(),
                    });
                }
                if subject.is_none() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Email action requires 'subject'".to_string(),
                    });
                }
                if body_template.is_none() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Email action requires 'body_template'".to_string(),
                    });
                }
                Ok(())
            },
            ActionConfig::Sms {
                phone,
                phone_template,
                message_template,
            } => {
                if phone.is_none() && phone_template.is_none() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "SMS action requires 'phone' or 'phone_template'".to_string(),
                    });
                }
                if message_template.is_none() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "SMS action requires 'message_template'".to_string(),
                    });
                }
                Ok(())
            },
            ActionConfig::Push {
                device_token,
                title_template,
                body_template,
            } => {
                if device_token.is_none() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Push action requires 'device_token'".to_string(),
                    });
                }
                if title_template.is_none() || body_template.is_none() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Push action requires 'title_template' and 'body_template'"
                            .to_string(),
                    });
                }
                Ok(())
            },
            ActionConfig::Search { index, .. } => {
                if index.is_empty() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Search action requires 'index'".to_string(),
                    });
                }
                Ok(())
            },
            ActionConfig::Cache {
                key_pattern,
                action,
            } => {
                if key_pattern.is_empty() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Cache action requires 'key_pattern'".to_string(),
                    });
                }
                if action != "invalidate" && action != "refresh" {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Cache action must be 'invalidate' or 'refresh'".to_string(),
                    });
                }
                Ok(())
            },
        }
    }
}

/// Multi-listener configuration for high-availability setup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiListenerConfig {
    /// Enable multi-listener coordination (default: false)
    #[serde(default)]
    pub enabled: bool,

    /// Unique listener ID for this instance (default: random UUID)
    #[serde(default = "default_listener_id")]
    pub listener_id: String,

    /// Lease duration in milliseconds (default: 30000)
    #[serde(default = "default_lease_duration_ms")]
    pub lease_duration_ms: u64,

    /// Health check interval in milliseconds (default: 5000)
    #[serde(default = "default_health_check_interval_ms")]
    pub health_check_interval_ms: u64,

    /// Failover threshold in milliseconds (default: 60000)
    #[serde(default = "default_failover_threshold_ms")]
    pub failover_threshold_ms: u64,

    /// Maximum listeners in group (default: 10)
    #[serde(default = "default_max_listeners")]
    pub max_listeners: usize,
}

fn default_listener_id() -> String {
    format!("listener-{}", uuid::Uuid::new_v4())
}

const fn default_lease_duration_ms() -> u64 {
    30000
}

const fn default_health_check_interval_ms() -> u64 {
    5000
}

const fn default_failover_threshold_ms() -> u64 {
    60000
}

const fn default_max_listeners() -> usize {
    10
}

impl Default for MultiListenerConfig {
    fn default() -> Self {
        Self {
            enabled:                  false,
            listener_id:              default_listener_id(),
            lease_duration_ms:        default_lease_duration_ms(),
            health_check_interval_ms: default_health_check_interval_ms(),
            failover_threshold_ms:    default_failover_threshold_ms(),
            max_listeners:            default_max_listeners(),
        }
    }
}

impl MultiListenerConfig {
    /// Create a new multi-listener config with default values
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.lease_duration_ms == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "lease_duration_ms must be > 0".to_string(),
            });
        }

        if self.health_check_interval_ms == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "health_check_interval_ms must be > 0".to_string(),
            });
        }

        if self.failover_threshold_ms < self.health_check_interval_ms {
            return Err(ObserverError::InvalidConfig {
                message: "failover_threshold_ms must be >= health_check_interval_ms".to_string(),
            });
        }

        if self.max_listeners == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "max_listeners must be > 0".to_string(),
            });
        }

        Ok(())
    }

    /// Enable multi-listener coordination
    #[must_use]
    pub const fn enable(mut self) -> Self {
        self.enabled = true;
        self
    }

    /// Set listener ID
    #[must_use]
    pub fn with_listener_id(mut self, listener_id: String) -> Self {
        self.listener_id = listener_id;
        self
    }

    /// Set lease duration
    #[must_use]
    pub const fn with_lease_duration_ms(mut self, lease_duration_ms: u64) -> Self {
        self.lease_duration_ms = lease_duration_ms;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_observer_runtime_config_defaults() {
        let _config = ObserverRuntimeConfig {
            transport:               TransportConfig::default(),
            redis:                   None,
            clickhouse:              None,
            job_queue:               None,
            performance:             PerformanceConfig::default(),
            channel_capacity:        0,
            max_concurrency:         0,
            overflow_policy:         OverflowPolicy::Drop,
            backlog_alert_threshold: 0,
            shutdown_timeout:        String::new(),
            observers:               HashMap::new(),
        };

        assert_eq!(default_channel_capacity(), 1000);
        assert_eq!(default_max_concurrency(), 50);
        assert_eq!(default_backlog_threshold(), 500);
        assert_eq!(default_shutdown_timeout(), "30s");
    }

    #[test]
    fn test_transport_kind_default() {
        let kind = TransportKind::default();
        assert_eq!(kind, TransportKind::Postgres);
    }

    #[test]
    fn test_transport_config_default() {
        let config = TransportConfig::default();
        assert_eq!(config.transport, TransportKind::Postgres);
        assert!(!config.run_bridge);
        assert!(config.run_executors);
    }

    #[test]
    fn test_transport_config_validation() {
        // Valid postgres config
        let config = TransportConfig::default();
        assert!(config.validate().is_ok());

        // Invalid: NATS transport without URL
        let config = TransportConfig {
            transport: TransportKind::Nats,
            nats: NatsTransportConfig {
                url: String::new(),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // Invalid: run_bridge=true with postgres transport
        let config = TransportConfig {
            transport: TransportKind::Postgres,
            run_bridge: true,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_nats_transport_config_default() {
        let config = NatsTransportConfig::default();
        assert!(config.url.contains("localhost:4222"));
        assert_eq!(config.subject_prefix, "fraiseql.mutation");
        assert_eq!(config.consumer_name, "fraiseql_observer_worker");
    }

    #[test]
    fn test_jetstream_config_default() {
        let config = JetStreamConfig::default();
        assert_eq!(config.dedup_window_minutes, 5);
        assert_eq!(config.max_age_days, 7);
        assert_eq!(config.max_msgs, 10_000_000);
        assert_eq!(config.ack_wait_secs, 30);
        assert_eq!(config.max_deliver, 3);
    }

    #[test]
    fn test_jetstream_config_validation() {
        // Valid config
        let config = JetStreamConfig::default();
        assert!(config.validate().is_ok());

        // Invalid: dedup window = 0
        let config = JetStreamConfig {
            dedup_window_minutes: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // Invalid: dedup window > 60
        let config = JetStreamConfig {
            dedup_window_minutes: 61,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // Invalid: ack_wait = 0
        let config = JetStreamConfig {
            ack_wait_secs: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_bridge_transport_config_default() {
        let config = BridgeTransportConfig::default();
        assert_eq!(config.transport_name, "pg_to_nats");
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.poll_interval_secs, 1);
        assert_eq!(config.notify_channel, "fraiseql_events");
    }

    #[test]
    fn test_bridge_transport_config_validation() {
        // Valid config
        let config = BridgeTransportConfig::default();
        assert!(config.validate().is_ok());

        // Invalid: empty transport_name
        let config = BridgeTransportConfig {
            transport_name: String::new(),
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // Invalid: batch_size = 0
        let config = BridgeTransportConfig {
            batch_size: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // Invalid: batch_size > 10000
        let config = BridgeTransportConfig {
            batch_size: 10001,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // Invalid: poll_interval = 0
        let config = BridgeTransportConfig {
            poll_interval_secs: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_retry_config_defaults() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_delay_ms, 100);
        assert_eq!(config.max_delay_ms, 30000);
    }

    #[test]
    fn test_action_type_names() {
        assert_eq!(
            ActionConfig::Webhook {
                url:           None,
                url_env:       None,
                headers:       HashMap::new(),
                body_template: None,
            }
            .action_type(),
            "webhook"
        );

        assert_eq!(
            ActionConfig::Email {
                to:               None,
                to_template:      None,
                subject:          None,
                subject_template: None,
                body_template:    None,
                reply_to:         None,
            }
            .action_type(),
            "email"
        );
    }

    #[test]
    fn test_webhook_action_validation() {
        let invalid = ActionConfig::Webhook {
            url:           None,
            url_env:       None,
            headers:       HashMap::new(),
            body_template: None,
        };

        assert!(invalid.validate().is_err());

        let valid = ActionConfig::Webhook {
            url:           Some("https://example.com".to_string()),
            url_env:       None,
            headers:       HashMap::new(),
            body_template: Some("{}".to_string()),
        };

        assert!(valid.validate().is_ok());
    }

    #[test]
    fn test_email_action_validation() {
        let invalid = ActionConfig::Email {
            to:               None,
            to_template:      None,
            subject:          None,
            subject_template: None,
            body_template:    None,
            reply_to:         None,
        };

        assert!(invalid.validate().is_err());

        let valid = ActionConfig::Email {
            to:               Some("user@example.com".to_string()),
            to_template:      None,
            subject:          Some("Test".to_string()),
            subject_template: None,
            body_template:    Some("Body".to_string()),
            reply_to:         None,
        };

        assert!(valid.validate().is_ok());
    }

    #[test]
    fn test_multi_listener_config_defaults() {
        let config = MultiListenerConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.lease_duration_ms, 30000);
        assert_eq!(config.health_check_interval_ms, 5000);
        assert_eq!(config.failover_threshold_ms, 60000);
        assert_eq!(config.max_listeners, 10);
    }

    #[test]
    fn test_multi_listener_config_validation() {
        let valid_config = MultiListenerConfig::default();
        assert!(valid_config.validate().is_ok());

        let invalid_lease = MultiListenerConfig {
            lease_duration_ms: 0,
            ..Default::default()
        };
        assert!(invalid_lease.validate().is_err());

        let invalid_health_check = MultiListenerConfig {
            health_check_interval_ms: 0,
            ..Default::default()
        };
        assert!(invalid_health_check.validate().is_err());

        let invalid_threshold = MultiListenerConfig {
            failover_threshold_ms: 1000,
            health_check_interval_ms: 5000,
            ..Default::default()
        };
        assert!(invalid_threshold.validate().is_err());

        let invalid_max_listeners = MultiListenerConfig {
            max_listeners: 0,
            ..Default::default()
        };
        assert!(invalid_max_listeners.validate().is_err());
    }

    #[test]
    fn test_multi_listener_config_builder() {
        let config = MultiListenerConfig::new()
            .enable()
            .with_listener_id("test-listener".to_string())
            .with_lease_duration_ms(20000);

        assert!(config.enabled);
        assert_eq!(config.listener_id, "test-listener");
        assert_eq!(config.lease_duration_ms, 20000);
    }

    #[test]
    fn test_redis_config_defaults() {
        let config = RedisConfig::default();
        assert!(config.url.contains("localhost:6379"));
        assert_eq!(config.pool_size, 10);
        assert_eq!(config.connect_timeout_secs, 5);
        assert_eq!(config.command_timeout_secs, 2);
        assert_eq!(config.dedup_window_secs, 300);
        assert_eq!(config.cache_ttl_secs, 60);
    }

    #[test]
    fn test_redis_config_validation() {
        // Valid config
        let config = RedisConfig::default();
        assert!(config.validate().is_ok());

        // Invalid: empty URL
        let config = RedisConfig {
            url: String::new(),
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // Invalid: pool_size = 0
        let config = RedisConfig {
            pool_size: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // Invalid: dedup_window too large
        let config = RedisConfig {
            dedup_window_secs: 3601,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_performance_config_defaults() {
        let config = PerformanceConfig::default();
        assert!(!config.enable_dedup);
        assert!(!config.enable_caching);
        assert!(config.enable_concurrent);
        assert_eq!(config.max_concurrent_actions, 10);
        assert_eq!(config.concurrent_timeout_ms, 30000);
    }

    #[test]
    fn test_performance_config_validation() {
        // Valid config (no Redis features enabled)
        let config = PerformanceConfig::default();
        assert!(config.validate(false).is_ok());

        // Invalid: enable_dedup without Redis
        let config = PerformanceConfig {
            enable_dedup: true,
            ..Default::default()
        };
        assert!(config.validate(false).is_err());
        assert!(config.validate(true).is_ok()); // OK with Redis

        // Invalid: enable_caching without Redis
        let config = PerformanceConfig {
            enable_caching: true,
            ..Default::default()
        };
        assert!(config.validate(false).is_err());
        assert!(config.validate(true).is_ok()); // OK with Redis

        // Invalid: max_concurrent_actions = 0
        let config = PerformanceConfig {
            max_concurrent_actions: 0,
            ..Default::default()
        };
        assert!(config.validate(false).is_err());
    }
}
