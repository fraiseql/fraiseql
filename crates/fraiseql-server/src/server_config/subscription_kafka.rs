//! `[subscription_kafka]` — mirroring subscription deliveries to Kafka (#1102).
//!
//! `fraiseql_core::runtime::subscription::KafkaAdapter` existed and nothing constructed
//! it. It was library-only surface: no config section, no route, no mount, which is why
//! no integration test covered it and why it went four releases setting no
//! `security.protocol` at all. This section is the missing consumer.
//!
//! # What this is, and what it is not
//!
//! It publishes **what live subscribers receive**: the manager broadcasts one payload
//! per matching active subscription, and each becomes one Kafka message carrying that
//! subscription's name. An event nobody is subscribed to produces nothing here.
//!
//! For a complete change stream that does not depend on who is connected — durable,
//! with an outbox, retries and a dead-letter state — the section is `[cdc_outbound]`
//! with `kind = "kafka"` (#975). The two are not alternatives: this one is a fan-out of
//! *subscription* deliveries and is **at-most-once**; that one is the change log and is
//! at-least-once.

use serde::{Deserialize, Serialize};

/// Configuration for the subscription Kafka mirror (`[subscription_kafka]`).
///
/// Presence of the section enables it; absence leaves it off. Strict
/// (`deny_unknown_fields`): an unrecognised key is a boot error.
///
/// # Example (TOML)
///
/// ```toml
/// [subscription_kafka]
/// endpoint = "kafka+ssl://broker.internal:9093"
/// default_topic = "fraiseql.subscriptions"
/// client_id = "fraiseql-subscriptions"
/// timeout_ms = 5000
/// compression = "lz4"
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SubscriptionKafkaConfig {
    /// The broker endpoint, **scheme first**: `kafka+ssl://`, `kafka+sasl-ssl://`, or
    /// `kafka://` for a development broker under `FRAISEQL_KAFKA_ALLOW_PLAINTEXT` plus a
    /// declared development environment.
    ///
    /// A scheme-less endpoint is refused at boot rather than defaulted. This transport
    /// carries entity after-images *and* pre-images, and librdkafka reads a bare
    /// `bootstrap.servers` list as `PLAINTEXT`.
    pub endpoint: String,

    /// The topic every payload is published to.
    pub default_topic: String,

    /// `client.id` for the producer.
    pub client_id: String,

    /// Delivery timeout in milliseconds.
    ///
    /// Short on purpose: this path is at-most-once with nothing behind it to retry, so a
    /// long timeout parks a delivery task rather than buying another attempt.
    pub timeout_ms: u64,

    /// `compression.type` (`"lz4"`, `"gzip"`, `"snappy"`, `"zstd"`), or unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compression: Option<String>,
}

impl Default for SubscriptionKafkaConfig {
    fn default() -> Self {
        Self {
            endpoint:      String::new(),
            default_topic: "fraiseql.subscriptions".to_owned(),
            client_id:     "fraiseql-subscriptions".to_owned(),
            timeout_ms:    5_000,
            compression:   None,
        }
    }
}

impl SubscriptionKafkaConfig {
    /// Reject a section that cannot describe a working transport.
    ///
    /// The endpoint *scheme* is not checked here — that is the guard's, at connect, so
    /// there is one place that decides it (#1102). What this catches is the section that
    /// is present but says nothing.
    ///
    /// # Errors
    ///
    /// Returns a description of the first problem found.
    pub fn validate(&self) -> Result<(), String> {
        if self.endpoint.trim().is_empty() {
            return Err("[subscription_kafka] endpoint is empty. Omit the section to \
                        disable the transport; an empty endpoint is a mistake, not a \
                        way to switch it off."
                .to_owned());
        }
        if self.default_topic.trim().is_empty() {
            return Err("[subscription_kafka] default_topic is empty.".to_owned());
        }
        if self.timeout_ms == 0 {
            return Err("[subscription_kafka] timeout_ms must be at least 1.".to_owned());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
