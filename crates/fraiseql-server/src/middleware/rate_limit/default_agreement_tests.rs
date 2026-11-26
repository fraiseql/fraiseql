//! The limiter's runtime default is the compiled policy's default.
//!
//! A server with no rate-limit section that an environment variable or CLI flag switches on
//! starts from [`RateLimitConfig::default`]; one whose `[security.rate_limiting]` sets only
//! `enabled = true` starts from [`RateLimitingSecurityConfig::DEFAULT`]. The two must
//! describe the same limiter, or the source that switched it on decides its budget.

use super::config::{DEFAULT_CLEANUP_INTERVAL_SECS, RateLimitConfig, RateLimitingSecurityConfig};

#[test]
fn the_runtime_default_is_the_policy_default() {
    let runtime = RateLimitConfig::default();
    let policy = RateLimitingSecurityConfig::DEFAULT;
    let from_policy = RateLimitConfig::from_security_config(&policy);

    assert_eq!(runtime.enabled, from_policy.enabled, "enabled");
    assert_eq!(runtime.rps_per_ip, from_policy.rps_per_ip, "rps_per_ip");
    assert_eq!(runtime.rps_per_user, from_policy.rps_per_user, "rps_per_user");
    assert_eq!(runtime.burst_size, policy.burst_size, "burst_size");
    assert_eq!(
        runtime.trust_proxy_headers, from_policy.trust_proxy_headers,
        "trust_proxy_headers"
    );
    assert_eq!(
        runtime.trusted_proxy_cidrs, from_policy.trusted_proxy_cidrs,
        "trusted_proxy_cidrs"
    );
    assert_eq!(runtime.max_buckets, from_policy.max_buckets, "max_buckets");
    assert_eq!(
        runtime.cleanup_interval_secs, DEFAULT_CLEANUP_INTERVAL_SECS,
        "cleanup_interval_secs"
    );
}
