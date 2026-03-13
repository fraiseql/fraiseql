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
}

/// Admission control configuration for backpressure limiting.
///
/// Pairs with [`crate::resilience::backpressure::AdmissionController`].
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
