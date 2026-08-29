//! In-memory token-bucket rate limiter backend.

use std::sync::Arc;

use dashmap::DashMap;
use tracing::debug;

use super::{
    config::{CheckResult, RateLimitConfig, RateLimitingSecurityConfig},
    key::{PathRateLimit, path_matches_rule},
    token_bucket::TokenBucket,
};

/// How many entries to sample when making room for a new bucket.
///
/// Redis uses 5 for its approximated LRU and finds it close to exact; the cost is
/// O(SAMPLE) per eviction, paid only at capacity.
const EVICTION_SAMPLE: usize = 8;

/// Make room for one bucket by evicting the least-recently-used of a small sample,
/// rather than refusing the newcomer (#1143).
///
/// **Why evict rather than deny.** Denying an unseen key is a denial of service
/// against strangers — and `ip_buckets` only grows on requests that have no bucket
/// yet, so the strangers are exactly the unauthenticated ones: every login and
/// registration attempt. Degrade accuracy, never availability. #1080 made a full map
/// recoverable rather than permanent; this makes it non-fatal in the first place.
///
/// **Why sampling rather than exact LRU.** An exact LRU needs an intrusive list behind
/// a mutex, which would serialise the request hot path and give back the very property
/// [`DashMap`] is here for. Sampling and evicting the oldest seen is the same trade
/// Redis makes, and costs O(`EVICTION_SAMPLE`) with no global lock. The sample is
/// whatever iteration yields first, so it is biased toward some shards; that is
/// acceptable for an approximation whose worst case is evicting a slightly younger
/// bucket than the true oldest.
///
/// **Why it is nearly free.** A bucket idle longer than its refill window has already
/// refilled to full, so evicting it loses no enforcement at all — the same criterion
/// [`cleanup`](InMemoryRateLimiter::cleanup) uses. Least-recently-used correlates with
/// most-idle, so the sampled-oldest is approximately the already-full. The worst case,
/// evicting a partly-drained bucket, costs at most `burst_size` of accuracy in a
/// regime where the alternative was refusing real clients.
///
/// **This is only safe because the key space is bounded.** With an attacker-inflatable
/// key (the pre-#1143 tenant and unverified subject), eviction would have been a
/// cheap way to flush *other* clients' buckets and reset one's own. Keys now derive
/// from the peer address or a verified identity, so mounting that needs real
/// addresses, against which the per-IP limit still applies individually.
fn evict_one_lru<K>(map: &DashMap<K, TokenBucket>)
where
    K: std::hash::Hash + Eq + Clone,
{
    let victim = {
        let mut oldest: Option<(K, std::time::Instant)> = None;
        for entry in map.iter().take(EVICTION_SAMPLE) {
            let seen = entry.value().last_refill;
            if oldest.as_ref().is_none_or(|(_, best)| seen < *best) {
                oldest = Some((entry.key().clone(), seen));
            }
        }
        oldest.map(|(k, _)| k)
    };
    // The iterator above holds shard guards; it must be dropped before `remove`
    // takes a write guard on the same shard, or this deadlocks.
    if let Some(key) = victim {
        map.remove(&key);
    }
}

/// In-memory token-bucket rate limiter.
///
/// Each bucket map is a [`DashMap`]: lookups/refills on the request hot path
/// take only a per-shard write reference, never an async lock, so unrelated
/// keys (different IPs / users / paths / tenants) never contend.  Capacity
/// checks against `max_buckets` are best-effort under heavy concurrent
/// insertion — total entries may oscillate around the cap by a small amount.
pub struct InMemoryRateLimiter {
    pub(super) config:          RateLimitConfig,
    // IP -> TokenBucket (global limit)
    pub(super) ip_buckets:      Arc<DashMap<String, TokenBucket>>,
    // User ID -> TokenBucket
    pub(super) user_buckets:    Arc<DashMap<String, TokenBucket>>,
    // Per-path rules (from [security.rate_limiting] auth endpoint fields)
    pub(super) path_rules:      Vec<PathRateLimit>,
    // (path_prefix, ip) -> TokenBucket
    pub(super) path_ip_buckets: Arc<DashMap<(String, String), TokenBucket>>,
    // tenant_key -> TokenBucket (per-tenant rate limit)
    pub(super) tenant_buckets:  Arc<DashMap<String, TokenBucket>>,
}

