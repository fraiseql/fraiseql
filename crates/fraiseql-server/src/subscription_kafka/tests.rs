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

/// A planned subscription to `Order { id note }`, its manager, and the row `v_order` holds.
fn planned_order() -> (Arc<SubscriptionManager>, SubscriptionId) {
    use fraiseql_core::schema::{
        CompiledSchema, FieldDefinition, FieldType, SubscriptionDefinition, TypeDefinition,
    };
    let mut schema = CompiledSchema::new();
    let mut order = TypeDefinition::new("Order", "v_order");
    order.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::nullable("note", FieldType::String),
    ];
    schema.types.push(order);
    schema.subscriptions.push(SubscriptionDefinition::new("orderChanged", "Order"));
    schema.build_indexes();
    let stored = fraiseql_core::db::types::JsonbValue::new(
        serde_json::json!({"id": "o1", "note": "as the subscriber reads it"}),
    );
    let executor = fraiseql_core::runtime::Executor::new(
        schema.clone(),
        Arc::new(
            fraiseql_test_utils::failing_adapter::FailingAdapter::new()
                .with_response("v_order", vec![stored]),
        ),
    );
    let document = fraiseql_core::graphql::parse_query("subscription { orderChanged { id note } }")
        .expect("the subscription parses");
    let plan = executor.plan_subscription(&document, None, None).expect("planned");
    let manager = Arc::new(SubscriptionManager::new(Arc::new(schema)));
    let id = manager
        .subscribe_planned(
            Arc::new(plan),
            serde_json::json!({}),
            serde_json::json!({}),
            "c1",
            vec![],
        )
        .expect("subscribed");
    (manager, id)
}

fn changed(
    operation: fraiseql_core::runtime::subscription::SubscriptionOperation,
) -> fraiseql_core::runtime::subscription::SubscriptionEvent {
    fraiseql_core::runtime::subscription::SubscriptionEvent::new(
        "Order",
        "o1",
        operation,
        serde_json::json!({"id": "o1", "note": "as the writer wrote it"}),
    )
}

#[tokio::test]
async fn the_mirror_is_served_the_subscriptions_read_and_leaves_the_subscribers_own_state() {
    use fraiseql_core::runtime::subscription::SubscriptionOperation::{Delete, Update};
    let (manager, _) = planned_order();
    let mut rx = manager.receiver();
    let mut mirrored = Mirrored::default();

    manager.publish_event(changed(Update));
    let pending = rx.try_recv().unwrap();
    let mirror = mirrored.serve(&manager, pending.clone()).await.expect("mirrored");
    let expected = serde_json::json!({"id": "o1", "note": "as the subscriber reads it"});
    assert_eq!((&mirror.data, &mirror.event.data), (&expected, &expected));
    let subscriber = manager.serve(pending, &mut SubscriberReads::default()).await.unwrap();
    assert_eq!(
        subscriber.map(|p| p.event.sequence_number),
        Some(1),
        "the subscriber's position is its own"
    );

    manager.publish_event(changed(Delete));
    let pending = rx.try_recv().unwrap();
    assert!(mirrored.serve(&manager, pending.clone()).await.is_some(), "the mirror's delete");
    let subscriber = manager.serve(pending, &mut SubscriberReads::default()).await.unwrap();
    assert!(subscriber.is_some(), "the subscriber's delete is not consumed by the mirror");
}

#[tokio::test]
async fn the_mirror_is_not_served_a_delete_of_a_row_it_was_never_served() {
    let (manager, _) = planned_order();
    let mut rx = manager.receiver();
    let mut mirrored = Mirrored::default();
    // A delete of a row the mirror was never served.
    manager.publish_event(changed(
        fraiseql_core::runtime::subscription::SubscriptionOperation::Delete,
    ));
    assert!(mirrored.serve(&manager, rx.try_recv().unwrap()).await.is_none());
}
