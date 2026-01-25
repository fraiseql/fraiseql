//! In-memory rate limiter using sliding window with backpressure support.

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use async_trait::async_trait;
use tokio::sync::RwLock;

use super::{RateLimit, RateLimitResult, RateLimitState, RateLimiter};
use crate::config::rate_limiting::BackpressureConfig;

/// In-memory rate limiter using sliding window with backpressure support
pub struct MemoryRateLimiter {
    windows: Arc<RwLock<HashMap<String, SlidingWindow>>>,
    config:  BackpressureConfig,
}

struct SlidingWindow {
    /// Timestamps of requests in the window
    requests:    Vec<Instant>,
    /// Current queue depth
    queue_depth: std::sync::atomic::AtomicUsize,
}

impl SlidingWindow {
    fn new(_config: &BackpressureConfig) -> Self {
        Self {
            requests:    Vec::new(),
            queue_depth: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn count_in_window(&self, window: Duration) -> u32 {
        let now = Instant::now();
        let cutoff = now - window;
        self.requests.iter().filter(|&&t| t > cutoff).count() as u32
    }

    fn cleanup(&mut self, window: Duration) {
        let now = Instant::now();
        let cutoff = now - window;
        self.requests.retain(|&t| t > cutoff);
    }

    fn record(&mut self) {
        self.requests.push(Instant::now());
    }

    fn _remaining(&self, limit: u32, window: Duration) -> u32 {
        limit.saturating_sub(self.count_in_window(window))
    }

    fn reset_at(&self, window: Duration) -> SystemTime {
        if let Some(&oldest) = self.requests.first() {
            let reset_instant = oldest + window;
            let now = Instant::now();
            if reset_instant > now {
                SystemTime::now() + (reset_instant - now)
            } else {
                SystemTime::now()
            }
        } else {
            SystemTime::now() + window
        }
    }
}

impl MemoryRateLimiter {
    #[must_use]
    pub fn new(config: BackpressureConfig) -> Self {
        let windows: Arc<RwLock<HashMap<String, SlidingWindow>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let windows_clone = Arc::clone(&windows);

        // Spawn cleanup task
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                let mut windows_guard = windows_clone.write().await;
                // Remove empty windows
                windows_guard.retain(|_, w| !w.requests.is_empty());
            }
        });

        Self { windows, config }
    }
}

#[async_trait]
impl RateLimiter for MemoryRateLimiter {
    async fn check(&self, key: &str, limit: &RateLimit) -> RateLimitResult {
        let effective_limit = limit.burst.unwrap_or(limit.requests);

        let mut windows = self.windows.write().await;
        let window = windows
            .entry(key.to_string())
            .or_insert_with(|| SlidingWindow::new(&self.config));

        // Cleanup old requests
        window.cleanup(limit.window);

        let current_count = window.count_in_window(limit.window);

        if current_count < effective_limit {
            // Under limit, allow
            RateLimitResult::Allowed {
                remaining: effective_limit - current_count - 1,
                limit:     limit.requests,
                reset_at:  window.reset_at(limit.window),
            }
        } else if self.config.queue_enabled {
            // At limit but queueing is enabled
            let queue_depth = window.queue_depth.load(std::sync::atomic::Ordering::SeqCst);

            if queue_depth >= self.config.max_queue_size {
                if self.config.load_shed {
                    RateLimitResult::Overloaded
                } else {
                    RateLimitResult::Limited {
                        retry_after: window
                            .reset_at(limit.window)
                            .duration_since(SystemTime::now())
                            .unwrap_or(limit.window),
                        limit:       limit.requests,
                    }
                }
            } else {
                // Queue the request
                window.queue_depth.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                RateLimitResult::Queued {
                    position:       queue_depth + 1,
                    estimated_wait: Duration::from_millis(
                        ((queue_depth as u64) + 1)
                            * (limit.window.as_millis() as u64 / u64::from(limit.requests)),
                    ),
                }
            }
        } else {
            // Rate limited
            RateLimitResult::Limited {
                retry_after: window
                    .reset_at(limit.window)
                    .duration_since(SystemTime::now())
                    .unwrap_or(limit.window),
                limit:       limit.requests,
            }
        }
    }

