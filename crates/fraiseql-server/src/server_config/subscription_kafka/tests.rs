#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code.

use super::*;

#[test]
fn a_section_parses_from_toml_with_its_documented_keys() {
    let config: SubscriptionKafkaConfig = toml::from_str(
        r#"
endpoint = "kafka+ssl://broker.internal:9093"
default_topic = "fraiseql.subscriptions"
client_id = "svc"
timeout_ms = 2500
compression = "lz4"
"#,
    )
    .expect("the documented example must parse");

    assert_eq!(config.endpoint, "kafka+ssl://broker.internal:9093");
    assert_eq!(config.timeout_ms, 2_500);
    assert_eq!(config.compression.as_deref(), Some("lz4"));
    config.validate().expect("a complete section is valid");
}

#[test]
fn an_unknown_key_is_a_boot_error_not_a_silent_ignore() {
    let err = toml::from_str::<SubscriptionKafkaConfig>(
        r#"
endpoint = "kafka+ssl://broker.internal:9093"
brokers = "broker.internal:9092"
"#,
    )
    .expect_err("`brokers` is the pre-#1102 spelling and must not be silently ignored");
    assert!(format!("{err}").contains("brokers"), "{err}");
}

#[test]
fn a_present_but_empty_section_is_refused_rather_than_treated_as_off() {
    let err = SubscriptionKafkaConfig::default()
        .validate()
        .expect_err("an empty endpoint must not read as 'disabled'");
    assert!(err.contains("Omit the section"), "{err}");
}

#[test]
fn the_default_timeout_is_the_at_most_once_one() {
    // Not the CDC sink's 30s. There is no outbox behind this transport, so a delivery
    // task waiting half a minute is a parked task, not a second chance.
    assert_eq!(SubscriptionKafkaConfig::default().timeout_ms, 5_000);
}
