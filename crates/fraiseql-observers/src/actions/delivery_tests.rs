//! The delivery client goes where the checked URL says and nowhere else: it follows no
//! redirect, and it connects only to an address its own lookup checked.
#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code; panics surface failures.

use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use fraiseql_guard::net::pinned::{Lookup, PinnedResolver};
use uuid::Uuid;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

use super::{SlackAction, WEBHOOK_SIGNATURE_HEADER, WebhookAction, delivery_client_with};
use crate::{
    event::{EntityEvent, EventKind},
    insecure_guard::test_override,
};

fn event() -> EntityEvent {
    EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        Uuid::new_v4(),
        serde_json::json!({ "id": "ord_1" }),
    )
}

/// A receiver answering every delivery with a 307 to `target`.
async fn redirecting_to(target: &str) -> MockServer {
    let receiver = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(307).insert_header("Location", target))
        .mount(&receiver)
        .await;
    receiver
}

async fn accepting() -> MockServer {
    let receiver = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&receiver)
        .await;
    receiver
}

/// A 307 is the receiver's answer: the delivery fails and nothing reaches the address it
/// names, neither the body nor the credential and signature headers. The checks are off
/// so that the first receiver, on loopback, is reachable at all.
#[tokio::test]
async fn a_webhook_delivery_does_not_follow_a_redirect() {
    let target = accepting().await;
    let receiver = redirecting_to(&format!("{}/internal", target.uri())).await;
    let _local_receivers = test_override::force(true);
    let headers = HashMap::from([("X-Token".to_string(), "credential".to_string())]);

    let delivered = WebhookAction::new()
        .execute(
            &format!("{}/hook", receiver.uri()),
            None,
            &headers,
            None,
            Some("s3cret"),
            &event(),
        )
        .await;

    assert!(delivered.is_err(), "{delivered:?}");
    assert_eq!(receiver.received_requests().await.unwrap().len(), 1);
    let sent = &receiver.received_requests().await.unwrap()[0];
    assert!(
        sent.headers.get(WEBHOOK_SIGNATURE_HEADER).is_some(),
        "the receiver got it signed"
    );
    assert!(
        target.received_requests().await.unwrap().is_empty(),
        "the redirect was followed"
    );
}

/// The same for a webhook built with its own timeout.
#[tokio::test]
async fn a_webhook_delivery_with_a_timeout_does_not_follow_a_redirect() {
    let target = accepting().await;
    let receiver = redirecting_to(&format!("{}/internal", target.uri())).await;
    let _local_receivers = test_override::force(true);

    let delivered = WebhookAction::with_timeout(Duration::from_secs(5))
        .execute(&format!("{}/hook", receiver.uri()), None, &HashMap::new(), None, None, &event())
        .await;

    assert!(delivered.is_err(), "{delivered:?}");
    assert!(
        target.received_requests().await.unwrap().is_empty(),
        "the redirect was followed"
    );
}

/// The same for a Slack message.
#[tokio::test]
async fn a_slack_delivery_does_not_follow_a_redirect() {
    let target = accepting().await;
    let receiver = redirecting_to(&format!("{}/internal", target.uri())).await;
    let _local_receivers = test_override::force(true);

    let delivered = SlackAction::new()
        .execute(&format!("{}/hook", receiver.uri()), Some("#c"), Some("hi"), &event())
        .await;

    assert!(delivered.is_err(), "{delivered:?}");
    assert_eq!(receiver.received_requests().await.unwrap().len(), 1);
    assert!(
        target.received_requests().await.unwrap().is_empty(),
        "the redirect was followed"
    );
}

/// A lookup that answers `addr` for every name: a DNS record pointing wherever the test
/// says, as a record changed after the URL was checked would.
fn answering(addr: SocketAddr) -> Lookup {
    Arc::new(move |_, _| Box::pin(async move { Ok(vec![addr]) }))
}

/// With the checks on, a name the client's own lookup answers with a loopback address is
/// refused at connect, and the receiver there gets nothing.
#[tokio::test]
async fn the_client_does_not_connect_to_a_blocked_address_its_lookup_answers() {
    let receiver = accepting().await;
    let resolver = PinnedResolver::new().with_lookup(answering(*receiver.address()));
    let client = delivery_client_with(Duration::from_secs(5), resolver, "WebhookAction");
    let _checks_on = test_override::force(false);

    let sent = client
        .post(format!("http://receiver.example:{}/hook", receiver.address().port()))
        .send()
        .await;

    assert!(sent.is_err(), "{sent:?}");
    assert!(receiver.received_requests().await.unwrap().is_empty());
}

/// The lookup above is the one the client connects by: with the checks off, the same
/// name reaches the receiver at the address it answered.
#[tokio::test]
async fn the_client_connects_where_its_own_lookup_answers() {
    let receiver = accepting().await;
    let resolver = PinnedResolver::new().with_lookup(answering(*receiver.address()));
    let client = delivery_client_with(Duration::from_secs(5), resolver, "WebhookAction");
    let _local_receivers = test_override::force(true);

    let sent = client
        .post(format!("http://receiver.example:{}/hook", receiver.address().port()))
        .send()
        .await
        .expect("delivered");

    assert!(sent.status().is_success());
    assert_eq!(receiver.received_requests().await.unwrap().len(), 1);
}

/// A proxy in the environment is not used: through one, the client's lookup would name
/// the proxy and the proxy would resolve the destination unchecked. The variables are set
/// only while the client is built, which is when they are read.
#[tokio::test]
async fn a_delivery_does_not_go_through_an_environment_proxy() {
    let receiver = accepting().await;
    let proxy = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&proxy)
        .await;
    let proxy_uri = proxy.uri();
    let action = temp_env::with_vars(
        [
            ("HTTP_PROXY", Some(proxy_uri.as_str())),
            ("http_proxy", Some(proxy_uri.as_str())),
            ("ALL_PROXY", Some(proxy_uri.as_str())),
            ("NO_PROXY", None),
            ("no_proxy", None),
        ],
        WebhookAction::new,
    );
    let _local_receivers = test_override::force(true);

    action
        .execute(&format!("{}/hook", receiver.uri()), None, &HashMap::new(), None, None, &event())
        .await
        .expect("delivered");

    assert_eq!(receiver.received_requests().await.unwrap().len(), 1);
    assert!(proxy.received_requests().await.unwrap().is_empty(), "the proxy was used");
}
