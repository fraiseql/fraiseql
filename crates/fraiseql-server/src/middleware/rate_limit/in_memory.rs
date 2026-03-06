//! In-memory token-bucket rate limiter backend.

use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;
use tracing::debug;

use super::config::{CheckResult, RateLimitConfig, RateLimitingSecurityConfig};
use super::key::{PathRateLimit, path_matches_rule};
use super::token_bucket::TokenBucket;

/// In-memory token-bucket rate limiter.
pub struct InMemoryRateLimiter {
    pub(super) config:          RateLimitConfig,
    // IP -> TokenBucket (global limit)
    pub(super) ip_buckets:      Arc<RwLock<HashMap<String, TokenBucket>>>,
    // User ID -> TokenBucket
    pub(super) user_buckets:    Arc<RwLock<HashMap<String, TokenBucket>>>,
    // Per-path rules (from [security.rate_limiting] auth endpoint fields)
    pub(super) path_rules:      Vec<PathRateLimit>,
    // (path_prefix, ip) -> TokenBucket
    pub(super) path_ip_buckets: Arc<RwLock<HashMap<(String, String), TokenBucket>>>,
}

impl InMemoryRateLimiter {
    /// Create new in-memory rate limiter.
    pub(super) fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            ip_buckets:      Arc::new(RwLock::new(HashMap::new())),
            user_buckets:    Arc::new(RwLock::new(HashMap::new())),
            path_rules:      Vec::new(),
            path_ip_buckets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Attach per-path rules derived from `[security.rate_limiting]` auth endpoint fields.
    ///
    /// Converts max-requests-per-window into token-per-second refill rates.
    #[must_use]
    pub(super) fn with_path_rules_from_security(mut self, sec: &RateLimitingSecurityConfig) -> Self {
        let mut rules = Vec::new();

        if sec.auth_start_max_requests > 0 && sec.auth_start_window_secs > 0 {
            rules.push(PathRateLimit {
                path_prefix:    "/auth/start".to_string(),
                tokens_per_sec: f64::from(sec.auth_start_max_requests)
                    / sec.auth_start_window_secs as f64,
                burst:          f64::from(sec.auth_start_max_requests),
            });
        }
        if sec.auth_callback_max_requests > 0 && sec.auth_callback_window_secs > 0 {
            rules.push(PathRateLimit {
                path_prefix:    "/auth/callback".to_string(),
                tokens_per_sec: f64::from(sec.auth_callback_max_requests)
                    / sec.auth_callback_window_secs as f64,
                burst:          f64::from(sec.auth_callback_max_requests),
            });
        }
        if sec.auth_refresh_max_requests > 0 && sec.auth_refresh_window_secs > 0 {
            rules.push(PathRateLimit {
                path_prefix:    "/auth/refresh".to_string(),
                tokens_per_sec: f64::from(sec.auth_refresh_max_requests)
                    / sec.auth_refresh_window_secs as f64,
                burst:          f64::from(sec.auth_refresh_max_requests),
            });
        }

        self.path_rules = rules;
        self
    }

    /// Check if request to `path` from `ip` is within the per-path limit.
    ///
    /// Returns an allowed [`CheckResult`] when no rule governs the path.
    /// Returns a denied result only when a matching rule exists and the bucket
    /// is empty.  `CheckResult::retry_after_secs` is set to the path-window
    /// interval (`ceil(1 / tokens_per_sec)`).
    pub(super) async fn check_path_limit(&self, path: &str, ip: &str) -> CheckResult {
        if !self.config.enabled {
            return CheckResult::allow(f64::from(self.config.burst_size));
        }

        let rule = self.path_rules.iter().find(|r| path_matches_rule(path, &r.path_prefix));
        let Some(rule) = rule else {
            return CheckResult::allow(f64::from(self.config.burst_size));
        };

        let key = (rule.path_prefix.clone(), ip.to_string());
        let (tokens_per_sec, burst) = (rule.tokens_per_sec, rule.burst);

        let mut buckets = self.path_ip_buckets.write().await;
        let bucket = buckets
            .entry(key)
            .or_insert_with(|| TokenBucket::new(burst, tokens_per_sec));

        let allowed = bucket.try_consume(1.0);
        let remaining = bucket.token_count();
        drop(buckets);

        if allowed {
            CheckResult::allow(remaining)
        } else {
            debug!(ip = ip, path = path, "Per-path rate limit exceeded");
            let retry = if tokens_per_sec > 0.0 {
                ((1.0_f64 / tokens_per_sec).ceil() as u32).max(1)
            } else {
                1
            };
            CheckResult::deny(retry)
        }
    }

    /// Get rate limiter configuration.
    pub(super) fn config(&self) -> &RateLimitConfig {
        &self.config
    }

