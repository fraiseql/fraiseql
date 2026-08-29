//! The mount for `[subscription_kafka]` (#1102).
//!
//! `KafkaAdapter` was library-only surface: re-exported, constructible, and reachable
//! from no shipped configuration. That is why nobody noticed it set no
//! `security.protocol` — no integration test could exist for a transport no
//! configuration could start. This module is the consumer that makes it real.
//!
//! # Shape
//!
//! The subscription manager broadcasts one
//! [`fraiseql_core::runtime::SubscriptionPayload`] per *matching active subscription*.
//! This mirror consumes that channel and publishes each payload, so a Kafka message
//! corresponds to a delivery a subscriber received, carrying that subscription's name.
//!
//! **At-most-once, deliberately.** There is no outbox on this path: a failed publish is
//! logged and dropped rather than retried. The durable Kafka path is `[cdc_outbound]`
//! with `kind = "kafka"` (#975), which has the outbox, the retry schedule and the
//! dead-letter state — putting a second durability story here is the drift #1102 is
//! about.
//!
//! A lagged receiver is reported rather than passed over: `broadcast` drops the oldest
//! payloads when a consumer falls behind, and a mirror that silently skips events is
//! indistinguishable from one that is working.

use std::sync::Arc;

use fraiseql_core::runtime::{
    SubscriptionManager, TransportAdapter,
    subscription::{KafkaAdapter, KafkaConfig},
};
use tokio::sync::broadcast::error::RecvError;
use tracing::{error, info, warn};

use crate::server_config::SubscriptionKafkaConfig;

/// A connected mirror, built at boot and started with the rest of the server's tasks.
#[derive(Debug)]
pub struct SubscriptionKafkaMirror {
    adapter: Arc<KafkaAdapter>,
    topic:   String,
}

/// Build the mirror from `[subscription_kafka]`, or `None` when the section is absent.
///
/// Fails the boot rather than degrading. This transport carries entity after-images and
/// pre-images; a server that starts with an unguarded or absent producer, after an
/// operator asked for a guarded one, is the failure mode the whole issue is about.
///
/// # Errors
///
/// Returns a description of the refusal: an incomplete section, or an endpoint the
/// transport guard rejects (no scheme, unsupported scheme, unopted-in plaintext, a
/// blocked broker host, unresolvable SASL credentials).
pub fn build_mirror(
    config: Option<&SubscriptionKafkaConfig>,
) -> Result<Option<SubscriptionKafkaMirror>, String> {
    let Some(config) = config else {
        return Ok(None);
    };
    config.validate()?;

    let mut adapter_config = KafkaConfig::new(&config.endpoint, &config.default_topic)
        .with_client_id(&config.client_id)
        .with_timeout(config.timeout_ms);
    if let Some(ref compression) = config.compression {
        adapter_config = adapter_config.with_compression(compression);
    }

    let adapter =
        KafkaAdapter::new(adapter_config).map_err(|e| format!("[subscription_kafka] {e}"))?;

    info!(
        topic = %config.default_topic,
        "subscription Kafka mirror configured"
    );

    Ok(Some(SubscriptionKafkaMirror {
        adapter: Arc::new(adapter),
        topic:   config.default_topic.clone(),
    }))
}

/// Start the mirror on the server's task set, so graceful shutdown stops it.
pub fn spawn(
    mirror: SubscriptionKafkaMirror,
    manager: &Arc<SubscriptionManager>,
    tasks: &mut tokio::task::JoinSet<()>,
) {
    let mut events = manager.receiver();
    let SubscriptionKafkaMirror { adapter, topic } = mirror;

    tasks.spawn(async move {
        info!(topic = %topic, "subscription Kafka mirror started");
        loop {
            match events.recv().await {
                Ok(payload) => {
                    // The adapter logs the failure with topic, key and reason; at most
                    // once means the answer here is to carry on, not to retry.
                    let _ = adapter.deliver(&payload.event, &payload.subscription_name).await;
                },
                Err(RecvError::Lagged(missed)) => {
                    // Not a warning to be ignored: these payloads are gone. Reported so
                    // a mirror that is quietly skipping deliveries is distinguishable
                    // from one with nothing to send.
                    error!(
                        missed,
                        topic = %topic,
                        "subscription Kafka mirror fell behind; those deliveries were \
                         dropped by the broadcast channel and are not recoverable"
                    );
                },
                Err(RecvError::Closed) => {
                    warn!(topic = %topic, "subscription channel closed; Kafka mirror stopping");
                    break;
                },
            }
        }
    });
}

#[cfg(test)]
mod tests;
