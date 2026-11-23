//! The expiry a Vault lease sets, and how long the cache serves it: a lease is a number
//! of seconds from the response, so any `i64`, negative and past every calendar included.
#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code, panics acceptable

use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

use super::VaultBackend;
use crate::secrets_manager::SecretsBackend;

type Instant = fraiseql_time::Timestamp;

fn now() -> Instant {
    fraiseql_time::Timestamp::now()
}

fn plus_seconds(at: Instant, seconds: i64) -> Instant {
    at + fraiseql_time::jiff::SignedDuration::from_secs(seconds)
}

/// The last day any expiry can fall on.
fn last_day() -> Instant {
    "9999-12-30T00:00:00Z".parse().unwrap()
}

/// A Vault answering every read with `lease_duration: lease`, expecting `reads` of them.
async fn vault(lease: i64, reads: u64) -> (VaultBackend, MockServer) {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "request_id": "r",
            "lease_id": "",
            "lease_duration": lease,
            "renewable": false,
            "data": {"value": "v"}
        })))
        .expect(reads)
        .mount(&mock)
        .await;
    (VaultBackend::new_for_test(mock.uri(), "t"), mock)
}

async fn expiry(vault: &VaultBackend) -> Instant {
    vault.get_secret_with_expiry("secret/x").await.expect("the read succeeds").1
}

/// A lease of `n` seconds expires `n` seconds after the read, `0` at it and a negative
/// lease before it.
#[tokio::test]
async fn a_lease_expires_that_many_seconds_after_the_read() {
    for lease in [3600, 0, -3600] {
        let (vault, _mock) = vault(lease, 1).await;
        let before = now();
        let expires = expiry(&vault).await;
        let after = now();
        assert!(
            plus_seconds(before, lease) <= expires && expires <= plus_seconds(after, lease),
            "{lease}: {expires:?}"
        );
    }
}

/// A lease reaching past the last representable day never expires within it, up to the
/// largest lease a response can carry.
#[tokio::test]
async fn a_lease_past_the_calendar_never_expires_within_it() {
    for lease in [400_000_000_000, 9_223_372_036_854_775, i64::MAX] {
        let (vault, _mock) = vault(lease, 1).await;
        let expires = expiry(&vault).await;
        assert!(expires >= last_day(), "{lease}: {expires:?}");
    }
}

/// A read the cache serves reports the expiry the lease set, as the read that fetched it
/// did: one secret has one expiry, however it is read. Vault is asked once.
#[tokio::test]
async fn a_cached_read_reports_the_expiry_the_fresh_read_did() {
    for lease in [3600, 400_000_000_000, i64::MAX] {
        let (backend, mock) = vault(lease, 1).await;
        let fresh = expiry(&backend).await;
        let cached = expiry(&backend).await;
        assert_eq!(cached, fresh, "{lease}");
        mock.verify().await;
    }
}

/// A lease that has run out by the read is not cached: the next read asks Vault again.
#[tokio::test]
async fn a_lapsed_lease_is_read_again() {
    for lease in [0, -3600, i64::MIN] {
        let (vault, mock) = vault(lease, 2).await;
        expiry(&vault).await;
        expiry(&vault).await;
        mock.verify().await;
    }
}

/// The cache serves an entry until its refresh point, reporting the lease's expiry, and
/// not after it, whatever the lease still has to run.
#[tokio::test]
async fn the_cache_serves_until_its_refresh_point_and_reports_the_lease() {
    use super::super::cache::SecretCache;
    use crate::secrets_manager::Secret;

    let cache = SecretCache::new(10);
    let at = now();
    let (refresh, lease) = (plus_seconds(at, 2880), plus_seconds(at, 3600));
    cache
        .set("served".to_string(), Secret::new("v".to_string()), refresh, lease)
        .await;
    let (_, reported) = cache.get_with_expiry("served").await.expect("before its refresh point");
    assert_eq!(reported, lease);

    let (refresh, lease) = (plus_seconds(at, -1), plus_seconds(at, 3600));
    cache.set("due".to_string(), Secret::new("v".to_string()), refresh, lease).await;
    assert!(cache.get_with_expiry("due").await.is_none(), "past its refresh point");
}

/// The cache refreshes a lease once 80% of it has run, before it expires: a 2-second
/// lease is read from Vault again 1.1 seconds later.
#[tokio::test]
async fn a_lease_is_read_again_once_80_percent_of_it_has_run() {
    let (backend, mock) = vault(2, 2).await;
    let first = expiry(&backend).await;
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    assert!(now() < first, "the lease has not expired yet");
    expiry(&backend).await;
    mock.verify().await;
}
