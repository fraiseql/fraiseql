//! Rate limiter for subscriptions
//!
//! Token bucket algorithm for per-user and per-subscription rate limiting.

use crate::subscriptions::config::RateLimiterConfig;
use crate::subscriptions::SubscriptionError;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Instant;

/// Token bucket for rate limiting
#[derive(Debug, Clone)]
struct TokenBucket {
    /// Current tokens available
    tokens: f64,

    /// Token capacity
    capacity: f64,

    /// Tokens per second refill rate
    refill_rate: f64,

    /// Last refill time
    last_refill: Instant,
}

impl TokenBucket {
    /// Create new token bucket
    fn new(capacity: f64, refill_rate: f64) -> Self {
        Self {
            tokens: capacity,
            capacity,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        let tokens_to_add = elapsed * self.refill_rate;

        self.tokens = (self.tokens + tokens_to_add).min(self.capacity);
        self.last_refill = now;
    }

    /// Try to consume tokens
    fn try_consume(&mut self, tokens: f64) -> bool {
        self.refill();

        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }

    /// Get current token count
    fn current_tokens(&mut self) -> f64 {
        self.refill();
        self.tokens
    }
}

/// Subscription rate limiter
pub struct SubscriptionRateLimiter {
    /// Per-user token buckets
    user_buckets: Arc<DashMap<i64, TokenBucket>>,

    /// Per-subscription token buckets
    subscription_buckets: Arc<DashMap<String, TokenBucket>>,

    /// Configuration
    config: RateLimiterConfig,
}

impl SubscriptionRateLimiter {
    /// Create new rate limiter
    pub fn new(config: RateLimiterConfig) -> Self {
        Self {
            user_buckets: Arc::new(DashMap::new()),
            subscription_buckets: Arc::new(DashMap::new()),
            config,
        }
    }

    /// Check if user can create new subscription
    pub fn check_subscription_creation(
        &self,
        user_id: i64,
    ) -> Result<(), SubscriptionError> {
        // Get or create user bucket
        let mut bucket = self
            .user_buckets
            .entry(user_id)
            .or_insert_with(|| {
                TokenBucket::new(
                    self.config.token_capacity as f64,
                    self.config.token_refill_rate,
                )
            })
            .clone();

        // Each subscription creation costs tokens
        if bucket.try_consume(1.0) {
            // Update the bucket
            self.user_buckets.insert(user_id, bucket);
            Ok(())
        } else {
            Err(SubscriptionError::RateLimitExceeded)
        }
    }

    /// Check if subscription can send event
    pub fn check_event_emission(
        &self,
        subscription_id: &str,
    ) -> Result<(), SubscriptionError> {
        // Get or create subscription bucket (1 token per second)
        let mut bucket = self
            .subscription_buckets
            .entry(subscription_id.to_string())
            .or_insert_with(|| {
                TokenBucket::new(
                    self.config.max_events_per_subscription as f64,
                    self.config.max_events_per_subscription as f64 / 60.0, // Refill once per minute
                )
            })
            .clone();

        // Each event costs 1 token
        if bucket.try_consume(1.0) {
            // Update the bucket
            self.subscription_buckets
                .insert(subscription_id.to_string(), bucket);
            Ok(())
        } else {
            Err(SubscriptionError::RateLimitExceeded)
        }
    }

    /// Check if user has too many connections
    pub fn check_connections_per_user(
        &self,
        user_id: i64,
        current_count: usize,
    ) -> Result<(), SubscriptionError> {
        if current_count >= self.config.max_connections_per_user {
            Err(SubscriptionError::SubscriptionRejected(
                "Too many connections for user".to_string(),
            ))
        } else {
            Ok(())
        }
    }

    /// Reset rate limit for user
    pub fn reset_user_limit(&self, user_id: i64) {
        self.user_buckets.remove(&user_id);
    }

    /// Reset rate limit for subscription
    pub fn reset_subscription_limit(&self, subscription_id: &str) {
        self.subscription_buckets.remove(subscription_id);
    }

