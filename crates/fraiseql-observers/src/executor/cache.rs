//! Cache backend integration for action result caching.
//!
//! Only compiled when the `caching` feature is enabled.

use tracing::{debug, warn};

use super::ObserverExecutor;
use crate::{
    cache::{CachedActionResult, key::action_result_key},
    config::ActionConfig,
    event::EntityEvent,
    traits::ActionResult,
};

impl ObserverExecutor {
    /// Try to get cached action result, return None if cache disabled or miss.
    pub(super) async fn try_cache_get(
        &self,
        event: &EntityEvent,
        action: &ActionConfig,
    ) -> Option<ActionResult> {
        if let Some(ref cache) = self.cache_backend {
            let cache_key = action_result_key(event, action)?;
            if let Ok(Some(cached)) = cache.get(&cache_key).await {
                debug!("Cache hit for {} ({}ms latency)", action.action_type(), cached.duration_ms);
                #[cfg(feature = "metrics")]
                self.metrics.cache_hit();

                return Some(ActionResult {
                    action_type: cached.action_type,
                    success:     cached.success,
                    message:     cached.message,
                    duration_ms: cached.duration_ms,
                    // A cache hit carries no fresh transport round-trip, so there
                    // is no HTTP status to surface (#468).
                    status_code: None,
                });
            }
        }
        None
    }

    /// Store action result in cache (no-op if cache disabled).
    pub(super) async fn cache_store(
        &self,
        event: &EntityEvent,
        action: &ActionConfig,
        result: &ActionResult,
    ) {
        if let Some(ref cache) = self.cache_backend {
            if result.success {
                let Some(cache_key) = action_result_key(event, action) else {
                    warn!("Action does not render to JSON; result not cached");
                    return;
                };
                let cached_result = CachedActionResult::new(
                    result.action_type.clone(),
                    result.success,
                    result.message.clone(),
                    result.duration_ms,
                );

                if let Err(e) = cache.set(&cache_key, &cached_result).await {
                    warn!("Failed to cache action result: {}", e);
                }
            }
        }
    }
}
