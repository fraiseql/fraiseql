//! Backoff strategies for job retry logic.
//!
//! Implements exponential, linear, and fixed backoff calculations
//! with configurable initial delays and maximum delays.

use std::time::Duration;

/// Calculate backoff delay for a given attempt number
///
/// # Arguments
///
/// * `strategy` - The backoff strategy to use
/// * `attempt` - Current attempt number (1-based)
/// * `initial_delay_ms` - Initial delay in milliseconds
/// * `max_delay_ms` - Maximum delay cap in milliseconds
///
/// # Returns
///
/// Duration to wait before retrying
#[must_use]
pub fn calculate_backoff(
    strategy: crate::config::BackoffStrategy,
    attempt: u32,
    initial_delay_ms: u64,
    max_delay_ms: u64,
) -> Duration {
    let delay_ms = match strategy {
        crate::config::BackoffStrategy::Exponential => {
            calculate_exponential(attempt, initial_delay_ms, max_delay_ms)
        },
        crate::config::BackoffStrategy::Linear => {
            calculate_linear(attempt, initial_delay_ms, max_delay_ms)
        },
        crate::config::BackoffStrategy::Fixed => initial_delay_ms,
    };

    Duration::from_millis(delay_ms)
}

/// Calculate exponential backoff: `initial_delay * 2^(attempt-1)`, capped at `max_delay`
///
/// Formula: `delay = min(initial_delay * 2^(attempt-1), max_delay)`
///
/// # Examples
///
/// With `initial_delay`=100ms, `max_delay`=30000ms:
/// - Attempt 1: 100ms
/// - Attempt 2: 200ms
/// - Attempt 3: 400ms
/// - Attempt 4: 800ms
/// - Attempt 5: 1600ms
#[must_use]
pub(super) fn calculate_exponential(attempt: u32, initial_delay_ms: u64, max_delay_ms: u64) -> u64 {
    let exponent = (attempt - 1).min(63); // Prevent overflow
    let delay_ms = initial_delay_ms.saturating_mul(2_u64.saturating_pow(exponent));
    delay_ms.min(max_delay_ms)
}

/// Calculate linear backoff: `initial_delay * attempt`, capped at `max_delay`
///
/// Formula: `delay = min(initial_delay * attempt, max_delay)`
///
/// # Examples
///
/// With `initial_delay`=100ms, `max_delay`=30000ms:
/// - Attempt 1: 100ms
/// - Attempt 2: 200ms
/// - Attempt 3: 300ms
/// - Attempt 4: 400ms
/// - Attempt 5: 500ms
#[must_use]
pub(super) fn calculate_linear(attempt: u32, initial_delay_ms: u64, max_delay_ms: u64) -> u64 {
    let delay_ms = initial_delay_ms.saturating_mul(u64::from(attempt));
    delay_ms.min(max_delay_ms)
}
