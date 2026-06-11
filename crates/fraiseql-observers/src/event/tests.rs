#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

#[test]
fn builders_set_actor_envelope() {
    let event = EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        Uuid::nil(),
        serde_json::json!({}),
    )
    .with_actor_type("ai_agent")
    .with_acting_for("11111111-1111-1111-1111-111111111111")
    .with_schema_version("v2.7.0");

    assert_eq!(event.actor_type.as_deref(), Some("ai_agent"));
    assert_eq!(event.acting_for.as_deref(), Some("11111111-1111-1111-1111-111111111111"));
    assert_eq!(event.schema_version.as_deref(), Some("v2.7.0"));
}

#[test]
fn deserializes_legacy_event_without_envelope_fields() {
    // EntityEvent is JSON-serialized onto the NATS wire. A producer from before
    // the Change-Spine envelope fields existed emits no `tenant_id`/`duration_ms`/
    // `seq`/`actor_type`/`acting_for`/`schema_version`. `#[serde(default)]` must let
    // a newer consumer decode it (→ None) rather than fail on a missing field.
    let legacy = serde_json::json!({
        "id": "22222222-2222-2222-2222-222222222222",
        "event_type": "INSERT",
        "entity_type": "Order",
        "entity_id": "33333333-3333-3333-3333-333333333333",
        "data": { "total": 100 },
        "changes": null,
        "user_id": null,
        "timestamp": "2026-01-22T10:00:00Z"
    });

    let event: EntityEvent = serde_json::from_value(legacy).unwrap();

    assert_eq!(event.entity_type, "Order");
    assert_eq!(event.tenant_id, None);
    assert_eq!(event.duration_ms, None);
    assert_eq!(event.seq, None);
    assert_eq!(event.actor_type, None);
    assert_eq!(event.acting_for, None);
    assert_eq!(event.schema_version, None);
}

#[test]
fn round_trips_actor_envelope_over_the_wire() {
    let event = EntityEvent::new(
        EventKind::Updated,
        "User".to_string(),
        Uuid::nil(),
        serde_json::json!({}),
    )
    .with_actor_type("service_account")
    .with_acting_for("44444444-4444-4444-4444-444444444444")
    .with_schema_version("v2.7.0");

    let bytes = serde_json::to_vec(&event).unwrap();
    let decoded: EntityEvent = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(decoded.actor_type.as_deref(), Some("service_account"));
    assert_eq!(decoded.acting_for.as_deref(), Some("44444444-4444-4444-4444-444444444444"));
    assert_eq!(decoded.schema_version.as_deref(), Some("v2.7.0"));
}