    async fn record(&self, key: &str, _limit: &RateLimit) {
        let mut windows = self.windows.write().await;
        if let Some(window) = windows.get_mut(key) {
            window.record();
        }
    }

    async fn get_state(&self, key: &str) -> Option<RateLimitState> {
        let windows = self.windows.read().await;
        windows.get(key).map(|w| RateLimitState {
            current_count: w.requests.len() as u32,
            window_start:  SystemTime::now(), // Simplified
            queue_depth:   w.queue_depth.load(std::sync::atomic::Ordering::SeqCst),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_rate_limiter_allows_under_limit() {
        let config = BackpressureConfig::default();
        let limiter = MemoryRateLimiter::new(config);
        let limit = RateLimit::parse("10/minute").unwrap();

        for _ in 0..10 {
            let result = limiter.check("test_key", &limit).await;
            assert!(matches!(result, RateLimitResult::Allowed { .. }));
            limiter.record("test_key", &limit).await;
        }
    }

    #[tokio::test]
    async fn test_memory_rate_limiter_blocks_over_limit() {
        let config = BackpressureConfig::default();
        let limiter = MemoryRateLimiter::new(config);
        let limit = RateLimit::parse("5/minute").unwrap();

        // Use up the limit
        for _ in 0..5 {
            let result = limiter.check("test_key", &limit).await;
            assert!(matches!(result, RateLimitResult::Allowed { .. }));
            limiter.record("test_key", &limit).await;
        }

        // Should be limited
        let result = limiter.check("test_key", &limit).await;
        assert!(matches!(result, RateLimitResult::Limited { .. }));
    }

    #[tokio::test]
    async fn test_memory_rate_limiter_with_burst() {
        let config = BackpressureConfig::default();
        let limiter = MemoryRateLimiter::new(config);
        let limit = RateLimit::parse("5/minute").unwrap().with_burst(10);

        // Burst allows up to 10 requests
        for _ in 0..10 {
            let result = limiter.check("test_key", &limit).await;
            assert!(matches!(result, RateLimitResult::Allowed { .. }));
            limiter.record("test_key", &limit).await;
        }

        // 11th request should be limited
        let result = limiter.check("test_key", &limit).await;
        assert!(matches!(result, RateLimitResult::Limited { .. }));
    }

    #[tokio::test]
    async fn test_memory_rate_limiter_backpressure_queue() {
        let config = BackpressureConfig {
            queue_enabled:  true,
            max_queue_size: 5,
            queue_timeout:  "5s".to_string(),
            load_shed:      false,
        };
        let limiter = MemoryRateLimiter::new(config);
        let limit = RateLimit::parse("2/minute").unwrap();

        // Use up the limit
        for _ in 0..2 {
            let result = limiter.check("test_key", &limit).await;
            assert!(matches!(result, RateLimitResult::Allowed { .. }));
            limiter.record("test_key", &limit).await;
        }

        // Next requests should be queued
        let result = limiter.check("test_key", &limit).await;
        assert!(matches!(result, RateLimitResult::Queued { .. }));
    }

    #[tokio::test]
    async fn test_memory_rate_limiter_load_shedding() {
        let config = BackpressureConfig {
            queue_enabled:  true,
            max_queue_size: 2,
            queue_timeout:  "5s".to_string(),
            load_shed:      true,
        };
        let limiter = MemoryRateLimiter::new(config);
        let limit = RateLimit::parse("1/minute").unwrap();

        // Use up the limit
        let result = limiter.check("test_key", &limit).await;
        assert!(matches!(result, RateLimitResult::Allowed { .. }));
        limiter.record("test_key", &limit).await;

        // Fill the queue
        for _ in 0..2 {
            let result = limiter.check("test_key", &limit).await;
            assert!(matches!(result, RateLimitResult::Queued { .. }));
        }

        // Next request should be overloaded
        let result = limiter.check("test_key", &limit).await;
        assert!(matches!(result, RateLimitResult::Overloaded));
    }
}
