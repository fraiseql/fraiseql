//! Security constraints (rate limiting, IP filtering, complexity analysis)
//!
//! This module provides:
//! - Rate limiting using token bucket algorithm
//! - IP allowlist/blocklist with CIDR support
//! - Query complexity analysis

use governor::{DefaultDirectRateLimiter, Quota};
use ipnetwork::IpNetwork;
use std::collections::HashMap;
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Rate limiter using token bucket algorithm
#[derive(Clone)]
pub struct RateLimiter {
    limiters: Arc<RwLock<HashMap<String, DefaultDirectRateLimiter>>>,
    quota: Quota,
}

impl RateLimiter {
    /// Create a new rate limiter
    ///
    /// # Arguments
    ///
    /// * `max_requests` - Maximum requests allowed per window
    /// * `window_seconds` - Time window in seconds
    pub fn new(max_requests: u32, _window_seconds: u64) -> Self {
        // Create quota (requests per second)
        let quota =
            Quota::per_second(NonZeroU32::new(max_requests).expect("max_requests must be > 0"));

        Self {
            limiters: Arc::new(RwLock::new(HashMap::new())),
            quota,
        }
    }

    /// Check if request is allowed for the given key
    ///
    /// # Arguments
    ///
    /// * `key` - Rate limit key (e.g., "user:123", "ip:192.168.1.1")
    ///
    /// # Returns
    ///
    /// `true` if request is allowed, `false` if rate limited
    pub async fn check(&self, key: &str) -> bool {
        let mut limiters = self.limiters.write().await;

        let limiter = limiters
            .entry(key.to_string())
            .or_insert_with(|| DefaultDirectRateLimiter::direct(self.quota));

        limiter.check().is_ok()
    }

    /// Reset rate limit for a specific key
    pub async fn reset(&self, key: &str) {
        let mut limiters = self.limiters.write().await;
        limiters.remove(key);
    }
}

/// IP filter with allowlist and blocklist
#[derive(Clone)]
pub struct IpFilter {
    allowlist: Vec<IpNetwork>,
    blocklist: Vec<IpNetwork>,
}

impl IpFilter {
    /// Create a new IP filter
    ///
    /// # Arguments
    ///
    /// * `allowlist` - CIDR ranges to allow (empty = allow all except blocked)
    /// * `blocklist` - CIDR ranges to block
    ///
    /// # Errors
    ///
    /// Returns error if CIDR parsing fails
    pub fn new(allowlist: Vec<String>, blocklist: Vec<String>) -> Result<Self, String> {
        let allowlist_parsed: Result<Vec<_>, _> =
            allowlist.iter().map(|s| s.parse::<IpNetwork>()).collect();

        let blocklist_parsed: Result<Vec<_>, _> =
            blocklist.iter().map(|s| s.parse::<IpNetwork>()).collect();

        Ok(Self {
            allowlist: allowlist_parsed.map_err(|e| e.to_string())?,
            blocklist: blocklist_parsed.map_err(|e| e.to_string())?,
        })
    }

    /// Check if IP is allowed
    ///
    /// # Arguments
    ///
    /// * `ip` - IP address to check
    ///
    /// # Returns
    ///
    /// `true` if IP is allowed, `false` if blocked
    pub async fn check(&self, ip: &str) -> bool {
        let ip_addr: IpAddr = match ip.parse() {
            Ok(addr) => addr,
            Err(_) => return false,
        };

        // Check blocklist first
        if self.blocklist.iter().any(|net| net.contains(ip_addr)) {
            return false;
        }

        // If allowlist is empty, allow all (except blocked)
        if self.allowlist.is_empty() {
            return true;
        }

        // Check allowlist
        self.allowlist.iter().any(|net| net.contains(ip_addr))
    }
}

/// Query complexity analyzer
#[derive(Clone)]
pub struct ComplexityAnalyzer {
    max_complexity: usize,
}

impl ComplexityAnalyzer {
    /// Create a new complexity analyzer
    ///
    /// # Arguments
    ///
    /// * `max_complexity` - Maximum allowed complexity score
    pub fn new(max_complexity: usize) -> Self {
        Self { max_complexity }
    }

    /// Check if query complexity is acceptable
    ///
    /// # Arguments
    ///
    /// * `query` - GraphQL query string
    ///
    /// # Returns
    ///
    /// `true` if complexity is acceptable, `false` if too complex
    pub async fn check(&self, query: &str) -> bool {
        let complexity = self.calculate_complexity(query);
        complexity <= self.max_complexity
    }

    /// Calculate query complexity
    ///
    /// Uses a simple heuristic:
    /// - Depth score: number of nesting levels × 10
    /// - Field score: number of fields
    /// - Total complexity = depth score + field score
    fn calculate_complexity(&self, query: &str) -> usize {
        // Count nesting depth (braces)
        let depth = query.matches('{').count();

        // Count fields (simplified: word count)
        let fields = query
            .split_whitespace()
            .filter(|w| !w.contains('{') && !w.contains('}'))
            .count();

        depth * 10 + fields
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter() {
        let limiter = RateLimiter::new(2, 60);

        // First 2 requests should pass
        assert!(limiter.check("user:1").await);
        assert!(limiter.check("user:1").await);

        // 3rd request should be blocked
        assert!(!limiter.check("user:1").await);

        // Different user should still have quota
        assert!(limiter.check("user:2").await);
    }

    #[tokio::test]
    async fn test_ip_filter_blocklist() {
        let filter = IpFilter::new(vec![], vec!["10.0.0.0/8".to_string()]).unwrap();

        assert!(filter.check("192.168.1.1").await);
        assert!(!filter.check("10.0.0.1").await);
    }

    #[tokio::test]
    async fn test_ip_filter_allowlist() {
        let filter = IpFilter::new(vec!["192.168.1.0/24".to_string()], vec![]).unwrap();

        assert!(filter.check("192.168.1.100").await);
        assert!(!filter.check("10.0.0.1").await);
    }

    #[tokio::test]
    async fn test_complexity_analyzer() {
        let analyzer = ComplexityAnalyzer::new(50);

        // Simple query
        let simple = "{ user { id name } }";
        assert!(analyzer.check(simple).await);

        // Complex query
        let complex = "{ users { posts { comments { author { posts { comments { id } } } } } } }";
        assert!(!analyzer.check(complex).await);
    }
}
