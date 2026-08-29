//! Tests for what this sink does with the shared egress, not for the egress itself.
//!
//! The endpoint guard lives in `fraiseql_guard::kafka` and the connect/classify contract
//! in `fraiseql_kafka` (#1102); both are tested there, in the always-compiled leg. What
//! is left here is that **this sink consults them** — a refactor that stopped resolving
//! SASL, or stopped screening the endpoint, would leave those suites green.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::KafkaSink;
use crate::sink::CdcSinkConfig;

#[test]
fn tls_endpoints_are_accepted_because_openssl_is_compiled_in() {
    // The accepting half of the transport guard, assertable without a broker.
    //
    // Producer creation is where librdkafka validates `security.protocol`, and
    // without rdkafka's `ssl` feature this call fails with *"Unsupported value
    // \"ssl\" ... OpenSSL not available at build time"*. That is what made the
    // accepting half untestable before, and a guard suite proving only the
    // refusing half is #816's shape again — green, and half a guard.
    //
    // No connection is attempted here: librdkafka fetches metadata lazily on a
    // background thread, so an unreachable broker still yields a producer.
    let cfg = CdcSinkConfig::new("primary", "fraiseql.{table}");
    let sink = KafkaSink::connect("kafka+ssl://broker.invalid:9092", cfg);
    assert!(
        sink.is_ok(),
        "kafka+ssl:// must be constructible — is rdkafka's `ssl` feature still on? {:?}",
        sink.err().map(|e| e.to_string())
    );
}

#[test]
fn sasl_ssl_endpoints_refuse_before_librdkafka_can_default_to_kerberos() {
    // Belt-and-braces over the pure `resolve_kafka_sasl` tests: assert the sink
    // actually consults it, so a future refactor cannot leave `sasl.mechanism`
    // unset and inherit librdkafka's GSSAPI default (which this build has no
    // provider for).
    temp_env::with_vars(
        [
            ("FRAISEQL_KAFKA_SASL_MECHANISM", None::<&str>),
            ("FRAISEQL_KAFKA_SASL_USERNAME", None),
            ("FRAISEQL_KAFKA_SASL_PASSWORD", None),
        ],
        || {
            let cfg = CdcSinkConfig::new("primary", "fraiseql.{table}");
            let err = KafkaSink::connect("kafka+sasl-ssl://broker.invalid:9092", cfg)
                .err()
                .expect("sasl-ssl without a mechanism must be refused")
                .to_string();
            assert!(
                err.contains("FRAISEQL_KAFKA_SASL_MECHANISM"),
                "should be our named refusal, not librdkafka's: {err}"
            );
        },
    );
}

#[test]
fn sasl_ssl_builds_a_producer_once_credentials_are_present() {
    // Proves the mechanism/credential values we emit are ones librdkafka accepts —
    // it validates them at client creation (PLAIN and SCRAM both reject a missing
    // username there).
    temp_env::with_vars(
        [
            ("FRAISEQL_KAFKA_SASL_MECHANISM", Some("SCRAM-SHA-512")),
            ("FRAISEQL_KAFKA_SASL_USERNAME", Some("u")),
            ("FRAISEQL_KAFKA_SASL_PASSWORD", Some("p")),
        ],
        || {
            let cfg = CdcSinkConfig::new("primary", "fraiseql.{table}");
            let sink = KafkaSink::connect("kafka+sasl-ssl://broker.invalid:9092", cfg);
            assert!(sink.is_ok(), "{:?}", sink.err().map(|e| e.to_string()));
        },
    );
}

// The partition-key properties moved to `sink::tests` with the key itself: it is
// shared with the Kinesis sink and pure, so it belongs in the always-compiled leg
// rather than only where `cdc-kafka` is on.
