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