// Reason: mirrors the Redis-backed limiter's awaited surface so the call sites
// in `middleware_fn` are backend-agnostic. In-memory buckets resolve without
// I/O; the distributed backend does not.
#[allow(unknown_lints, clippy::unused_async_trait_impl)]
impl InMemoryRateLimiter {
    /// Create new in-memory rate limiter.
    pub(super) fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            ip_buckets: Arc::new(DashMap::new()),
            user_buckets: Arc::new(DashMap::new()),
            path_rules: Vec::new(),
            path_ip_buckets: Arc::new(DashMap::new()),
            tenant_buckets: Arc::new(DashMap::new()),
        }
    }

    /// Attach the per-path rules derived from `[security.rate_limiting]` auth
    /// endpoint fields — built by the shared [`PathRateLimit::rules_from_security`]
    /// so this backend and the Redis one cannot drift.
    #[must_use]
    pub(super) fn with_path_rules_from_security(
        mut self,
        sec: &RateLimitingSecurityConfig,
    ) -> Self {
        self.path_rules = PathRateLimit::rules_from_security(sec);
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

        // Best-effort capacity check: a parallel inserter racing past this point
        // may push us slightly above max_buckets, but never unboundedly.
        if !self.path_ip_buckets.contains_key(&key)
            && self.path_ip_buckets.len() >= self.config.max_buckets
        {
            debug!(ip = ip, path = path, "Path-IP bucket capacity reached — evicting LRU");
            evict_one_lru(&self.path_ip_buckets);
        }

        let (allowed, remaining) = {
            let mut bucket_ref = self
                .path_ip_buckets
                .entry(key)
                .or_insert_with(|| TokenBucket::new(burst, tokens_per_sec));
            let bucket = bucket_ref.value_mut();
            let allowed = bucket.try_consume(1.0);
            let remaining = bucket.token_count();
            (allowed, remaining)
        };

        if allowed {
            CheckResult::allow(remaining)
        } else {
            debug!(ip = ip, path = path, "Per-path rate limit exceeded");
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // Reason: ceil(1/tokens_per_sec) is always a small positive integer
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
    ///
    /// **The key is the IP and nothing else (#1143).** It used to be
    /// `format!("{tenant}:{ip}")`, with the tenant taken raw from an `X-Tenant-ID`
    /// header that nothing validated — and a fresh key is a fresh *full* bucket, so
    /// varying the header did not merely add map entries: it handed the caller an
    /// unlimited budget. Measured before the fix: 50 of 50 requests allowed against
    /// `rps_per_ip = 1, burst = 1`, from one IP, unauthenticated.
    ///
    /// A rate limiter's key space must be bounded by something the caller cannot
    /// inflate. That is the whole property; capacity caps and eviction are
    /// compensation for its absence, not substitutes for it. The Redis backend already
    /// keyed on the IP alone, so this also removes a silent disagreement between the
    /// two backends about what a bucket means.
    pub(super) async fn check_ip_limit(&self, ip: &str) -> CheckResult {
        if !self.config.enabled {
            return CheckResult::allow(f64::from(self.config.burst_size));
        }

        let key = ip.to_string();

        // Best-effort capacity check; see `check_path_limit` for why this races safely.
        if !self.ip_buckets.contains_key(&key) && self.ip_buckets.len() >= self.config.max_buckets {
            debug!(ip = ip, "IP bucket capacity reached — evicting LRU");
            evict_one_lru(&self.ip_buckets);
        }

        let (allowed, remaining) = {
            let mut bucket_ref = self.ip_buckets.entry(key).or_insert_with(|| {
                TokenBucket::new(
                    f64::from(self.config.burst_size),
                    f64::from(self.config.rps_per_ip),
                )
            });
            let bucket = bucket_ref.value_mut();
            let allowed = bucket.try_consume(1.0);
            let remaining = bucket.token_count();
            (allowed, remaining)
        };

        if allowed {
            CheckResult::allow(remaining)
        } else {
            debug!(ip = ip, "Rate limit exceeded for IP");
            let rps = self.config.rps_per_ip;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // Reason: ceil(1/rps) is always a small positive integer
            let retry = if rps == 0 {
                1
            } else {
                ((1.0_f64 / f64::from(rps)).ceil() as u32).max(1)
            };
            CheckResult::deny(retry)
        }
    }

    /// Check if request is allowed for given user.
    ///
    /// `user_id` must be a **verified** identity. The key is it and nothing else, for
    /// the same reason as [`check_ip_limit`](Self::check_ip_limit): the unvalidated
    /// tenant used to be folded in, so varying `X-Tenant-ID` minted a fresh full
    /// bucket per request.
    ///
    /// Callers are responsible for the "verified" half, and the HTTP middleware no
    /// longer calls this at all — it derived `user_id` from a JWT payload it decoded
    /// without checking the signature, which is attacker-chosen text. gRPC
    /// authenticates before calling, so its use is sound.
    pub(super) async fn check_user_limit(&self, user_id: &str) -> CheckResult {
        if !self.config.enabled {
            return CheckResult::allow(f64::from(self.config.burst_size));
        }

        let key = user_id.to_string();

        // Best-effort capacity check; see `check_path_limit` for why this races safely.
        if !self.user_buckets.contains_key(&key)
            && self.user_buckets.len() >= self.config.max_buckets
        {
            debug!(user_id = user_id, "User bucket capacity reached — evicting LRU");
            evict_one_lru(&self.user_buckets);
        }

        let (allowed, remaining) = {
            let mut bucket_ref = self.user_buckets.entry(key).or_insert_with(|| {
                TokenBucket::new(
                    f64::from(self.config.burst_size),
                    f64::from(self.config.rps_per_user),
                )
            });
            let bucket = bucket_ref.value_mut();
            let allowed = bucket.try_consume(1.0);
            let remaining = bucket.token_count();
            (allowed, remaining)
        };

        if allowed {
            CheckResult::allow(remaining)
        } else {
            debug!(user_id = user_id, "Rate limit exceeded for user");
            let rps = self.config.rps_per_user;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // Reason: ceil(1/rps) is always a small positive integer
            let retry = if rps == 0 {
                1
            } else {
                ((1.0_f64 / f64::from(rps)).ceil() as u32).max(1)
            };
            CheckResult::deny(retry)
        }
    }

    /// Check if a request is allowed for a given tenant key.
    ///
    /// Each tenant gets its own token bucket keyed by `tenant:{key}`.
    /// The `rps` and `burst` values come from the tenant's quota configuration
    /// (not the global rate-limit config).
    ///
    /// Returns an allowed [`CheckResult`] when the tenant bucket has tokens.
    #[allow(clippy::cast_precision_loss)] // Reason: precision loss is acceptable for rate-limit token calculations
    pub(super) async fn check_tenant_limit(
        &self,
        tenant_key: &str,
        rps: u32,
        burst: u32,
    ) -> CheckResult {
        let bucket_key = format!("tenant:{tenant_key}");

        // Best-effort capacity check; see `check_path_limit` for why this races safely.
        if !self.tenant_buckets.contains_key(&bucket_key)
            && self.tenant_buckets.len() >= self.config.max_buckets
        {
            debug!(
                tenant_key = tenant_key,
                "Tenant bucket capacity reached — denying unseen tenant"
            );
            return CheckResult::deny(1);
        }

        let (allowed, remaining) = {
            let mut bucket_ref = self
                .tenant_buckets
                .entry(bucket_key)
                .or_insert_with(|| TokenBucket::new(f64::from(burst), f64::from(rps)));
            let bucket = bucket_ref.value_mut();
            let allowed = bucket.try_consume(1.0);
            let remaining = bucket.token_count();
            (allowed, remaining)
        };

        if allowed {
            CheckResult::allow(remaining)
        } else {
            debug!(tenant_key = tenant_key, "Per-tenant rate limit exceeded");
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // Reason: ceil(1/rps) is always a small positive integer
            let retry = if rps == 0 {
                1
            } else {
                ((1.0_f64 / f64::from(rps)).ceil() as u32).max(1)
            };
            CheckResult::deny(retry)
        }
    }

    /// Evict stale in-memory buckets.
    ///
    /// Called by the sweep `Server::spawn_rate_limit_cleanup` puts on the server's
    /// `JoinSet` at `cleanup_interval_secs` (#1080). That parenthetical used to say
    /// "(called by background cleanup task)" while **no such task existed** — the
    /// function was correct, complete, and reachable only from tests, so `max_buckets`
    /// was a permanent lockout rather than a cap. It is true now; if the spawn is ever
    /// removed, this sentence is the thing to delete with it.
    ///
    /// A bucket is stale once it has been idle for longer than the time required
    /// to fully refill from empty (`burst_size / rps_per_ip`).  At that point the
    /// next request would start a fresh full bucket anyway, so the entry is safe
    /// to remove.
    #[allow(clippy::cast_precision_loss)] // Reason: precision loss is acceptable for rate-limit cleanup interval calculations
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

        let before_ip = self.ip_buckets.len();
        self.ip_buckets.retain(|_, b| b.last_refill >= ip_threshold);
        let evicted_ip = before_ip.saturating_sub(self.ip_buckets.len());

        let before_user = self.user_buckets.len();
        self.user_buckets.retain(|_, b| b.last_refill >= user_threshold);
        let evicted_user = before_user.saturating_sub(self.user_buckets.len());

        self.path_ip_buckets.retain(|_, b| b.last_refill >= ip_threshold);
        self.tenant_buckets.retain(|_, b| b.last_refill >= ip_threshold);

        debug!(evicted_ip, evicted_user, "Rate limiter cleanup complete");
    }

    /// Live buckets across all four maps.
    ///
    /// Summed rather than reported per-map: the caller outside this module is asking
    /// "did the sweep run", and any map shrinking answers it. The per-map view stays
    /// with the unit tests, which can read the maps directly.
    pub(super) fn live_bucket_count(&self) -> usize {
        self.ip_buckets.len()
            + self.user_buckets.len()
            + self.path_ip_buckets.len()
            + self.tenant_buckets.len()
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
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                // Reason: ceil(1/tokens_per_sec) is always a small positive integer
                return ((1.0_f64 / rule.tokens_per_sec).ceil() as u32).max(1);
            }
        }
        1
    }
}
