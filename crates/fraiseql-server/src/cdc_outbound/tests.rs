//! Boot-refusal and subject-derivation tests for the outbound CDC wiring (#382).

#![allow(clippy::unwrap_used)] // Reason: test module
#![allow(clippy::panic)] // Reason: test module — an unmet precondition must fail loudly

use super::{build_drains, subject_wildcard};
use crate::server_config::cdc_outbound::{CdcOutboundConfig, CdcSinkSectionConfig};

fn sink(name: &str, kind: &str) -> CdcSinkSectionConfig {
    CdcSinkSectionConfig {
        name:             name.to_string(),
        kind:             kind.to_string(),
        endpoint:         "nats://localhost:4222".to_string(),
        subject_template: "fraiseql.{tenant_id}.{table}".to_string(),
        tables:           None,
        tenants:          None,
        max_attempts:     None,
        ensure_stream:    None,
    }
}

fn config(sinks: Vec<CdcSinkSectionConfig>) -> CdcOutboundConfig {
    CdcOutboundConfig {
        sinks,
        ..CdcOutboundConfig::default()
    }
}

/// Every way a section can be wrong is a named refusal, never a server that
/// boots believing it replicates changes it would in fact drop.
#[test]
fn invalid_sections_are_refused_by_name() {
    let cases: Vec<(CdcOutboundConfig, &str)> = vec![
        (config(vec![]), "declares no sinks"),
        (
            CdcOutboundConfig {
                sinks: vec![sink("a", "nats-jetstream")],
                tick_interval_secs: 0,
                ..CdcOutboundConfig::default()
            },
            "tick_interval_secs",
        ),
        (
            CdcOutboundConfig {
                sinks: vec![sink("a", "nats-jetstream")],
                batch_size: 0,
                ..CdcOutboundConfig::default()
            },
            "batch_size",
        ),
        (
            config(vec![sink("dup", "nats-jetstream"), sink("dup", "nats-jetstream")]),
            "duplicate sink name",
        ),
        (config(vec![sink("", "nats-jetstream")]), "empty name"),
        (config(vec![sink("a", "kinesis")]), "not implemented yet"),
        (config(vec![sink("a", "pulsar")]), "not implemented yet"),
        (config(vec![sink("a", "rabbitmq")]), "unknown kind"),
    ];
    for (cfg, expected) in cases {
        let err = cfg.validate().expect_err("must be refused");
        assert!(err.contains(expected), "expected {expected:?} in: {err}");
    }

    let mut empty_endpoint = sink("a", "nats-jetstream");
    empty_endpoint.endpoint = String::new();
    assert!(config(vec![empty_endpoint]).validate().unwrap_err().contains("empty endpoint"));

    let mut empty_subject = sink("a", "nats-jetstream");
    empty_subject.subject_template = "  ".to_string();
    assert!(
        config(vec![empty_subject])
            .validate()
            .unwrap_err()
            .contains("empty subject_template")
    );

    let mut zero_attempts = sink("a", "nats-jetstream");
    zero_attempts.max_attempts = Some(0);
    assert!(config(vec![zero_attempts]).validate().unwrap_err().contains("max_attempts"));
}

/// A valid section is accepted by validation (so the refusals above are not
/// vacuous — something must pass).
#[test]
fn a_valid_section_validates() {
    assert!(config(vec![sink("warehouse", "nats-jetstream")]).validate().is_ok());
}

/// `kind = "kafka"` is accepted only by a binary that compiled the sink in.
///
/// The two halves run in different legs by construction — a `cfg(not(feature))`
/// test never runs where the feature is on — so both a default-feature leg and a
/// `cdc-kafka` leg are needed to cover this pair. That is the point: the failure
/// this guards against is a feature-off binary accepting the kind and then
/// dropping every event.
#[cfg(feature = "cdc-kafka")]
#[test]
fn kafka_is_accepted_when_the_sink_is_compiled_in() {
    assert!(config(vec![sink("warehouse", "kafka")]).validate().is_ok());
    assert!(
        config(vec![sink("warehouse", "KAFKA")]).validate().is_ok(),
        "kind is case-folded"
    );
}

#[cfg(not(feature = "cdc-kafka"))]
#[test]
fn kafka_is_refused_by_name_when_the_sink_is_not_compiled_in() {
    let err = config(vec![sink("warehouse", "kafka")])
        .validate()
        .expect_err("must be refused");
    assert!(err.contains("cdc-kafka"), "should name the missing feature: {err}");
    assert!(
        !err.contains("unknown kind"),
        "must be refused by name, not as an unknown kind: {err}"
    );
}

/// A configured section with no database pool refuses to boot: the outbox and
/// the delivery state are both database-resident, so there is nothing to drain
/// from and no way to remember what was drained.
#[tokio::test]
async fn a_configured_section_without_a_pool_refuses_to_boot() {
    let cfg = config(vec![sink("warehouse", "nats-jetstream")]);
    let Err(err) = build_drains(Some(&cfg), None).await else {
        panic!("a configured section without a pool must refuse to boot")
    };
    assert!(err.contains("requires a database pool"), "{err}");
}

/// No section, no drain — and no error.
#[tokio::test]
async fn an_absent_section_builds_nothing() {
    match build_drains(None, None).await {
        Ok(None) => {},
        Ok(Some(_)) => panic!("no section must build no drains"),
        Err(err) => panic!("no section must not error: {err}"),
    }
}

/// The ensured stream must cover everything the template can render into; a
/// narrower subject would silently drop the events it excludes.
#[test]
fn subject_wildcard_covers_the_whole_template_space() {
    assert_eq!(subject_wildcard("fraiseql.{tenant_id}.{table}"), "fraiseql.>");
    assert_eq!(subject_wildcard("a.b.c.{table}"), "a.b.c.>");
    assert_eq!(subject_wildcard("{tenant_id}.events"), ">");
    assert_eq!(subject_wildcard("static.subject"), "static.subject.>");
}
