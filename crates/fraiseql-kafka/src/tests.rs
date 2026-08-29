//! What can be asserted without a broker: the guard runs before any client is built,
//! the classification, and the timeout contract.
//!
//! Producing itself needs a real broker and is covered by the CDC sink's Dagger suite
//! and the subscription transport's; the point of the tests here is that a *refusal*
//! never reaches a producer at all.

#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code.

use std::time::Duration;

use super::*;

fn without_optin<T>(f: impl FnOnce() -> T) -> T {
    temp_env::with_var_unset("FRAISEQL_KAFKA_ALLOW_PLAINTEXT", f)
}

fn with_optin<T>(f: impl FnOnce() -> T) -> T {
    temp_env::with_vars(
        [
            ("FRAISEQL_KAFKA_ALLOW_PLAINTEXT", Some("true")),
            ("FRAISEQL_ENV", Some("development")),
            ("FRAISEQL_PROFILE", None),
            ("KUBERNETES_SERVICE_HOST", None),
        ],
        f,
    )
}

/// The refusal happens before a client exists, so a bad endpoint cannot reach
/// librdkafka's `PLAINTEXT` default by any path through this crate.
#[test]
fn a_scheme_less_endpoint_is_refused_at_connect() {
    without_optin(|| {
        let err = KafkaEgress::connect("broker:9092", &KafkaEgressConfig::new("test"))
            .expect_err("a bare bootstrap list must be refused, not defaulted to plaintext");
        assert!(matches!(err, EgressError::Config(_)), "got {err:?}");
        assert!(err.to_string().contains("scheme"), "{err}");
    });
}

/// The guard's plaintext rule reaches through this crate unchanged. Both directions,
/// because a guard tested one way is how #816 shipped inverted.
#[test]
fn plaintext_is_refused_without_the_opt_in_and_accepted_with_it() {
    without_optin(|| {
        assert!(
            KafkaEgress::connect("kafka://localhost:9092", &KafkaEgressConfig::new("test"))
                .is_err(),
            "plaintext without the development opt-in must be refused"
        );
    });
    with_optin(|| {
        assert!(
            KafkaEgress::connect("kafka://localhost:9092", &KafkaEgressConfig::new("test")).is_ok(),
            "the opt-in in a declared development environment must be honoured — a guard \
             that refuses everything is not a guard"
        );
    });
}

/// …and the opt-in does not become an SSRF licence: a non-loopback broker on the
/// plaintext path is still refused even when opted in.
#[test]
fn the_plaintext_opt_in_does_not_reach_past_loopback() {
    with_optin(|| {
        assert!(
            KafkaEgress::connect("kafka://169.254.169.254:9092", &KafkaEgressConfig::new("test"))
                .is_err(),
            "the metadata service must stay refused on the opted-in plaintext path"
        );
    });
}

/// An encrypted endpoint needs no opt-in and is not host-screened: MSK and every
/// VPC-hosted cluster addresses brokers in RFC 1918 space.
///
/// This is also where rdkafka's `ssl` feature is proved compiled in. Client creation is
/// where librdkafka validates `security.protocol`, and without that feature this call
/// fails with *"Unsupported value `ssl` … `OpenSSL` not available at build time"* — which
/// is the state #1102 was filed against. A guard suite that only proves the refusing
/// half is #816's shape again: green, and half a guard.
#[test]
fn an_ssl_endpoint_needs_no_opt_in_and_permits_private_brokers() {
    without_optin(|| {
        assert!(
            KafkaEgress::connect("kafka+ssl://10.0.1.5:9093", &KafkaEgressConfig::new("test"))
                .is_ok(),
            "a private-range broker over TLS is a normal managed-Kafka deployment"
        );
    });
}

/// `kafka+sasl-ssl://` resolves credentials at connect. Unset ⇒ refused, rather than a
/// client that fails on its first produce.
#[test]
fn sasl_credentials_are_resolved_at_connect_not_at_produce() {
    temp_env::with_vars(
        [
            ("FRAISEQL_KAFKA_SASL_MECHANISM", None::<&str>),
            ("FRAISEQL_KAFKA_SASL_USERNAME", None),
            ("FRAISEQL_KAFKA_SASL_PASSWORD", None),
        ],
        || {
            let err = KafkaEgress::connect(
                "kafka+sasl-ssl://broker.example.com:9093",
                &KafkaEgressConfig::new("test"),
            )
            .expect_err("no mechanism configured");
            assert!(matches!(err, EgressError::Config(_)), "got {err:?}");
        },
    );
}

/// The defaults are the durable caller's. A caller with no retry loop must be able to
/// shorten both, and both together — a hot path that blocks for the drain's budget is
/// the failure this method exists to prevent.
#[test]
fn timeouts_are_the_callers_to_choose() {
    let default = KafkaEgressConfig::new("test");
    assert_eq!(default.message_timeout, Duration::from_secs(30));
    assert_eq!(default.enqueue_timeout, Duration::from_secs(5));
    assert!(default.idempotent);

    let hot_path = KafkaEgressConfig::new("test")
        .with_timeouts(Duration::from_secs(5), Duration::from_millis(250));
    assert_eq!(hot_path.message_timeout, Duration::from_secs(5));
    assert_eq!(hot_path.enqueue_timeout, Duration::from_millis(250));
}

/// Only failures that cannot succeed on redelivery are permanent. A misclassified
/// transient error costs a delay; a misclassified permanent one discards the message.
#[test]
fn classification_defaults_to_transient() {
    use rdkafka::{error::KafkaError, types::RDKafkaErrorCode};

    for code in [
        RDKafkaErrorCode::MessageSizeTooLarge,
        RDKafkaErrorCode::MessageBatchTooLarge,
        RDKafkaErrorCode::InvalidTopic,
        RDKafkaErrorCode::InvalidRecord,
    ] {
        assert!(
            matches!(classify(&KafkaError::MessageProduction(code)), EgressOutcome::Permanent(_)),
            "{code:?} cannot succeed on redelivery of the same bytes"
        );
    }

    for code in [
        RDKafkaErrorCode::BrokerTransportFailure,
        RDKafkaErrorCode::RequestTimedOut,
        RDKafkaErrorCode::QueueFull,
        RDKafkaErrorCode::NotLeaderForPartition,
    ] {
        assert!(
            matches!(classify(&KafkaError::MessageProduction(code)), EgressOutcome::Transient(_)),
            "{code:?} must be retryable"
        );
    }
}
