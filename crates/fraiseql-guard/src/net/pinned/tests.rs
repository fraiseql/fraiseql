#![allow(clippy::unwrap_used)] // Reason: test code

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use reqwest::dns::{Name, Resolve};

use super::{Lookup, PinnedResolver};

/// A lookup answering `ip` for every name, counting the lookups.
fn answering(ip: IpAddr, count: Arc<AtomicUsize>) -> Lookup {
    Arc::new(move |_, port| {
        count.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move { Ok(vec![SocketAddr::new(ip, port)]) })
    })
}

async fn resolve(resolver: &PinnedResolver, name: &str) -> Result<Vec<IpAddr>, String> {
    resolver
        .resolve(Name::from_str(name).unwrap())
        .await
        .map(|addrs| addrs.map(|addr| addr.ip()).collect())
        .map_err(|e| e.to_string())
}

const PUBLIC: IpAddr = IpAddr::V4(Ipv4Addr::new(93, 184, 215, 14));
const LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const METADATA: IpAddr = IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254));

#[tokio::test]
async fn a_public_answer_is_the_answer() {
    let resolver = PinnedResolver::new().with_lookup(answering(PUBLIC, Arc::default()));

    assert_eq!(resolve(&resolver, "receiver.example").await, Ok(vec![PUBLIC]));
}

#[tokio::test]
async fn a_blocked_answer_is_refused() {
    for blocked in [LOOPBACK, METADATA, IpAddr::V4(Ipv4Addr::new(10, 0, 0, 7))] {
        let resolver = PinnedResolver::new().with_lookup(answering(blocked, Arc::default()));

        let refused = resolve(&resolver, "receiver.example").await.unwrap_err();
        assert!(refused.contains(&blocked.to_string()), "{blocked}: {refused}");
    }
}

#[tokio::test]
async fn a_blocked_name_is_refused_before_any_lookup() {
    let count = Arc::new(AtomicUsize::new(0));
    let resolver = PinnedResolver::new().with_lookup(answering(PUBLIC, Arc::clone(&count)));

    assert!(resolve(&resolver, "localhost").await.is_err());
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn an_unchecked_resolver_answers_as_the_lookup_does() {
    let resolver = PinnedResolver::new()
        .with_lookup(answering(LOOPBACK, Arc::default()))
        .unchecked_when(|| true);

    assert_eq!(resolve(&resolver, "receiver.example").await, Ok(vec![LOOPBACK]));
}
