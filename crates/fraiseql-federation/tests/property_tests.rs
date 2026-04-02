//! Property-based tests for fraiseql-federation security boundaries.
//!
//! Covers:
//! - SSRF URL validation (`validate_subgraph_url`, `is_ssrf_blocked_ip`)
//! - Entity batch-size guard (`parse_representations`)
//! - URL parsing stability (no panics on arbitrary input)

#![allow(clippy::doc_markdown)] // Reason: doc comments in test helpers are informal
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use fraiseql_federation::{
    is_ssrf_blocked_ip, parse_representations, types::FederationMetadata, validate_subgraph_url,
};
use proptest::prelude::*;
use serde_json::{Value, json};

// ── SSRF IP blocking ────────────────────────────────────────────────────────

/// Generate an arbitrary private IPv4 from RFC 1918 10.0.0.0/8.
fn arb_rfc1918_10() -> impl Strategy<Value = Ipv4Addr> {
    (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(b, c, d)| Ipv4Addr::new(10, b, c, d))
}

/// Generate an arbitrary private IPv4 from RFC 1918 172.16.0.0/12.
fn arb_rfc1918_172() -> impl Strategy<Value = Ipv4Addr> {
    (16..=31u8, any::<u8>(), any::<u8>()).prop_map(|(b, c, d)| Ipv4Addr::new(172, b, c, d))
}

/// Generate an arbitrary private IPv4 from RFC 1918 192.168.0.0/16.
fn arb_rfc1918_192() -> impl Strategy<Value = Ipv4Addr> {
    (any::<u8>(), any::<u8>()).prop_map(|(c, d)| Ipv4Addr::new(192, 168, c, d))
}

/// Generate an arbitrary loopback IPv4 from 127.0.0.0/8.
fn arb_loopback_v4() -> impl Strategy<Value = Ipv4Addr> {
    (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(b, c, d)| Ipv4Addr::new(127, b, c, d))
}

/// Generate an arbitrary link-local IPv4 from 169.254.0.0/16.
fn arb_link_local_v4() -> impl Strategy<Value = Ipv4Addr> {
    (any::<u8>(), any::<u8>()).prop_map(|(c, d)| Ipv4Addr::new(169, 254, c, d))
}

/// Generate an arbitrary CGNAT IPv4 from 100.64.0.0/10.
fn arb_cgnat_v4() -> impl Strategy<Value = Ipv4Addr> {
    (64..=127u8, any::<u8>(), any::<u8>()).prop_map(|(b, c, d)| Ipv4Addr::new(100, b, c, d))
}

/// Generate a "known public" IPv4 that must NOT be blocked.
///
/// Uses the 8.x.x.x range (Google DNS lives here, and the entire /8 is public).
fn arb_public_v4() -> impl Strategy<Value = Ipv4Addr> {
    (1..=254u8, 1..=254u8, 1..=254u8).prop_map(|(b, c, d)| Ipv4Addr::new(8, b, c, d))
}

