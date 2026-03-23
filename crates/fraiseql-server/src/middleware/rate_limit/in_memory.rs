//! In-memory token-bucket rate limiter backend.

use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use tokio::sync::RwLock;
use tracing::debug;

/// Total number of rate limit denials (IP + user + path combined).
static DENIALS_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Total rate limit denials across all in-memory buckets.
pub fn denials_total() -> u64 {
    DENIALS_TOTAL.load(Ordering::Relaxed)
}

use super::{
    config::{CheckResult, RateLimitConfig, RateLimitingSecurityConfig},
    key::{PathRateLimit, path_matches_rule},
    token_bucket::TokenBucket,
};

/// Number of shards used to distribute rate-limit buckets.
///
/// 16 shards keeps per-shard contention low at high RPS while remaining
/// small enough that iterating all shards (e.g. during cleanup) is cheap.
const SHARD_COUNT: usize = 16;

/// Sharded hash map that distributes `TokenBucket` entries across
/// [`SHARD_COUNT`] independent `RwLock`-protected shards, reducing
/// lock contention under concurrent access.
struct ShardedBuckets {
    shards: [RwLock<HashMap<String, TokenBucket>>; SHARD_COUNT],
}

impl ShardedBuckets {
    /// Create a new `ShardedBuckets` with empty shards.
    fn new() -> Self {
        Self {
            shards: std::array::from_fn(|_| RwLock::new(HashMap::new())),
        }
    }

    /// Determine which shard a key belongs to.
    fn shard_index(key: &str) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish() as usize % SHARD_COUNT
    }

    /// Get a write lock on the shard for `key`.
    async fn shard_for(
        &self,
        key: &str,
    ) -> tokio::sync::RwLockWriteGuard<'_, HashMap<String, TokenBucket>> {
        self.shards[Self::shard_index(key)].write().await
    }

    /// Retain entries across all shards, returning the total number evicted.
    async fn retain(&self, mut predicate: impl FnMut(&str, &TokenBucket) -> bool) -> usize {
        let mut evicted = 0;
        for shard in &self.shards {
            let mut map = shard.write().await;
            let before = map.len();
            map.retain(|k, v| predicate(k, v));
            evicted += before - map.len();
        }
        evicted
    }

    /// Total number of entries across all shards.
    async fn len(&self) -> usize {
        let mut total = 0;
        for shard in &self.shards {
            total += shard.read().await.len();
        }
        total
    }
}

/// In-memory token-bucket rate limiter.
pub struct InMemoryRateLimiter {
    pub(super) config:     RateLimitConfig,
    /// IP -> `TokenBucket` (global limit), sharded.
    ip_buckets:            Arc<ShardedBuckets>,
    /// User ID -> `TokenBucket`, sharded.
    user_buckets:          Arc<ShardedBuckets>,
    /// Per-path rules (from `[security.rate_limiting]` auth endpoint fields).
    pub(super) path_rules: Vec<PathRateLimit>,
    /// `"path_prefix:ip"` -> `TokenBucket`, sharded.
    path_ip_buckets:       Arc<ShardedBuckets>,
}

