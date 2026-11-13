//! The corpus, asserted against the guard itself.
//!
//! Dependent crates run the same tables against their own entry points; these
//! assert the predicate underneath them.

#![allow(clippy::panic)] // Reason: test code; a malformed corpus entry should abort loudly.

use std::net::IpAddr;

use super::{
    BlockedReason, blocked_host_reason, is_blocked_ip, is_loopback_host,
    vectors::{MUST_ALLOW, MUST_ALLOW_HOSTS, MUST_BLOCK, MUST_BLOCK_HOSTS},
};

#[test]
fn every_blocked_vector_is_blocked() {
    for (addr, why) in MUST_BLOCK {
        let ip: IpAddr = addr
            .parse()
            .unwrap_or_else(|e| panic!("corpus entry {addr} is not a valid address: {e}"));
        assert!(is_blocked_ip(&ip), "{addr} must be blocked ({why})");
    }
}

#[test]
fn every_allowed_vector_is_allowed() {
    for addr in MUST_ALLOW {
        let ip: IpAddr = addr
            .parse()
            .unwrap_or_else(|e| panic!("corpus entry {addr} is not a valid address: {e}"));
        assert!(
            !is_blocked_ip(&ip),
            "{addr} is an ordinary public address and must stay reachable"
        );
    }
}

#[test]
fn every_blocked_host_is_blocked() {
    for (host, why) in MUST_BLOCK_HOSTS {
        assert!(blocked_host_reason(host).is_some(), "{host} must be blocked ({why})");
    }
}

#[test]
fn every_allowed_host_is_allowed() {
    for host in MUST_ALLOW_HOSTS {
        assert_eq!(blocked_host_reason(host), None, "{host} must not be refused by the host guard");
    }
}

#[test]
fn mapped_and_nat64_report_the_embedded_address_reason() {
    // A mapped address is refused because of what it embeds, not because it is
    // mapped — otherwise `::ffff:8.8.8.8` would be refused too.
    assert_eq!(
        blocked_host_reason("[::ffff:169.254.169.254]"),
        Some(BlockedReason::ReservedAddress)
    );
    assert_eq!(blocked_host_reason("[::ffff:8.8.8.8]"), None);
    assert_eq!(
        blocked_host_reason("[64:ff9b::169.254.169.254]"),
        Some(BlockedReason::ReservedAddress)
    );
}

#[test]
fn the_corpus_has_no_duplicate_entries() {
    // A duplicate reads as coverage that is not there.
    let mut addrs: Vec<&str> = MUST_BLOCK.iter().map(|(a, _)| *a).collect();
    let before = addrs.len();
    addrs.sort_unstable();
    addrs.dedup();
    assert_eq!(before, addrs.len(), "MUST_BLOCK contains a duplicate address");
}

#[test]
fn the_two_corpora_do_not_overlap() {
    // Normalised through IpAddr so `::ffff:a9fe:a9fe` and `::ffff:169.254.169.254`
    // count as the same entry.
    let blocked: Vec<IpAddr> = MUST_BLOCK.iter().filter_map(|(a, _)| a.parse().ok()).collect();
    for addr in MUST_ALLOW {
        let ip: IpAddr = addr.parse().unwrap_or_else(|e| panic!("bad corpus entry {addr}: {e}"));
        assert!(!blocked.contains(&ip), "{addr} appears in both MUST_ALLOW and MUST_BLOCK");
    }
}

#[test]
fn only_the_reserved_localhost_names_and_loopback_literals_are_the_local_machine() {
    // RFC 6761 reserves `localhost` and every name under it, in the absolute form too:
    // the `url` crate keeps a trailing dot in `host_str()`.
    for host in [
        "localhost",
        "LOCALHOST",
        "localhost.",
        "api.localhost",
        "Api.LocalHost.",
        "127.0.0.1",
        "127.0.0.1:9092",
        "[::1]",
        "::1",
        "[::ffff:127.0.0.1]",
    ] {
        assert!(is_loopback_host(host), "{host} denotes the local machine");
    }
    // A name that merely begins with `localhost.` is in somebody's DNS zone.
    for host in [
        "localhost.example.com",
        "LOCALHOST.EXAMPLE.COM",
        "localhost.localdomain",
        "localhost..",
        "localhostage.com",
        "notlocalhost",
        "api.localhost.example.com",
        "example.com",
        "10.0.0.1",
        "[::ffff:8.8.8.8]",
    ] {
        assert!(!is_loopback_host(host), "{host} is not known to be the local machine");
    }
}

#[test]
fn of_the_blocked_loopback_names_only_the_reserved_ones_count_as_the_local_machine() {
    // The host guard refuses every `localhost.`-prefixed name, which is the safe side;
    // `is_loopback_host` admits, so it must not inherit that wider match.
    let admitted: Vec<&str> = MUST_BLOCK_HOSTS
        .iter()
        .map(|(host, _)| *host)
        .filter(|host| blocked_host_reason(host) == Some(BlockedReason::LoopbackHostname))
        .filter(|host| is_loopback_host(host))
        .collect();
    assert_eq!(admitted, ["localhost", "LOCALHOST", "api.localhost", "api.localhost."]);
}
