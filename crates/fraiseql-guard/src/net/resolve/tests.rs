#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::net::{IpAddr, SocketAddr};

use super::{Refusal, check_resolved, resolve_and_check};
use crate::net::BlockedReason;

#[tokio::test]
async fn a_loopback_alias_is_refused_before_any_lookup() {
    for host in ["localhost", "api.localhost", "localhost.evil.example"] {
        assert_eq!(
            resolve_and_check(host, 443).await,
            Err(Refusal::Host(BlockedReason::LoopbackHostname)),
            "{host}"
        );
    }
}

#[tokio::test]
async fn a_metadata_alias_is_refused_before_any_lookup() {
    assert_eq!(
        resolve_and_check("metadata.google.internal", 80).await,
        Err(Refusal::Host(BlockedReason::MetadataHostname))
    );
}

#[tokio::test]
async fn a_blocked_literal_is_refused() {
    for host in ["127.0.0.1", "169.254.169.254", "[::1]", "[::ffff:10.0.0.1]"] {
        assert_eq!(
            resolve_and_check(host, 443).await,
            Err(Refusal::Host(BlockedReason::ReservedAddress)),
            "{host}"
        );
    }
}

#[tokio::test]
async fn an_allowed_literal_is_returned_without_a_lookup() {
    let ip: IpAddr = "8.8.8.8".parse().unwrap();
    assert_eq!(resolve_and_check("8.8.8.8", 443).await, Ok(vec![SocketAddr::new(ip, 443)]));
    let v6: IpAddr = "2001:4860:4860::8888".parse().unwrap();
    assert_eq!(
        resolve_and_check("[2001:4860:4860::8888]", 443).await,
        Ok(vec![SocketAddr::new(v6, 443)])
    );
}

/// `.invalid` is reserved never to resolve (RFC 6761), so this needs no network.
#[tokio::test]
async fn a_name_that_does_not_resolve_is_refused() {
    let err = resolve_and_check("fraiseql-guard-test.invalid", 443).await.unwrap_err();
    assert!(matches!(err, Refusal::ResolutionFailed(_) | Refusal::NoAddresses), "{err:?}");
}

fn addr(s: &str) -> SocketAddr {
    SocketAddr::new(s.parse().unwrap(), 443)
}

/// A rebinding name answers with a public address and a private one; the whole
/// resolution is refused, naming the private address.
#[test]
fn any_blocked_address_in_a_resolution_refuses_it() {
    assert_eq!(
        check_resolved(vec![addr("8.8.8.8"), addr("10.0.0.5")]),
        Err(Refusal::BlockedAddress("10.0.0.5".parse().unwrap()))
    );
    assert_eq!(
        check_resolved(vec![addr("::ffff:169.254.169.254")]),
        Err(Refusal::BlockedAddress("::ffff:169.254.169.254".parse().unwrap()))
    );
}

#[test]
fn an_empty_resolution_is_refused() {
    assert_eq!(check_resolved(Vec::new()), Err(Refusal::NoAddresses));
}

#[test]
fn a_public_resolution_is_returned_whole() {
    let addrs = vec![addr("8.8.8.8"), addr("2001:4860:4860::8888")];
    assert_eq!(check_resolved(addrs.clone()), Ok(addrs));
}
