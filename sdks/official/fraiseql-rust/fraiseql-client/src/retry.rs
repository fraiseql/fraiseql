//! Retry configuration and exponential back-off logic.

use std::time::Duration;

/// Configuration for automatic request retries.
///
/// Default: `max_attempts = 1` (no retry).
///
/// # Example
///
/// ```rust,no_run
/// use fraiseql_client::RetryConfig;
/// use std::time::Duration;
///
/// // Requires: a running FraiseQL server
/// let retry = RetryConfig {
///     max_attempts: 3,
///     base_delay: Duration::from_millis(500),
///     ..RetryConfig::default()
/// };
/// ```
#[derive(Debug, Clone)]
#[must_use = "call FraiseQLClientBuilder::retry() to apply this config"]
pub struct RetryConfig {
    /// Total number of attempts (1 = no retry).
    pub max_attempts: u32,
    /// Initial delay before the first retry.
    pub base_delay: Duration,
    /// Maximum delay between retries.
    pub max_delay: Duration,
    /// Whether to add random jitter to the delay.
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 1,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            jitter: true,
        }
    }
}

impl RetryConfig {
    /// Calculate the delay for a given retry attempt (0-indexed).
    pub(crate) fn delay_for(&self, attempt: u32) -> Duration {
        let exp = u32::min(attempt, 31); // prevent overflow
        let raw = self.base_delay.saturating_mul(2_u32.saturating_pow(exp));
        let capped = raw.min(self.max_delay);
        if self.jitter {
            // Add up to 10% jitter using a simple deterministic hash for no-rand dep
            let nanos = capped.subsec_nanos();
            let jitter_nanos = nanos / 10;
            capped + Duration::from_nanos(u64::from(jitter_nanos))
        } else {
            capped
        }
    }
}
