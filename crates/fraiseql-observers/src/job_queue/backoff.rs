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

/// Calculate exponential backoff: `initial_delay * 2^(attempt-1)`, capped at max_delay
///
/// Formula: `delay = min(initial_delay * 2^(attempt-1), max_delay)`
///
/// # Examples
///
/// With initial_delay=100ms, max_delay=30000ms:
/// - Attempt 1: 100ms
/// - Attempt 2: 200ms
/// - Attempt 3: 400ms
/// - Attempt 4: 800ms
/// - Attempt 5: 1600ms
#[must_use]
fn calculate_exponential(attempt: u32, initial_delay_ms: u64, max_delay_ms: u64) -> u64 {
    let exponent = (attempt - 1).min(63); // Prevent overflow
    let delay_ms = initial_delay_ms.saturating_mul(2_u64.saturating_pow(exponent));
    delay_ms.min(max_delay_ms)
}

/// Calculate linear backoff: `initial_delay * attempt`, capped at max_delay
///
/// Formula: `delay = min(initial_delay * attempt, max_delay)`
///
/// # Examples
///
/// With initial_delay=100ms, max_delay=30000ms:
/// - Attempt 1: 100ms
/// - Attempt 2: 200ms
/// - Attempt 3: 300ms
/// - Attempt 4: 400ms
/// - Attempt 5: 500ms
#[must_use]
fn calculate_linear(attempt: u32, initial_delay_ms: u64, max_delay_ms: u64) -> u64 {
    let delay_ms = initial_delay_ms.saturating_mul(u64::from(attempt));
    delay_ms.min(max_delay_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exponential_backoff() {
        let initial = 100;
        let max = 30000;

        assert_eq!(calculate_exponential(1, initial, max), 100);
        assert_eq!(calculate_exponential(2, initial, max), 200);
        assert_eq!(calculate_exponential(3, initial, max), 400);
        assert_eq!(calculate_exponential(4, initial, max), 800);
        assert_eq!(calculate_exponential(5, initial, max), 1600);
    }

    #[test]
    fn test_exponential_backoff_caps_at_max() {
        let initial = 100;
        let max = 1000;

        assert_eq!(calculate_exponential(1, initial, max), 100);
        assert_eq!(calculate_exponential(2, initial, max), 200);
        assert_eq!(calculate_exponential(3, initial, max), 400);
        assert_eq!(calculate_exponential(4, initial, max), 800);
        assert_eq!(calculate_exponential(5, initial, max), 1000); // Capped at max
        assert_eq!(calculate_exponential(6, initial, max), 1000); // Still capped
    }

    #[test]
    fn test_linear_backoff() {
        let initial = 100;
        let max = 30000;

        assert_eq!(calculate_linear(1, initial, max), 100);
        assert_eq!(calculate_linear(2, initial, max), 200);
        assert_eq!(calculate_linear(3, initial, max), 300);
        assert_eq!(calculate_linear(4, initial, max), 400);
        assert_eq!(calculate_linear(5, initial, max), 500);
    }

    #[test]
    fn test_linear_backoff_caps_at_max() {
        let initial = 100;
        let max = 350;

        assert_eq!(calculate_linear(1, initial, max), 100);
        assert_eq!(calculate_linear(2, initial, max), 200);
        assert_eq!(calculate_linear(3, initial, max), 300);
        assert_eq!(calculate_linear(4, initial, max), 350); // Capped at max
        assert_eq!(calculate_linear(5, initial, max), 350); // Still capped
    }

    #[test]
    fn test_calculate_backoff_exponential() {
        let duration = calculate_backoff(
            crate::config::BackoffStrategy::Exponential,
            2,
            100,
            30000,
        );
        assert_eq!(duration.as_millis(), 200);
    }

    #[test]
    fn test_calculate_backoff_linear() {
        let duration = calculate_backoff(
            crate::config::BackoffStrategy::Linear,
            3,
            100,
            30000,
        );
        assert_eq!(duration.as_millis(), 300);
    }

    #[test]
    fn test_calculate_backoff_fixed() {
        let duration = calculate_backoff(
            crate::config::BackoffStrategy::Fixed,
            5, // Attempt number is ignored for fixed
            100,
            30000,
        );
        assert_eq!(duration.as_millis(), 100);
    }

    #[test]
    fn test_backoff_overflow_protection() {
        // Test that exponential backoff doesn't overflow
        let delay = calculate_exponential(100, 100, u64::MAX);
        assert!(delay <= u64::MAX);
    }

    #[test]
    fn test_zero_initial_delay() {
        assert_eq!(calculate_exponential(1, 0, 1000), 0);
        assert_eq!(calculate_linear(1, 0, 1000), 0);
    }

    #[test]
    fn test_max_delay_equals_initial() {
        let initial = 100;
        assert_eq!(calculate_exponential(5, initial, initial), initial);
        assert_eq!(calculate_linear(5, initial, initial), initial);
    }
}
