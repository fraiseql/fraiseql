//! Rate limiting middleware with backpressure support.

use async_trait::async_trait;
use std::time::{Duration, SystemTime};

pub mod memory;

#[cfg(feature = "redis-rate-limiting")]
pub mod redis;

use crate::config::rate_limiting::BackpressureConfig;

/// Parsed rate limit
#[derive(Debug, Clone)]
pub struct RateLimit {
    pub requests: u32,
    pub window: Duration,
    pub burst: Option<u32>,
}

impl RateLimit {
    /// Parse "100/minute" format
    pub fn parse(s: &str) -> Result<Self, ParseError> {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() != 2 {
            return Err(ParseError::InvalidFormat {
                value: s.to_string(),
            });
        }

        let requests: u32 = parts[0].parse().map_err(|_| ParseError::InvalidNumber {
            value: parts[0].to_string(),
        })?;

        let window = match parts[1].to_lowercase().as_str() {
            "second" | "sec" | "s" => Duration::from_secs(1),
            "minute" | "min" | "m" => Duration::from_secs(60),
            "hour" | "hr" | "h" => Duration::from_secs(3600),
            "day" | "d" => Duration::from_secs(86400),
            _ => {
                return Err(ParseError::InvalidPeriod {
                    value: parts[1].to_string(),
                })
            }
        };

        Ok(Self {
            requests,
            window,
            burst: None,
        })
    }

    #[must_use]
    pub fn with_burst(mut self, burst: u32) -> Self {
        self.burst = Some(burst);
        self
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Invalid rate limit format: {value} (expected 'N/period')")]
    InvalidFormat { value: String },

    #[error("Invalid number in rate limit: {value}")]
    InvalidNumber { value: String },

    #[error("Invalid period in rate limit: {value} (expected second/minute/hour/day)")]
    InvalidPeriod { value: String },
}

/// Result of a rate limit check
#[derive(Debug, Clone)]
pub enum RateLimitResult {
    /// Request is allowed
    Allowed {
        remaining: u32,
        limit: u32,
        reset_at: SystemTime,
    },
    /// Request should be queued (backpressure)
    Queued {
        position: usize,
        estimated_wait: Duration,
    },
    /// Request is rate limited
    Limited { retry_after: Duration, limit: u32 },
    /// System is overloaded, shed load
    Overloaded,
}

/// Trait for rate limiter implementations (injectable for testing)
#[async_trait]
pub trait RateLimiter: Send + Sync {
    /// Check if request is allowed
    async fn check(&self, key: &str, limit: &RateLimit) -> RateLimitResult;

    /// Record a request (after processing, for sliding window)
    async fn record(&self, key: &str, limit: &RateLimit);

    /// Get current state for a key (for metrics/debugging)
    async fn get_state(&self, key: &str) -> Option<RateLimitState>;
}

#[derive(Debug, Clone)]
pub struct RateLimitState {
    pub current_count: u32,
    pub window_start: SystemTime,
    pub queue_depth: usize,
}

/// Mock rate limiter for testing
#[cfg(any(test, feature = "testing"))]
pub struct MockRateLimiter {
    pub results: std::sync::Arc<std::sync::Mutex<Vec<RateLimitResult>>>,
    pub calls: std::sync::Arc<std::sync::Mutex<Vec<(String, RateLimit)>>>,
}

#[cfg(any(test, feature = "testing"))]
impl MockRateLimiter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            results: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    #[must_use]
    pub fn with_results(results: Vec<RateLimitResult>) -> Self {
        Self {
            results: std::sync::Arc::new(std::sync::Mutex::new(results)),
            calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }
}

#[cfg(any(test, feature = "testing"))]
impl Default for MockRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "testing"))]
#[async_trait]
impl RateLimiter for MockRateLimiter {
    async fn check(&self, key: &str, limit: &RateLimit) -> RateLimitResult {
        self.calls
            .lock()
            .unwrap()
            .push((key.to_string(), limit.clone()));
        self.results
            .lock()
            .unwrap()
            .pop()
            .unwrap_or(RateLimitResult::Allowed {
                remaining: limit.requests,
                limit: limit.requests,
                reset_at: SystemTime::now() + limit.window,
            })
    }

    async fn record(&self, _key: &str, _limit: &RateLimit) {}

    async fn get_state(&self, _key: &str) -> Option<RateLimitState> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_parse() {
        let limit = RateLimit::parse("100/minute").unwrap();
        assert_eq!(limit.requests, 100);
        assert_eq!(limit.window, Duration::from_secs(60));

        let limit = RateLimit::parse("10/second").unwrap();
        assert_eq!(limit.requests, 10);
        assert_eq!(limit.window, Duration::from_secs(1));

        let limit = RateLimit::parse("1000/hour").unwrap();
        assert_eq!(limit.requests, 1000);
        assert_eq!(limit.window, Duration::from_secs(3600));
    }

    #[test]
    fn test_rate_limit_parse_invalid() {
        assert!(RateLimit::parse("invalid").is_err());
        assert!(RateLimit::parse("abc/minute").is_err());
        assert!(RateLimit::parse("100/lightyear").is_err());
    }

    #[tokio::test]
    async fn test_mock_rate_limiter() {
        let mock = MockRateLimiter::with_results(vec![
            RateLimitResult::Allowed {
                remaining: 5,
                limit: 10,
                reset_at: SystemTime::now(),
            },
            RateLimitResult::Limited {
                retry_after: Duration::from_secs(60),
                limit: 10,
            },
        ]);

        let limit = RateLimit::parse("10/minute").unwrap();

        // First call returns Limited (LIFO)
        let result = mock.check("key", &limit).await;
        assert!(matches!(result, RateLimitResult::Limited { .. }));

        // Second call returns Allowed
        let result = mock.check("key", &limit).await;
        assert!(matches!(result, RateLimitResult::Allowed { .. }));

        // Verify calls were recorded
        let calls = mock.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
    }
}
