//! Auxiliary configuration types consumed by [`crate::ServerConfig`] sections.
//!
//! The server's configuration file is deserialized directly into
//! [`crate::ServerConfig`] (see `server_config/`); this module holds the
//! supporting types some of its sections reference, plus the error-sanitization
//! and pool-tuning subsystem configs.
//!
//! The former `RuntimeConfig` layer that used to live here — a parallel
//! `[server]`/`[database]`-shaped config tree with its own loader and
//! `ConfigValidator` — was constructed by nothing but its own tests, while the
//! architecture docs described it as the binary's config path (#839). It was
//! deleted rather than wired in.

use serde::Deserialize;

pub mod error_sanitization;
pub mod pool_tuning;
#[cfg(test)]
mod tests;

// Re-export config types
pub use error_sanitization::{ErrorSanitizationConfig, ErrorSanitizer};
#[allow(deprecated)] // Reason: re-export deprecated alias for backwards compatibility
pub use pool_tuning::{PoolPressureMonitorConfig, PoolTuningConfig};

/// Configuration for durable usage counter persistence.
///
/// Add a `[usage]` section to the server config TOML to enable:
///
/// ```toml
/// [usage]
/// flush_interval_secs = 60
/// ```
///
/// When absent (default), the [`NoopBackend`] is used and counters are
/// in-memory only (reset on process restart).
///
/// [`NoopBackend`]: crate::usage::aggregator::NoopBackend
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct UsagePersistenceConfig {
    /// How often (in seconds) to flush in-memory counters to PostgreSQL.
    ///
    /// Defaults to `60` seconds.
    #[serde(default = "default_flush_interval_secs")]
    pub flush_interval_secs: u64,
}

const fn default_flush_interval_secs() -> u64 {
    60
}

/// Configuration for a single incoming webhook route.
#[derive(Debug, Clone, serde::Serialize, Deserialize)]
pub struct WebhookRouteConfig {
    /// Name of the environment variable that holds the webhook signing secret.
    pub secret_env: String,
    /// Webhook provider identifier (e.g. `"github"`, `"stripe"`).
    pub provider:   String,
    /// URL path override; if absent, the route name is used as the path segment.
    #[serde(default)]
    pub path:       Option<String>,
    /// The exact public URL this route is registered under at the provider.
    ///
    /// Required for providers whose signing scheme covers the request URL
    /// (Twilio signs scheme + host + path + query). It must be the URL as the
    /// provider knows it — reconstructing it from `Host`/`X-Forwarded-*` headers
    /// would put the signed material under the sender's control, so the server
    /// refuses to boot instead when a URL-signing provider lacks this (#781).
    #[serde(default)]
    pub public_url: Option<String>,
}