    /// Check if request is allowed for given IP.
    pub(super) async fn check_ip_limit(&self, ip: &str) -> CheckResult {
        if !self.config.enabled {
            return CheckResult::allow(f64::from(self.config.burst_size));
        }

        let mut buckets = self.ip_buckets.write().await;
        let bucket = buckets.entry(ip.to_string()).or_insert_with(|| {
            TokenBucket::new(f64::from(self.config.burst_size), f64::from(self.config.rps_per_ip))
        });

        let allowed = bucket.try_consume(1.0);
        let remaining = bucket.token_count();
        drop(buckets);

        if allowed {
            CheckResult::allow(remaining)
        } else {
            debug!(ip = ip, "Rate limit exceeded for IP");
            let rps = self.config.rps_per_ip;
            let retry = if rps == 0 { 1 } else { ((1.0_f64 / f64::from(rps)).ceil() as u32).max(1) };
            CheckResult::deny(retry)
        }
    }

    /// Check if request is allowed for given user.
    pub(super) async fn check_user_limit(&self, user_id: &str) -> CheckResult {
        if !self.config.enabled {
            return CheckResult::allow(f64::from(self.config.burst_size));
        }

        let mut buckets = self.user_buckets.write().await;
        let bucket = buckets.entry(user_id.to_string()).or_insert_with(|| {
            TokenBucket::new(
                f64::from(self.config.burst_size),
                f64::from(self.config.rps_per_user),
            )
        });

        let allowed = bucket.try_consume(1.0);
        let remaining = bucket.token_count();
        drop(buckets);

        if allowed {
            CheckResult::allow(remaining)
        } else {
            debug!(user_id = user_id, "Rate limit exceeded for user");
            let rps = self.config.rps_per_user;
            let retry =
                if rps == 0 { 1 } else { ((1.0_f64 / f64::from(rps)).ceil() as u32).max(1) };
            CheckResult::deny(retry)
        }
    }

    /// Evict stale in-memory buckets (called by background cleanup task).
    ///
    /// A bucket is stale once it has been idle for longer than the time required
    /// to fully refill from empty (`burst_size / rps_per_ip`).  At that point the
    /// next request would start a fresh full bucket anyway, so the entry is safe
    /// to remove.
    pub(super) async fn cleanup(&self) {
        let ip_refill_secs = if self.config.rps_per_ip == 0 {
            self.config.cleanup_interval_secs as f64
        } else {
            f64::from(self.config.burst_size) / f64::from(self.config.rps_per_ip)
        };
        let user_refill_secs = if self.config.rps_per_user == 0 {
            self.config.cleanup_interval_secs as f64
        } else {
            f64::from(self.config.burst_size) / f64::from(self.config.rps_per_user)
        };

        let now = std::time::Instant::now();
        let ip_threshold = now
            .checked_sub(std::time::Duration::from_secs_f64(ip_refill_secs))
            .unwrap_or(now);
        let user_threshold = now
            .checked_sub(std::time::Duration::from_secs_f64(user_refill_secs))
            .unwrap_or(now);

        let mut ip_buckets = self.ip_buckets.write().await;
        let before_ip = ip_buckets.len();
        ip_buckets.retain(|_, b| b.last_refill >= ip_threshold);
        let evicted_ip = before_ip - ip_buckets.len();
        drop(ip_buckets);

        let mut user_buckets = self.user_buckets.write().await;
        let before_user = user_buckets.len();
        user_buckets.retain(|_, b| b.last_refill >= user_threshold);
        let evicted_user = before_user - user_buckets.len();
        drop(user_buckets);

        let mut path_buckets = self.path_ip_buckets.write().await;
        path_buckets.retain(|_, b| b.last_refill >= ip_threshold);
        drop(path_buckets);

        debug!(evicted_ip, evicted_user, "Rate limiter cleanup complete");
    }

    /// Number of per-path rate limit rules registered.
    pub(super) fn path_rule_count(&self) -> usize {
        self.path_rules.len()
    }

    /// Seconds a client should wait before retrying after a per-path rate limit rejection.
    ///
    /// Returns `ceil(1 / tokens_per_sec)` for the rule matching `path`, or 1 if no rule
    /// matches (which shouldn't happen in practice — callers only invoke this after a
    /// rejection).
    pub(super) fn retry_after_for_path(&self, path: &str) -> u32 {
        if let Some(rule) =
            self.path_rules.iter().find(|r| path_matches_rule(path, &r.path_prefix))
        {
            if rule.tokens_per_sec > 0.0 {
                return ((1.0_f64 / rule.tokens_per_sec).ceil() as u32).max(1);
            }
        }
        1
    }
}
