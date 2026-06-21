#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::time::Duration;

use uuid::Uuid;

use super::*;
use crate::event::{ChangeEvent, ChangeOp};

#[test]
fn sink_kind_serde_is_kebab() {
    assert_eq!(serde_json::to_string(&SinkKind::NatsJetStream).unwrap(), "\"nats-jetstream\"");
    assert_eq!(
        serde_json::from_str::<SinkKind>("\"nats-jetstream\"").unwrap(),
        SinkKind::NatsJetStream
    );
}

#[test]
fn render_subject_happy_path() {
    let tenant = Uuid::from_u128(0xab);
    let ev = ChangeEvent::new(1, "tb_post", ChangeOp::Insert).with_tenant(tenant);
    assert_eq!(
        render_subject("fraiseql.{tenant_id}.{table}", &ev).unwrap(),
        format!("fraiseql.{tenant}.tb_post")
    );
}

#[test]
fn render_subject_none_tenant_renders_placeholder() {
    let ev = ChangeEvent::new(1, "tb_post", ChangeOp::Insert);
    assert_eq!(
        render_subject("fraiseql.{tenant_id}.{table}", &ev).unwrap(),
        "fraiseql._none_.tb_post"
    );
}

#[test]
fn render_subject_op_placeholder() {
    let ev = ChangeEvent::new(1, "tb_post", ChangeOp::Update);
    assert_eq!(render_subject("c.{table}.{op}", &ev).unwrap(), "c.tb_post.update");
}

#[test]
fn render_subject_rejects_nats_illegal_table_chars() {
    for bad in ["tb.post", "tb*post", "tb>post", "tb post", "tb\tpost"] {
        let ev = ChangeEvent::new(1, bad, ChangeOp::Insert);
        assert!(
            render_subject("fraiseql.{tenant_id}.{table}", &ev).is_err(),
            "should reject illegal table segment {bad:?}"
        );
    }
}

#[test]
fn matches_allows_everything_with_no_filter() {
    let cfg = CdcSinkConfig::new("s", "fraiseql.{table}");
    assert!(cfg.matches(&ChangeEvent::new(1, "tb_post", ChangeOp::Insert)));
}

#[test]
fn matches_table_allowlist() {
    let cfg = CdcSinkConfig::new("s", "t").with_tables(vec!["tb_post".to_owned()]);
    assert!(cfg.matches(&ChangeEvent::new(1, "tb_post", ChangeOp::Insert)));
    assert!(!cfg.matches(&ChangeEvent::new(1, "tb_user", ChangeOp::Insert)));
}

#[test]
fn matches_tenant_allowlist_rejects_other_and_unstamped() {
    let allowed = Uuid::from_u128(1);
    let cfg = CdcSinkConfig::new("s", "t").with_tenants(vec![allowed]);
    assert!(cfg.matches(&ChangeEvent::new(1, "tb_post", ChangeOp::Insert).with_tenant(allowed)));
    assert!(!cfg.matches(
        &ChangeEvent::new(1, "tb_post", ChangeOp::Insert).with_tenant(Uuid::from_u128(2))
    ));
    assert!(!cfg.matches(&ChangeEvent::new(1, "tb_post", ChangeOp::Insert)));
}

#[test]
fn publish_outcome_distinguishes_transient_and_permanent() {
    assert_ne!(
        PublishOutcome::Transient("x".to_owned()),
        PublishOutcome::Permanent("x".to_owned())
    );
    assert_ne!(PublishOutcome::Published, PublishOutcome::Transient("x".to_owned()));
}

#[test]
fn backoff_is_monotonic_and_capped() {
    assert_eq!(next_attempt_delay(0), Duration::from_secs(1));
    assert_eq!(next_attempt_delay(1), Duration::from_secs(1));
    assert_eq!(next_attempt_delay(2), Duration::from_secs(2));
    assert_eq!(next_attempt_delay(3), Duration::from_secs(4));
    assert_eq!(next_attempt_delay(9), Duration::from_secs(256));
    assert_eq!(next_attempt_delay(20), Duration::from_secs(300));
    assert_eq!(next_attempt_delay(u32::MAX), Duration::from_secs(300));
}
