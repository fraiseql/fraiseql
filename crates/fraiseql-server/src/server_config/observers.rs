//! Observer runtime and admission control configuration.

use serde::{Deserialize, Serialize};

#[cfg(feature = "observers")]
const fn default_observers_enabled() -> bool {
    true
}

#[cfg(feature = "observers")]
const fn default_poll_interval_ms() -> u64 {
    100
}

#[cfg(feature = "observers")]
const fn default_batch_size() -> usize {
    100
}

#[cfg(feature = "observers")]
const fn default_channel_capacity() -> usize {
    1000
}

#[cfg(feature = "observers")]
const fn default_auto_reload() -> bool {
    true
}

#[cfg(feature = "observers")]
const fn default_reload_interval_secs() -> u64 {
    60
}

/// Pool configuration for the observer's dedicated PostgreSQL connection pool.
///
/// The observer pool is separate from the application pool because the
/// LISTEN/NOTIFY connection occupies a persistent slot. Smaller defaults
/// are appropriate since observers need far fewer connections than the app.
///
/// Configure via `[observers.pool]` in `fraiseql.toml`:
///
/// ```toml
/// [observers.pool]
/// min_connections = 2
/// max_connections = 5
/// acquire_timeout_secs = 10
/// ```
#[cfg(feature = "observers")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ObserverPoolConfig {
    /// Minimum number of connections to keep open (default: 2).
    #[serde(default = "default_observer_pool_min")]
    pub min_connections: u32,

    /// Maximum number of connections in the observer pool (default: 5).
    #[serde(default = "default_observer_pool_max")]
    pub max_connections: u32,

    /// Timeout in seconds for acquiring a connection from the pool (default: 10).
    #[serde(default = "default_observer_acquire_timeout")]
    pub acquire_timeout_secs: u64,
}

#[cfg(feature = "observers")]
const fn default_observer_pool_min() -> u32 {
    2
}

#[cfg(feature = "observers")]
const fn default_observer_pool_max() -> u32 {
    5
}

#[cfg(feature = "observers")]
const fn default_observer_acquire_timeout() -> u64 {
    10
}

#[cfg(feature = "observers")]
impl Default for ObserverPoolConfig {
    fn default() -> Self {
        Self {
            min_connections:      default_observer_pool_min(),
            max_connections:      default_observer_pool_max(),
            acquire_timeout_secs: default_observer_acquire_timeout(),
        }
    }
}

/// Observer runtime configuration.
#[cfg(feature = "observers")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObserverConfig {
    /// Enable observer runtime (default: true).
    #[serde(default = "default_observers_enabled")]
    pub enabled: bool,

    /// Poll interval for change log in milliseconds (default: 100).
    #[serde(default = "default_poll_interval_ms")]
    pub poll_interval_ms: u64,

    /// Batch size for fetching change log entries (default: 100).
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// Channel capacity for event buffering (default: 1000).
    #[serde(default = "default_channel_capacity")]
    pub channel_capacity: usize,

    /// Auto-reload observers on changes (default: true).
    #[serde(default = "default_auto_reload")]
    pub auto_reload: bool,

    /// Reload interval in seconds (default: 60).
    #[serde(default = "default_reload_interval_secs")]
    pub reload_interval_secs: u64,

    /// Dedicated connection pool configuration for the observer runtime.
    ///
    /// When absent, sensible observer-specific defaults are used (smaller
    /// than the application pool). Operators can set `[observers.pool]` in
    /// `fraiseql.toml` to tune independently of the main pool.
    #[serde(default)]
    pub pool: ObserverPoolConfig,
}

/// Admission control configuration for backpressure limiting.
///
/// Pairs with `crate::resilience::backpressure::AdmissionController`.
/// See [`super::ServerConfig::admission_control`] for wiring instructions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdmissionConfig {
    /// Maximum number of in-flight concurrent requests (semaphore permits).
    ///
    /// Defaults to 500.
    #[serde(default = "default_admission_max_concurrent")]
    pub max_concurrent: usize,

    /// Maximum number of requests waiting for a permit (queue depth).
    ///
    /// When the queue is full, new requests are rejected with 503.
    /// Defaults to 1000.
    #[serde(default = "default_admission_max_queue_depth")]
    pub max_queue_depth: u64,
}

pub(crate) const fn default_admission_max_concurrent() -> usize {
    500
}

pub(crate) const fn default_admission_max_queue_depth() -> u64 {
    1000
}

