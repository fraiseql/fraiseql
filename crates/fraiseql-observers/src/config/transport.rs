//! Transport configuration: Postgres LISTEN/NOTIFY, NATS JetStream, in-memory.

use std::env;

use serde::{Deserialize, Serialize};

use crate::error::{ObserverError, Result};

// ============================================================================
// Transport Kind
// ============================================================================

/// Transport type for event sourcing
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
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

// ============================================================================
// Transport Config
// ============================================================================

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
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::InvalidConfig`] if NATS transport is selected without
    /// a URL, if `run_bridge` is set without NATS transport, or if nested NATS or
    /// bridge config validation fails.
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
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::InvalidConfig`] if `subject_prefix` or `consumer_name`
    /// is empty, or if nested JetStream config validation fails.
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

// ============================================================================
// JetStream Configuration
// ============================================================================

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
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::InvalidConfig`] if `dedup_window_minutes` is outside
    /// `1..=60`, `ack_wait_secs` is 0, or `max_deliver` is not positive.
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
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::InvalidConfig`] if `transport_name` is empty,
    /// `batch_size` is 0 or exceeds 10,000, or `poll_interval_secs` is 0.
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
    #[cfg(all(feature = "postgres", feature = "nats"))]
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
