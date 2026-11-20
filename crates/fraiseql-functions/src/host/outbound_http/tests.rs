//! The client a guest's request is sent with goes where the checked URL says and nowhere
//! else.
#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code; panics surface failures.

use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use fraiseql_guard::net::pinned::{Lookup, PinnedResolver};
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::any};

use super::outbound_client;
use crate::host::live::HostContextConfig;

async fn accepting() -> MockServer {
    let receiver = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&receiver)
        .await;
    receiver
}

/// A 307 is handed back as the answer; the address it names gets nothing.
#[tokio::test]
async fn a_redirect_is_returned_not_followed() {
    let target = accepting().await;
    let receiver = MockServer::start().await;
    Mock::given(any())
        .respond_with(
            ResponseTemplate::new(307)
                .insert_header("Location", format!("{}/internal", target.uri())),
        )
        .mount(&receiver)
        .await;
    let client = outbound_client(&HostContextConfig::default(), PinnedResolver::new()).unwrap();

    let answer = client.get(format!("{}/api", receiver.uri())).send().await.unwrap();

    assert_eq!(answer.status().as_u16(), 307);
    assert!(
        target.received_requests().await.unwrap().is_empty(),
        "the redirect was followed"
    );
}

/// A lookup that answers `addr` for every name, counting the lookups: a DNS record
/// pointing wherever the test says, as a record changed after the URL was checked would.
fn answering(addr: SocketAddr, count: Arc<AtomicUsize>) -> Lookup {
    Arc::new(move |_, _| {
        count.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move { Ok(vec![addr]) })
    })
}

/// The client looks the name up itself, through the checking resolver: a loopback answer
/// is refused at connect, and the receiver there gets nothing.
#[tokio::test]
async fn the_client_does_not_connect_to_a_blocked_address_its_lookup_answers() {
    let receiver = accepting().await;
    let lookups = Arc::new(AtomicUsize::new(0));
    let resolver =
        PinnedResolver::new().with_lookup(answering(*receiver.address(), Arc::clone(&lookups)));
    let client = outbound_client(&HostContextConfig::default(), resolver).unwrap();

    let sent = client
        .get(format!("http://api.example:{}/v1", receiver.address().port()))
        .send()
        .await;

    assert!(sent.is_err(), "{sent:?}");
    assert_eq!(lookups.load(Ordering::SeqCst), 1, "the client's lookup is the resolver's");
    assert!(receiver.received_requests().await.unwrap().is_empty());
}

/// A proxy in the environment is not used: through one, the client's lookup would name
/// the proxy and the proxy would resolve the destination unchecked. The variables are set
/// only while the client is built, which is when they are read.
#[tokio::test]
async fn a_request_does_not_go_through_an_environment_proxy() {
    let receiver = accepting().await;
    let proxy = accepting().await;
    let proxy_uri = proxy.uri();
    let client = temp_env::with_vars(
        [
            ("HTTP_PROXY", Some(proxy_uri.as_str())),
            ("http_proxy", Some(proxy_uri.as_str())),
            ("ALL_PROXY", Some(proxy_uri.as_str())),
            ("NO_PROXY", None),
            ("no_proxy", None),
        ],
        || outbound_client(&HostContextConfig::default(), PinnedResolver::new()).unwrap(),
    );

    client.get(format!("{}/v1", receiver.uri())).send().await.unwrap();

    assert_eq!(receiver.received_requests().await.unwrap().len(), 1);
    assert!(proxy.received_requests().await.unwrap().is_empty(), "the proxy was used");
}