proptest! {
    // ── Private ranges must always be blocked ────────────────────────────

    #[test]
    fn ssrf_blocks_rfc1918_10(ip in arb_rfc1918_10()) {
        prop_assert!(is_ssrf_blocked_ip(&IpAddr::V4(ip)),
            "10.x.x.x must be blocked: {ip}");
    }

    #[test]
    fn ssrf_blocks_rfc1918_172(ip in arb_rfc1918_172()) {
        prop_assert!(is_ssrf_blocked_ip(&IpAddr::V4(ip)),
            "172.16-31.x.x must be blocked: {ip}");
    }

    #[test]
    fn ssrf_blocks_rfc1918_192(ip in arb_rfc1918_192()) {
        prop_assert!(is_ssrf_blocked_ip(&IpAddr::V4(ip)),
            "192.168.x.x must be blocked: {ip}");
    }

    #[test]
    fn ssrf_blocks_loopback_v4(ip in arb_loopback_v4()) {
        prop_assert!(is_ssrf_blocked_ip(&IpAddr::V4(ip)),
            "127.x.x.x must be blocked: {ip}");
    }

    #[test]
    fn ssrf_blocks_link_local_v4(ip in arb_link_local_v4()) {
        prop_assert!(is_ssrf_blocked_ip(&IpAddr::V4(ip)),
            "169.254.x.x must be blocked: {ip}");
    }

    #[test]
    fn ssrf_blocks_cgnat_v4(ip in arb_cgnat_v4()) {
        prop_assert!(is_ssrf_blocked_ip(&IpAddr::V4(ip)),
            "100.64-127.x.x must be blocked: {ip}");
    }

    // ── Public IPs must NOT be blocked ──────────────────────────────────

    #[test]
    fn ssrf_allows_public_v4(ip in arb_public_v4()) {
        prop_assert!(!is_ssrf_blocked_ip(&IpAddr::V4(ip)),
            "8.x.x.x must be allowed: {ip}");
    }

    // ── IPv6 special addresses ──────────────────────────────────────────

    #[test]
    fn ssrf_blocks_ipv6_ula(
        seg1 in 0xfc00u16..=0xfdffu16,
        seg2 in any::<u16>(),
        seg3 in any::<u16>(),
        seg4 in any::<u16>(),
    ) {
        let ip = Ipv6Addr::new(seg1, seg2, seg3, seg4, 0, 0, 0, 1);
        prop_assert!(is_ssrf_blocked_ip(&IpAddr::V6(ip)),
            "ULA (fc00::/7) must be blocked: {ip}");
    }

    #[test]
    fn ssrf_blocks_ipv6_link_local(
        suffix in any::<u16>(),
    ) {
        // fe80::/10 — link-local
        let ip = Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, suffix);
        prop_assert!(is_ssrf_blocked_ip(&IpAddr::V6(ip)),
            "link-local (fe80::/10) must be blocked: {ip}");
    }

    // ── validate_subgraph_url: arbitrary URL input must not panic ────────

    #[test]
    fn validate_url_never_panics(input in "\\PC{0,256}") {
        // We don't care about the result — only that it does not panic.
        let _ = validate_subgraph_url(&input);
    }

    // ── validate_subgraph_url: bracketed IPv6 URLs must not panic ───────

    #[test]
    fn validate_url_bracketed_ipv6_no_panic(
        seg1 in any::<u16>(),
        seg2 in any::<u16>(),
        seg3 in any::<u16>(),
        seg4 in any::<u16>(),
    ) {
        let url = format!(
            "https://[{seg1:x}:{seg2:x}:{seg3:x}:{seg4:x}::1]/graphql"
        );
        let _ = validate_subgraph_url(&url);
    }

    // ── validate_subgraph_url: private IPs via URL are rejected ─────────

    #[test]
    fn validate_url_rejects_rfc1918_10(ip in arb_rfc1918_10()) {
        let url = format!("https://{ip}/graphql");
        prop_assert!(validate_subgraph_url(&url).is_err(),
            "URL with 10.x.x.x must be rejected: {url}");
    }

    #[test]
    fn validate_url_rejects_rfc1918_172(ip in arb_rfc1918_172()) {
        let url = format!("https://{ip}/graphql");
        prop_assert!(validate_subgraph_url(&url).is_err(),
            "URL with 172.16-31.x.x must be rejected: {url}");
    }

    #[test]
    fn validate_url_rejects_rfc1918_192(ip in arb_rfc1918_192()) {
        let url = format!("https://{ip}/graphql");
        prop_assert!(validate_subgraph_url(&url).is_err(),
            "URL with 192.168.x.x must be rejected: {url}");
    }

    #[test]
    fn validate_url_allows_public_v4(ip in arb_public_v4()) {
        let url = format!("https://{ip}/graphql");
        prop_assert!(validate_subgraph_url(&url).is_ok(),
            "URL with 8.x.x.x must be allowed: {url}");
    }
}

// ── Entity batch-size guard ─────────────────────────────────────────────────

/// The constant is not exported; mirror it here for property tests.
const MAX_ENTITIES_BATCH_SIZE: usize = 1_000;

proptest! {
    #[test]
    fn batch_size_accepted_up_to_max(count in 0..=MAX_ENTITIES_BATCH_SIZE) {
        let items: Vec<Value> = (0..count)
            .map(|i| json!({"__typename": "User", "id": i.to_string()}))
            .collect();
        let input = Value::Array(items);
        let metadata = FederationMetadata::default();
        let result = parse_representations(&input, &metadata);
        prop_assert!(result.is_ok(),
            "batch of {count} must be accepted (max {MAX_ENTITIES_BATCH_SIZE})");
    }

    #[test]
    fn batch_size_rejected_above_max(extra in 1..500usize) {
        let count = MAX_ENTITIES_BATCH_SIZE + extra;
        let items: Vec<Value> = (0..count)
            .map(|i| json!({"__typename": "User", "id": i.to_string()}))
            .collect();
        let input = Value::Array(items);
        let metadata = FederationMetadata::default();
        let result = parse_representations(&input, &metadata);
        prop_assert!(result.is_err(),
            "batch of {count} must be rejected (max {MAX_ENTITIES_BATCH_SIZE})");
    }
}

// ── parse_representations stability ─────────────────────────────────────────

proptest! {
    #[test]
    fn parse_representations_never_panics_on_arbitrary_json(
        // Generate an arbitrary JSON value (limited depth).
        input in prop::collection::vec(
            prop::string::string_regex("[a-zA-Z_][a-zA-Z0-9_]{0,15}").unwrap(),
            0..5
        )
    ) {
        // Build a JSON array of objects with random field names.
        let items: Vec<Value> = input.iter()
            .map(|name| json!({"__typename": name, "id": "1"}))
            .collect();
        let val = Value::Array(items);
        let metadata = FederationMetadata::default();
        let _ = parse_representations(&val, &metadata);
    }
}
