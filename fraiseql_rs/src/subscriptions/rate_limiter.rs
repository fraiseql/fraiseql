//! Rate limiter for subscriptions
//!
//! Token bucket algorithm for per-user and per-subscription rate limiting.

use crate::subscriptions::config::RateLimiterConfig;
use crate::subscriptions::SubscriptionError;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

/// Token bucket for rate limiting
///
/// Uses Arc<Mutex<>> to ensure state persists across multiple checks.
/// Without this, the bucket would be cloned on each access, defeating the rate limiting.
#[derive(Debug)]
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
    #[allow(dead_code)] // Used for monitoring/diagnostics
    fn current_tokens(&mut self) -> f64 {
        self.refill();
        self.tokens
    }
}

/// Subscription rate limiter
#[derive(Debug)]
pub struct SubscriptionRateLimiter {
    /// Per-user token buckets (Arc<Mutex<>> ensures state persists across checks)
    user_buckets: Arc<DashMap<i64, Arc<Mutex<TokenBucket>>>>,

    /// Per-subscription token buckets (Arc<Mutex<>> ensures state persists across checks)
    subscription_buckets: Arc<DashMap<String, Arc<Mutex<TokenBucket>>>>,

    /// Configuration
    config: RateLimiterConfig,
}

impl SubscriptionRateLimiter {
    /// Create new rate limiter
    #[must_use]
    pub fn new(config: RateLimiterConfig) -> Self {
        Self {
            user_buckets: Arc::new(DashMap::new()),
            subscription_buckets: Arc::new(DashMap::new()),
            config,
        }
    }

    /// Check if user can create new subscription
    ///
    /// # Errors
    ///
    /// Returns an error if the rate limit is exceeded.
    pub async fn check_subscription_creation(&self, user_id: i64) -> Result<(), SubscriptionError> {
        // Get or create user bucket
        let bucket_arc = self
            .user_buckets
            .entry(user_id)
            .or_insert_with(|| {
                Arc::new(Mutex::new(TokenBucket::new(
                    f64::from(self.config.token_capacity),
                    self.config.token_refill_rate,
                )))
            })
            .clone();

        // Lock the bucket and try to consume tokens
        let mut bucket = bucket_arc.lock().await;
        if bucket.try_consume(1.0) {
            Ok(())
        } else {
            Err(SubscriptionError::RateLimitExceeded)
        }
    }

    /// Check if subscription can send event
    ///
    /// # Errors
    ///
    /// Returns an error if the rate limit is exceeded.
    pub async fn check_event_emission(
        &self,
        subscription_id: &str,
    ) -> Result<(), SubscriptionError> {
        // Get or create subscription bucket (1 token per second)
        let bucket_arc = self
            .subscription_buckets
            .entry(subscription_id.to_string())
            .or_insert_with(|| {
                Arc::new(Mutex::new(TokenBucket::new(
                    self.config.max_events_per_subscription as f64,
                    self.config.max_events_per_subscription as f64 / 60.0, // Refill once per minute
                )))
            })
            .clone();

        // Lock the bucket and try to consume tokens
        let mut bucket = bucket_arc.lock().await;
        if bucket.try_consume(1.0) {
            Ok(())
        } else {
            Err(SubscriptionError::RateLimitExceeded)
        }
    }

    /// Check if user has too many connections
    ///
    /// # Errors
    ///
    /// Returns an error if the maximum number of connections per user is exceeded.
    pub fn check_connections_per_user(
        &self,
        _user_id: i64,
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
    #[must_use]
    pub fn get_user_info(&self, user_id: i64) -> Option<(f64, f64)> {
        self.user_buckets.get(&user_id).map(|bucket_arc| {
            // Note: This is a blocking operation in async context.
            // In production, use try_lock() to avoid blocking.
            let bucket = bucket_arc.blocking_lock();
            (bucket.tokens, bucket.capacity)
        })
    }

    /// Get subscription bucket info for testing/monitoring
    #[must_use]
    pub fn get_subscription_info(&self, subscription_id: &str) -> Option<(f64, f64)> {
        self.subscription_buckets
            .get(subscription_id)
            .map(|bucket_arc| {
                // Note: This is a blocking operation in async context.
                // In production, use try_lock() to avoid blocking.
                let bucket = bucket_arc.blocking_lock();
                (bucket.tokens, bucket.capacity)
            })
    }

    /// Clear all limits
    pub fn clear_all(&self) {
        self.user_buckets.clear();
        self.subscription_buckets.clear();
    }
}

/// `SubscriptionRateLimiter` can be cloned since Arc<`DashMap`<>> handles sharing
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
        assert!((bucket.tokens - 100.0).abs() < 0.001);
        assert!((bucket.capacity - 100.0).abs() < 0.001);
        assert!((bucket.refill_rate - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_token_bucket_consume() {
        let mut bucket = TokenBucket::new(100.0, 10.0);
        assert!(bucket.try_consume(50.0));
        assert!((bucket.tokens - 50.0).abs() < 0.001);

        assert!(bucket.try_consume(50.0));
        assert!(!bucket.try_consume(1.0)); // No tokens left
    }

    #[test]
    fn test_token_bucket_refill() {
        let mut bucket = TokenBucket::new(10.0, 10.0); // 10 tokens/sec
        bucket.try_consume(10.0);
        assert!(bucket.tokens.abs() < 0.001);

        // Wait 0.1 seconds = 1 token refilled
        std::thread::sleep(Duration::from_millis(100));
        assert!(bucket.try_consume(1.0));
    }

    #[tokio::test]
    async fn test_rate_limiter_subscription_creation() {
        let config = RateLimiterConfig::default();
        let limiter = SubscriptionRateLimiter::new(config);

        let user_id = 123i64;
        assert!(limiter.check_subscription_creation(user_id).await.is_ok());
        assert!(limiter.check_subscription_creation(user_id).await.is_ok());
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

    #[tokio::test]
    async fn test_rate_limiter_reset_user() {
        let config = RateLimiterConfig::default();
        let limiter = SubscriptionRateLimiter::new(config);

        let user_id = 123i64;
        limiter.check_subscription_creation(user_id).await.unwrap();

        let (tokens_before, _) = limiter.get_user_info(user_id).unwrap();
        assert!(tokens_before < 1000.0);

        limiter.reset_user_limit(user_id);

        let (tokens_after, capacity) = limiter.get_user_info(user_id).unwrap_or((0.0, 0.0));
        assert!((tokens_after - capacity).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_rate_limiter_event_emission() {
        let config = RateLimiterConfig::default();
        let limiter = SubscriptionRateLimiter::new(config);

        let sub_id = "sub-123";
        assert!(limiter.check_event_emission(sub_id).await.is_ok());
        assert!(limiter.check_event_emission(sub_id).await.is_ok());
    }

    #[tokio::test]
    async fn test_rate_limiter_clear_all() {
        let config = RateLimiterConfig::default();
        let limiter = SubscriptionRateLimiter::new(config);

        let user_id = 123i64;
        limiter.check_subscription_creation(user_id).await.unwrap();

        assert!(limiter.get_user_info(user_id).is_some());

        limiter.clear_all();

        assert!(limiter.get_user_info(user_id).is_none());
    }
}
