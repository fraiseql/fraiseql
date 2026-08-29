//! The boot contract: an absent section builds nothing, a bad one refuses to boot.
//!
//! Delivery itself needs a broker and is covered by
//! `tests/subscription_kafka_mirror_e2e.rs` on the Dagger leg that binds one.

#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code.

use super::*;

fn section(endpoint: &str) -> SubscriptionKafkaConfig {
    SubscriptionKafkaConfig {
        endpoint: endpoint.to_owned(),
        ..SubscriptionKafkaConfig::default()
    }
}

#[test]
fn an_absent_section_builds_nothing() {
    assert!(build_mirror(None).expect("absent is not an error").is_none());
}

#[test]
fn a_scheme_less_endpoint_refuses_the_boot() {
    temp_env::with_var_unset("FRAISEQL_KAFKA_ALLOW_PLAINTEXT", || {
        let err = build_mirror(Some(&section("broker.internal:9092")))
            .expect_err("a bare bootstrap list must not boot a producer");
        assert!(err.contains("scheme"), "{err}");
    });
}

#[test]
fn plaintext_refuses_the_boot_without_the_opt_in_and_starts_with_it() {
    temp_env::with_var_unset("FRAISEQL_KAFKA_ALLOW_PLAINTEXT", || {
        assert!(
            build_mirror(Some(&section("kafka://localhost:9092"))).is_err(),
            "an operator who asked for a guarded producer must not get an unguarded one"
        );
    });
    temp_env::with_vars(
        [
            ("FRAISEQL_KAFKA_ALLOW_PLAINTEXT", Some("true")),
            ("FRAISEQL_ENV", Some("development")),
            ("FRAISEQL_PROFILE", None),
            ("KUBERNETES_SERVICE_HOST", None),
        ],
        || {
            assert!(
                build_mirror(Some(&section("kafka://localhost:9092")))
                    .expect("the development opt-in must be honoured")
                    .is_some(),
                "a guard that refuses everything is not a guard"
            );
        },
    );
}

#[test]
fn an_incomplete_section_refuses_the_boot() {
    let err = build_mirror(Some(&SubscriptionKafkaConfig::default()))
        .expect_err("an empty endpoint must not read as 'disabled'");
    assert!(err.contains("Omit the section"), "{err}");
}
