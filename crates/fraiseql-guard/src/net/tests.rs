//! The corpus, asserted against the guard itself.
//!
//! Dependent crates run the same tables against their own entry points; these
//! assert the predicate underneath them.

#![allow(clippy::panic)] // Reason: test code; a malformed corpus entry should abort loudly.

use std::net::IpAddr;

use super::{
    BlockedReason, blocked_host_reason, is_blocked_ip,
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
