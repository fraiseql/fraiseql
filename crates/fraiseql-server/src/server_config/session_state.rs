//! `[session_state]` — durable per-thread conversation state (#389).

use serde::{Deserialize, Serialize};

/// Configuration for the session-state subsystem (`[session_state]`).
///
/// Presence of the section enables the subsystem; absence leaves it off
/// entirely. Strict (`deny_unknown_fields`): an unrecognised key is a boot
/// error, not a silently inert setting.
///
/// # Example (TOML)
///
/// ```toml
/// [session_state]
/// backend = "postgres"        # "memory" (volatile, dev) | "postgres" (durable)
/// default_ttl_secs = 3600     # per-entry TTL
/// evict_interval_secs = 300   # background sweep cadence
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SessionStateServerConfig {
    /// Storage backend: `"memory"` (volatile — lost on restart; development
    /// only) or `"postgres"` (durable, `_system.session_state`; requires a
    /// database pool). A configured `postgres` backend that cannot initialise
    /// its table at boot **refuses to boot** — it never downgrades to memory.
    pub backend: String,

    /// Per-entry time-to-live, in seconds. Expired entries are invisible to
    /// reads immediately and removed by the background sweep.
    pub default_ttl_secs: u64,

    /// Cadence of the background eviction sweep, in seconds.
    pub evict_interval_secs: u64,
}

impl Default for SessionStateServerConfig {
    fn default() -> Self {
        Self {
            backend:             "memory".to_string(),
            default_ttl_secs:    3600,
            evict_interval_secs: 300,
        }
    }
}

impl SessionStateServerConfig {
    /// Validate field values (called from
    /// [`ServerConfig::validate`](super::ServerConfig::validate)).
    ///
    /// # Errors
    ///
    /// Returns a message naming the offending key when `backend` is not one of
    /// the supported tokens or an interval is zero.
    pub fn validate(&self) -> Result<(), String> {
        match self.backend.as_str() {
            "memory" | "postgres" => {},
            other => {
                return Err(format!(
                    "[session_state] backend = \"{other}\" is not supported — use \"memory\" \
                     (volatile, development) or \"postgres\" (durable)."
                ));
            },
        }
        if self.default_ttl_secs == 0 {
            return Err("[session_state] default_ttl_secs must be at least 1 — a zero TTL would \
                        make every write instantly invisible."
                .to_string());
        }
        if self.evict_interval_secs == 0 {
            return Err("[session_state] evict_interval_secs must be at least 1.".to_string());
        }
        Ok(())
    }
}