    /// Get user bucket info for testing/monitoring
    pub fn get_user_info(&self, user_id: i64) -> Option<(f64, f64)> {
        self.user_buckets.get(&user_id).map(|bucket| {
            let mut b = bucket.clone();
            (b.current_tokens(), b.capacity)
        })
    }

    /// Get subscription bucket info for testing/monitoring
    pub fn get_subscription_info(&self, subscription_id: &str) -> Option<(f64, f64)> {
        self.subscription_buckets.get(subscription_id).map(|bucket| {
            let mut b = bucket.clone();
            (b.current_tokens(), b.capacity)
        })
    }

    /// Clear all limits
    pub fn clear_all(&self) {
        self.user_buckets.clear();
        self.subscription_buckets.clear();
    }
}

impl Clone for SubscriptionRateLimiter {
    fn clone(&self) -> Self {
        Self {
            user_buckets: self.user_buckets.clone(),
            subscription_buckets: self.subscription_buckets.clone(),
            config: self.config.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_token_bucket_creation() {
        let bucket = TokenBucket::new(100.0, 10.0);
        assert_eq!(bucket.tokens, 100.0);
        assert_eq!(bucket.capacity, 100.0);
        assert_eq!(bucket.refill_rate, 10.0);
    }

    #[test]
    fn test_token_bucket_consume() {
        let mut bucket = TokenBucket::new(100.0, 10.0);
        assert!(bucket.try_consume(50.0));
        assert_eq!(bucket.tokens, 50.0);

        assert!(bucket.try_consume(50.0));
        assert!(!bucket.try_consume(1.0)); // No tokens left
    }

    #[test]
    fn test_token_bucket_refill() {
        let mut bucket = TokenBucket::new(10.0, 10.0); // 10 tokens/sec
        bucket.try_consume(10.0);
        assert_eq!(bucket.tokens, 0.0);

        // Wait 0.1 seconds = 1 token refilled
        std::thread::sleep(Duration::from_millis(100));
        assert!(bucket.try_consume(1.0));
    }

    #[test]
    fn test_rate_limiter_subscription_creation() {
        let config = RateLimiterConfig::default();
        let limiter = SubscriptionRateLimiter::new(config);

        let user_id = 123i64;
        assert!(limiter.check_subscription_creation(user_id).is_ok());
        assert!(limiter.check_subscription_creation(user_id).is_ok());
    }

    #[test]
    fn test_rate_limiter_check_connections() {
        let config = RateLimiterConfig::default();
        let limiter = SubscriptionRateLimiter::new(config);

        let user_id = 123i64;
        assert!(limiter.check_connections_per_user(user_id, 5).is_ok());
        assert!(limiter.check_connections_per_user(user_id, 10).is_ok());
        assert!(limiter.check_connections_per_user(user_id, 11).is_err());
    }

    #[test]
    fn test_rate_limiter_reset_user() {
        let config = RateLimiterConfig::default();
        let limiter = SubscriptionRateLimiter::new(config);

        let user_id = 123i64;
        limiter.check_subscription_creation(user_id).unwrap();

        let (tokens_before, _) = limiter.get_user_info(user_id).unwrap();
        assert!(tokens_before < 1000.0);

        limiter.reset_user_limit(user_id);

        let (tokens_after, capacity) = limiter.get_user_info(user_id).unwrap_or((0.0, 0.0));
        assert_eq!(tokens_after, capacity);
    }

    #[test]
    fn test_rate_limiter_event_emission() {
        let config = RateLimiterConfig::default();
        let limiter = SubscriptionRateLimiter::new(config);

        let sub_id = "sub-123";
        assert!(limiter.check_event_emission(sub_id).is_ok());
        assert!(limiter.check_event_emission(sub_id).is_ok());
    }

    #[test]
    fn test_rate_limiter_clear_all() {
        let config = RateLimiterConfig::default();
        let limiter = SubscriptionRateLimiter::new(config);

        let user_id = 123i64;
        limiter.check_subscription_creation(user_id).unwrap();

        assert!(limiter.get_user_info(user_id).is_some());

        limiter.clear_all();

        assert!(limiter.get_user_info(user_id).is_none());
    }
}
