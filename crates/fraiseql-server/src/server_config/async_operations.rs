//! `[async_operations]` — durable submit / status / cancel operations (#391).

use serde::{Deserialize, Serialize};

/// Configuration for the async-operations subsystem (`[async_operations]`).
///
/// Presence of the section enables the subsystem (a new HTTP surface — mounting
/// is an explicit operator decision, never a side effect); absence leaves it
/// off. Strict (`deny_unknown_fields`): an unrecognised key is a boot error.
///
/// # Example (TOML)
///
/// ```toml
/// [async_operations]
/// operations = ["largeExport", "rebuildIndex"]  # required, non-empty allowlist
/// workers = 2
/// poll_interval_ms = 500
/// stuck_threshold_secs = 300
/// max_attempts = 1
/// result_ttl_secs = 86400
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AsyncOperationsConfig {
    /// Root operations submittable asynchronously — an explicit, **required**
    /// allowlist (empty refuses to boot). Fail-closed: an operation added to
    /// the schema later is not silently submittable.
    pub operations: Vec<String>,

    /// Background worker tasks executing queued operations (min 1).
    pub workers: u32,

    /// Queue poll cadence per worker, in milliseconds (min 50).
    pub poll_interval_ms: u64,

    /// A `running` operation whose heartbeat is older than this is considered
    /// abandoned (its worker died) and becomes claimable again. Must exceed the
    /// longest heartbeat gap, i.e. effectively the longest single execution
    /// stall (the P19 staleness rule: "stuck" means STALE, never merely
    /// "currently claimed").
    pub stuck_threshold_secs: u64,

    /// Total execution attempts before an operation is marked `failed` (min 1).
    /// Default 1 — **no automatic retry**: re-executing a non-idempotent
    /// mutation is a correctness decision the operator must opt into.
    pub max_attempts: u32,

    /// How long finished (succeeded / failed / cancelled) operations stay
    /// queryable before the sweep removes them, in seconds.
    pub result_ttl_secs: u64,
}

impl Default for AsyncOperationsConfig {
    fn default() -> Self {
        Self {
            operations:           Vec::new(),
            workers:              2,
            poll_interval_ms:     500,
            stuck_threshold_secs: 300,
            max_attempts:         1,
            result_ttl_secs:      86_400,
        }
    }
}

impl AsyncOperationsConfig {
    /// Validate field values (called from
    /// [`ServerConfig::validate`](super::ServerConfig::validate)).
    ///
    /// # Errors
    ///
    /// Returns a message naming the offending key.
    pub fn validate(&self) -> Result<(), String> {
        if self.operations.is_empty() {
            return Err("[async_operations] operations must list at least one root operation — \
                        the allowlist is explicit and fail-closed, so an empty list would make \
                        the whole surface unreachable while looking configured."
                .to_string());
        }
        if self.workers == 0 {
            return Err("[async_operations] workers must be at least 1.".to_string());
        }
        if self.poll_interval_ms < 50 {
            return Err("[async_operations] poll_interval_ms must be at least 50.".to_string());
        }
        if self.stuck_threshold_secs == 0 {
            return Err("[async_operations] stuck_threshold_secs must be at least 1.".to_string());
        }
        if self.max_attempts == 0 {
            return Err("[async_operations] max_attempts must be at least 1.".to_string());
        }
        Ok(())
    }
}
