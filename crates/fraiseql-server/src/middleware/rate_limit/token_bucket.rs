//! Token bucket algorithm implementation.

/// Token bucket for rate limiting.
#[derive(Debug, Clone)]
pub(super) struct TokenBucket {
    /// Current token count
    pub(super) tokens: f64,

    /// Maximum tokens
    pub(super) capacity: f64,

    /// Refill rate (tokens per second)
    pub(super) refill_rate: f64,

    /// Last refill timestamp
    pub(super) last_refill: std::time::Instant,
}

impl TokenBucket {
    /// Create new token bucket.
    pub(super) fn new(capacity: f64, refill_rate: f64) -> Self {
        Self {
            tokens: capacity,
            capacity,
            refill_rate,
            last_refill: std::time::Instant::now(),
        }
    }

    /// Try to consume tokens. Returns true if allowed, false if rate limited.
    pub(super) fn try_consume(&mut self, tokens: f64) -> bool {
        // Refill based on elapsed time
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        let refilled = elapsed * self.refill_rate;
        self.tokens = (self.tokens + refilled).min(self.capacity);
        self.last_refill = now;

        // Try to consume
        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }

    /// Get current token count.
    pub(super) fn token_count(&self) -> f64 {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        let refilled = elapsed * self.refill_rate;
        (self.tokens + refilled).min(self.capacity)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn new_bucket_starts_at_capacity() {
        let bucket = TokenBucket::new(100.0, 10.0);
        assert!((bucket.tokens - 100.0).abs() < f64::EPSILON);
        assert!((bucket.capacity - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn consume_more_than_available_fails() {
        let mut bucket = TokenBucket::new(3.0, 1.0);
        assert!(!bucket.try_consume(4.0), "consuming more than capacity must fail");
    }

    #[test]
    fn token_count_never_exceeds_capacity() {
        // Fabricate a bucket whose last_refill is far in the past
        let bucket = TokenBucket {
            tokens:      50.0,
            capacity:    100.0,
            refill_rate: 1000.0,
            last_refill: Instant::now().checked_sub(Duration::from_secs(1000)).unwrap(),
        };
        // Even with 1_000_000 tokens of potential refill, count is capped at 100
        assert!(
            bucket.token_count() <= 100.0,
            "token_count must never exceed capacity"
        );
    }

    #[test]
    fn refill_restores_tokens_after_idle_period() {
        let mut bucket = TokenBucket {
            tokens:      0.0,
            capacity:    10.0,
            refill_rate: 100.0, // 100 tokens/sec
            last_refill: Instant::now().checked_sub(Duration::from_millis(100)).unwrap(),
        };
        // 100ms at 100 tok/s = 10 tokens refilled → full capacity
        assert!(bucket.try_consume(1.0), "refilled bucket must allow consumption");
    }

    #[test]
    fn zero_refill_rate_never_refills() {
        let mut bucket = TokenBucket {
            tokens:      0.0,
            capacity:    10.0,
            refill_rate: 0.0,
            last_refill: Instant::now().checked_sub(Duration::from_secs(60)).unwrap(),
        };
        assert!(!bucket.try_consume(1.0), "zero refill rate means no refill ever");
    }

    #[test]
    fn fractional_consume_works() {
        let mut bucket = TokenBucket::new(1.0, 0.0);
        assert!(bucket.try_consume(0.5));
        assert!(bucket.try_consume(0.5));
        assert!(!bucket.try_consume(0.1));
    }
}
