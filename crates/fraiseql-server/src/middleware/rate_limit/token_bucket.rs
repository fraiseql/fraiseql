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