impl InMemoryRateLimiter {
    /// Create new in-memory rate limiter.
    pub(super) fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            ip_buckets: Arc::new(ShardedBuckets::new()),
            user_buckets: Arc::new(ShardedBuckets::new()),
            path_rules: Vec::new(),
            path_ip_buckets: Arc::new(ShardedBuckets::new()),
        }
    }

    /// Attach per-path rules derived from `[security.rate_limiting]` auth endpoint fields.
    ///
    /// Converts max-requests-per-window into token-per-second refill rates.
    #[must_use]
    pub(super) fn with_path_rules_from_security(
        mut self,
        sec: &RateLimitingSecurityConfig,
    ) -> Self {
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

    /// Build the composite shard key for path+IP buckets.
    fn path_ip_key(path_prefix: &str, ip: &str) -> String {
        let mut key = String::with_capacity(path_prefix.len() + 1 + ip.len());
        key.push_str(path_prefix);
        key.push(':');
        key.push_str(ip);
        key
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

        let key = Self::path_ip_key(&rule.path_prefix, ip);
        let (tokens_per_sec, burst) = (rule.tokens_per_sec, rule.burst);

        let mut shard = self.path_ip_buckets.shard_for(&key).await;
        let bucket = shard.entry(key).or_insert_with(|| TokenBucket::new(burst, tokens_per_sec));

        let allowed = bucket.try_consume(1.0);
        let remaining = bucket.token_count();
        drop(shard);

        if allowed {
            CheckResult::allow(remaining)
        } else {
            DENIALS_TOTAL.fetch_add(1, Ordering::Relaxed);
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
    pub(super) const fn config(&self) -> &RateLimitConfig {
        &self.config
    }

    /// Check if request is allowed for given IP.
    pub(super) async fn check_ip_limit(&self, ip: &str) -> CheckResult {
        if !self.config.enabled {
            return CheckResult::allow(f64::from(self.config.burst_size));
        }

        let mut shard = self.ip_buckets.shard_for(ip).await;
        let bucket = shard.entry(ip.to_string()).or_insert_with(|| {
            TokenBucket::new(f64::from(self.config.burst_size), f64::from(self.config.rps_per_ip))
        });

        let allowed = bucket.try_consume(1.0);
        let remaining = bucket.token_count();
        drop(shard);

        if allowed {
            CheckResult::allow(remaining)
        } else {
            DENIALS_TOTAL.fetch_add(1, Ordering::Relaxed);
            debug!(ip = ip, "Rate limit exceeded for IP");
            let rps = self.config.rps_per_ip;
            let retry = if rps == 0 {
                1
            } else {
                ((1.0_f64 / f64::from(rps)).ceil() as u32).max(1)
            };
            CheckResult::deny(retry)
        }
    }

    /// Check if request is allowed for given user.
    pub(super) async fn check_user_limit(&self, user_id: &str) -> CheckResult {
        if !self.config.enabled {
            return CheckResult::allow(f64::from(self.config.burst_size));
        }

        let mut shard = self.user_buckets.shard_for(user_id).await;
        let bucket = shard.entry(user_id.to_string()).or_insert_with(|| {
            TokenBucket::new(f64::from(self.config.burst_size), f64::from(self.config.rps_per_user))
        });

        let allowed = bucket.try_consume(1.0);
        let remaining = bucket.token_count();
        drop(shard);

        if allowed {
            CheckResult::allow(remaining)
        } else {
            DENIALS_TOTAL.fetch_add(1, Ordering::Relaxed);
            debug!(user_id = user_id, "Rate limit exceeded for user");
            let rps = self.config.rps_per_user;
            let retry = if rps == 0 {
                1
            } else {
                ((1.0_f64 / f64::from(rps)).ceil() as u32).max(1)
            };
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

        let evicted_ip = self.ip_buckets.retain(|_, b| b.last_refill >= ip_threshold).await;

        let evicted_user = self.user_buckets.retain(|_, b| b.last_refill >= user_threshold).await;

        // Reason: path buckets share the IP refill threshold since they are keyed by IP.
        let _evicted_path = self.path_ip_buckets.retain(|_, b| b.last_refill >= ip_threshold).await;

        debug!(evicted_ip, evicted_user, "Rate limiter cleanup complete");
    }

    /// Number of active rate limit keys (IP + user + path buckets).
    pub(super) async fn active_key_count(&self) -> usize {
        let ip = self.ip_buckets.len().await;
        let user = self.user_buckets.len().await;
        let path = self.path_ip_buckets.len().await;
        ip + user + path
    }

    /// Number of per-path rate limit rules registered.
    pub(super) const fn path_rule_count(&self) -> usize {
        self.path_rules.len()
    }

    /// Seconds a client should wait before retrying after a per-path rate limit rejection.
    ///
    /// Returns `ceil(1 / tokens_per_sec)` for the rule matching `path`, or 1 if no rule
    /// matches (which shouldn't happen in practice — callers only invoke this after a
    /// rejection).
    pub(super) fn retry_after_for_path(&self, path: &str) -> u32 {
        if let Some(rule) = self.path_rules.iter().find(|r| path_matches_rule(path, &r.path_prefix))
        {
            if rule.tokens_per_sec > 0.0 {
                return ((1.0_f64 / rule.tokens_per_sec).ceil() as u32).max(1);
            }
        }
        1
    }
}
