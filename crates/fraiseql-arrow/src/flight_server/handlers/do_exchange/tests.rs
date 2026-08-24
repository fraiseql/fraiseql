#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

//! Unit tests for the `do_exchange` request handlers.
//!
//! `Subscribe` had no test of any kind before #1067 — the placebo suite named for
//! `do_exchange` asserted a `register_defaults()` constant — which is how an
//! acknowledgement that was false in every shipped configuration survived.

use tokio::sync::mpsc;

use super::handle_subscribe;
use crate::exchange_protocol::ExchangeMessage;

/// Drive `handle_subscribe` and decode the single message it sends.
async fn subscribe_response(correlation_id: &str, entity_type: &str) -> ExchangeMessage {
    let (tx, mut rx) = mpsc::channel(4);

    handle_subscribe(tx, correlation_id.to_string(), entity_type.to_string()).await;

    let flight_data = rx
        .recv()
        .await
        .expect("handle_subscribe must send exactly one response")
        .expect("the response must not be a stream error");

    ExchangeMessage::from_json_bytes(&flight_data.app_metadata)
        .expect("the response must be a decodable ExchangeMessage")
}

/// #1067 — `Subscribe` must refuse, not acknowledge.
///
/// Nothing in the workspace calls `SubscriptionManager::broadcast_event`, so the
/// old `Ok("Subscribed to {entity_type}")` was false in every configuration this
/// repository produces: the client waited indefinitely for events no code path
/// can emit.
#[tokio::test]
async fn subscribe_is_refused_rather_than_acknowledged() {
    let response = subscribe_response("corr-1", "Order").await;

    match response {
        ExchangeMessage::Response {
            correlation_id,
            result,
        } => {
            assert_eq!(correlation_id, "corr-1", "the refusal must match the request");
            let message = result.expect_err("Subscribe must not report success");
            assert!(
                message.contains("not implemented"),
                "the refusal must say the surface is unimplemented, got: {message}"
            );
        },
        other => panic!("expected a Response, got {other:?}"),
    }
}

/// The refusal must never carry the old success payload, whatever else it says.
#[tokio::test]
async fn subscribe_response_does_not_claim_a_subscription() {
    let response = subscribe_response("corr-2", "Order").await;

    let ExchangeMessage::Response { result, .. } = response else {
        panic!("expected a Response");
    };
    let message = result.expect_err("Subscribe must not report success");
    assert!(
        !message.contains("Subscribed to"),
        "the refusal must not read as an acknowledgement, got: {message}"
    );
}

/// Exactly one message, and no forwarder task left behind holding the sender.
///
/// The old handler spawned a task that blocked on `recv()` forever, keeping a
/// task, a channel and a `DashMap` entry alive for the process lifetime — one per
/// distinct client-chosen `correlation_id`. If anything still held a sender clone,
/// the channel would not close here.
#[tokio::test]
async fn subscribe_sends_one_message_and_closes_the_channel() {
    let (tx, mut rx) = mpsc::channel(4);

    handle_subscribe(tx, "corr-3".to_string(), "Order".to_string()).await;

    assert!(rx.recv().await.is_some(), "the refusal must be sent");
    assert!(
        rx.recv().await.is_none(),
        "no sender may outlive the call — a leaked forwarder task would hold one"
    );
}
