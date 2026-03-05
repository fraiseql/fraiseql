//! Property-based tests for rate-limit key construction.
//!
//! Verifies that `build_rate_limit_key` is panic-free on arbitrary inputs,
//! produces deterministic output, and embeds the strategy prefix in every key.

use fraiseql_server::middleware::rate_limit::build_rate_limit_key;
use proptest::prelude::*;

proptest! {
    /// Key construction must never panic regardless of input content.
    #[test]
    fn rate_limit_ip_key_never_panics(ip in "\\PC*") {
        let _ = build_rate_limit_key("ip", &ip, None);
    }

    /// Key construction with user identifier must never panic.
    #[test]
    fn rate_limit_user_key_never_panics(user_id in "\\PC*") {
        let _ = build_rate_limit_key("user", &user_id, None);
    }

    /// Key construction with an optional path prefix must never panic.
    #[test]
    fn rate_limit_path_key_never_panics(ip in "\\PC*", prefix in "\\PC*") {
        let _ = build_rate_limit_key("path", &ip, Some(&prefix));
    }

    /// Every key must start with the namespaced strategy prefix so different
    /// strategies never collide in Redis.
    #[test]
    fn rate_limit_key_contains_strategy_prefix(ip in "[0-9a-f:.]{1,40}") {
        let key = build_rate_limit_key("ip", &ip, None);
        prop_assert!(
            key.starts_with("fraiseql:rl:ip:"),
            "key {key:?} must start with 'fraiseql:rl:ip:'"
        );
    }

    /// Identical inputs must always produce the same key (no random components).
    #[test]
    fn rate_limit_key_is_deterministic(ip in "[0-9a-f:.]{1,40}") {
        let k1 = build_rate_limit_key("ip", &ip, None);
        let k2 = build_rate_limit_key("ip", &ip, None);
        prop_assert_eq!(k1, k2);
    }

    /// Keys for different strategies must not be equal for the same identifier.
    #[test]
    fn rate_limit_different_strategies_produce_different_keys(id in "[a-z0-9]{1,20}") {
        let ip_key = build_rate_limit_key("ip", &id, None);
        let user_key = build_rate_limit_key("user", &id, None);
        prop_assert_ne!(
            ip_key,
            user_key,
            "ip and user keys for the same id must differ"
        );
    }

    /// A key built with a prefix must contain both the prefix and identifier.
    #[test]
    fn rate_limit_path_key_embeds_prefix(
        ip in "[0-9]{1,15}",
        prefix in "[a-z/]{1,20}"
    ) {
        let key = build_rate_limit_key("path", &ip, Some(&prefix));
        prop_assert!(
            key.contains(&prefix),
            "key {key:?} must embed prefix {prefix:?}"
        );
        prop_assert!(
            key.contains(&ip),
            "key {key:?} must embed identifier {ip:?}"
        );
    }
}
