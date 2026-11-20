#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use serde_json::json;
use uuid::Uuid;

use super::*;

#[test]
fn from_modification_type_is_case_insensitive() {
    assert_eq!(ChangeOp::from_modification_type("INSERT"), ChangeOp::Insert);
    assert_eq!(ChangeOp::from_modification_type("update"), ChangeOp::Update);
    assert_eq!(ChangeOp::from_modification_type("Delete"), ChangeOp::Delete);
    assert_eq!(ChangeOp::from_modification_type("UPSERT"), ChangeOp::Custom);
}

#[test]
fn op_codes() {
    assert_eq!(ChangeOp::Insert.as_str(), "insert");
    assert_eq!(ChangeOp::Insert.debezium_code(), 'c');
    assert_eq!(ChangeOp::Update.debezium_code(), 'u');
    assert_eq!(ChangeOp::Delete.debezium_code(), 'd');
    assert_eq!(ChangeOp::Custom.debezium_code(), 'r');
}

#[test]
fn change_op_serde_roundtrip_is_kebab() {
    assert_eq!(serde_json::to_string(&ChangeOp::Update).unwrap(), "\"update\"");
    assert_eq!(serde_json::from_str::<ChangeOp>("\"delete\"").unwrap(), ChangeOp::Delete);
}

#[test]
fn builder_sets_optional_fields() {
    let tenant = Uuid::from_u128(0xab);
    let ev = ChangeEvent::new(3, "tb_post", ChangeOp::Insert)
        .with_tenant(tenant)
        .with_after(json!({ "id": 1 }));
    assert_eq!(ev.seq, 3);
    assert_eq!(ev.object_type, "tb_post");
    assert_eq!(ev.op, ChangeOp::Insert);
    assert_eq!(ev.tenant_id, Some(tenant));
    assert!(ev.after.is_some());
    assert!(ev.before.is_none());
    assert!(ev.object_id.is_none());
}

/// Timestamps are written as stored documents hold them today: the fraction padded to
/// `.120` / `.500`, none at a whole second, and a `Z` suffix. A document read and written
/// again keeps those bytes; webhooks, idempotency keys and JSONB comparisons depend on them.
mod stored_time_format {
    use crate::event::ChangeEvent;

    #[test]
    fn a_change_event_keeps_its_stored_commit_time() {
        let doc = serde_json::json!({"seq":1,"object_type":"Order","op":"insert","tenant_id":null,"object_id":null,"after":null,"before":null,"commit_time":"2025-11-20T12:00:00.120Z"});
        let read: ChangeEvent =
            serde_json::from_value(doc.clone()).expect("the stored document reads");
        assert_eq!(serde_json::to_value(&read).expect("it writes"), doc);
    }
}
