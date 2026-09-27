//! A subscription's read plan (ruling AA 4): what it refuses at subscribe time, and what it
//! serves or suppresses per event.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
use serde_json::json;

use super::suppressed_subscription_events;
use crate::{
    error::{FraiseQLError, Result},
    runtime::{Executor, RuntimeConfig, executor::test_support::CapturingMockAdapter},
    schema::{
        CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldType, RoleDefinition,
        SecurityConfig, StaticFilterCondition, SubscriptionDefinition, SubscriptionFilter,
        TypeDefinition,
    },
    security::{FieldAuthorizer, FieldAuthzDecision, FieldAuthzRequest, SecurityContext},
};

fn principal(roles: &[&str]) -> SecurityContext {
    SecurityContext {
        user_id:          "user-1".into(),
        roles:            roles.iter().map(|r| (*r).to_string()).collect(),
        tenant_id:        None,
        scopes:           vec![],
        attributes:       HashMap::default(),
        request_id:       "req-1".to_string(),
        ip_address:       None,
        expires_at:       Utc::now() + chrono::Duration::hours(1),
        authenticated_at: Utc::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

/// `Order { id, note (Mask, read:note), secret (Reject, read:secret), owner_note (authorize) }`
/// and `orderCreated: Order`; the role `analyst` grants `read:note`.
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut owner_note = FieldDefinition::nullable("owner_note", FieldType::String);
    owner_note.authorize = true;
    schema.types.push(TypeDefinition {
        fields: vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::nullable("note", FieldType::String)
                .with_requires_scope("read:note")
                .with_on_deny(FieldDenyPolicy::Mask),
            FieldDefinition::nullable("secret", FieldType::String)
                .with_requires_scope("read:secret"),
            owner_note,
        ],
        ..TypeDefinition::new("Order", "v_order")
    });
    schema.subscriptions.push(SubscriptionDefinition::new("orderCreated", "Order"));
    let mut security = SecurityConfig::default();
    security.add_role(RoleDefinition::new("analyst", vec!["read:note".to_string()]));
    schema.security = Some(security);
    schema.build_indexes();
    schema
}

fn executor(schema: CompiledSchema, config: RuntimeConfig) -> Executor {
    Executor::with_config(schema, Arc::new(CapturingMockAdapter::new(vec![])), config)
}

fn plan(
    exec: &Executor,
    query: &str,
    variables: &serde_json::Value,
    who: Option<&SecurityContext>,
) -> Result<super::SubscriptionPlan> {
    let document = crate::graphql::parse_query(query).unwrap();
    exec.plan_subscription(&document, Some(variables), who)
}

fn event() -> serde_json::Value {
    json!({"id": "o1", "note": "n", "secret": "s", "owner_note": "mine"})
}

#[test]
fn a_field_the_type_does_not_declare_refuses_the_plan() {
    let exec = executor(schema(), RuntimeConfig::default());
    let err =
        plan(&exec, "subscription { orderCreated { id margin } }", &json!({}), None).unwrap_err();
    assert!(matches!(err, FraiseQLError::Validation { .. }), "{err:?}");
    assert!(err.to_string().contains("margin"), "{err}");
}

#[test]
fn a_field_outside_the_compiled_field_list_refuses_the_plan() {
    let mut schema = schema();
    schema.subscriptions[0].fields = vec!["id".to_string()];
    let exec = executor(schema, RuntimeConfig::default());
    let err =
        plan(&exec, "subscription { orderCreated { id note } }", &json!({}), None).unwrap_err();
    assert!(err.to_string().contains("does not deliver field 'note'"), "{err}");
}

#[test]
fn a_type_whose_role_the_subscriber_lacks_refuses_the_plan() {
    let mut schema = schema();
    schema.types[0].requires_role = Some("finance".to_string());
    let exec = executor(schema, RuntimeConfig::default());
    let err =
        plan(&exec, "subscription { orderCreated { id } }", &json!({}), Some(&principal(&[])))
            .unwrap_err();
    assert!(matches!(err, FraiseQLError::Authorization { .. }), "{err:?}");
    plan(
        &exec,
        "subscription { orderCreated { id } }",
        &json!({}),
        Some(&principal(&["finance"])),
    )
    .expect("the role's holder may subscribe");
}

#[test]
fn a_masked_field_is_null_in_every_event_and_served_to_its_scopes_holder() {
    let exec = executor(schema(), RuntimeConfig::default());
    let query = "subscription { orderCreated { id note } }";
    let anonymous = plan(&exec, query, &json!({}), None).unwrap();
    assert_eq!(anonymous.deliver(&event()), Some(json!({"id": "o1", "note": null})));
    let analyst = plan(&exec, query, &json!({}), Some(&principal(&["analyst"]))).unwrap();
    assert_eq!(analyst.deliver(&event()), Some(json!({"id": "o1", "note": "n"})));
}

#[test]
fn a_static_filter_on_a_field_the_subscriber_may_not_read_refuses_the_plan() {
    let mut schema = schema();
    schema.subscriptions[0].filter = Some(SubscriptionFilter {
        argument_paths: HashMap::new(),
        static_filters: vec![StaticFilterCondition {
            path:     "/secret".to_string(),
            operator: crate::schema::FilterOperator::Eq,
            value:    json!("s"),
        }],
    });
    let exec = executor(schema, RuntimeConfig::default());
    let err = plan(&exec, "subscription { orderCreated { id } }", &json!({}), None).unwrap_err();
    assert!(matches!(err, FraiseQLError::Authorization { .. }), "{err:?}");
    assert!(err.to_string().contains("Order.secret"), "{err}");
}

#[test]
fn an_argument_filter_is_a_reference_only_when_the_subscriber_binds_it() {
    let mut schema = schema();
    schema.subscriptions[0].filter = Some(SubscriptionFilter {
        argument_paths: HashMap::from([("secret".to_string(), "/secret".to_string())]),
        static_filters: vec![],
    });
    let exec = executor(schema, RuntimeConfig::default());
    let query = "subscription { orderCreated { id } }";
    plan(&exec, query, &json!({}), None).expect("an unbound filter filters by nothing");
    let err = plan(&exec, query, &json!({"secret": "s"}), None).unwrap_err();
    assert!(matches!(err, FraiseQLError::Authorization { .. }), "{err:?}");
}

struct DenyReject;
impl FieldAuthorizer for DenyReject {
    fn authorize_field(&self, _r: &FieldAuthzRequest<'_>) -> Result<FieldAuthzDecision> {
        Ok(FieldAuthzDecision::Deny {
            code:    "not_owner".into(),
            on_deny: FieldDenyPolicy::Reject,
        })
    }
}

// The #423 decision is per document: a Reject over this event suppresses it for this
// subscriber — no frame — and counts once in the aggregate figure.
#[test]
fn an_event_the_field_authorizer_rejects_is_suppressed() {
    let config = RuntimeConfig::default().with_field_authorizer(Arc::new(DenyReject));
    let exec = executor(schema(), config);
    let planned = plan(
        &exec,
        "subscription { orderCreated { id owner_note } }",
        &json!({}),
        Some(&principal(&[])),
    )
    .unwrap();
    let before = suppressed_subscription_events();
    assert_eq!(planned.deliver(&event()), None, "a rejected event is not delivered");
    assert!(suppressed_subscription_events() > before, "the suppression is counted");
}

// The #423 refusals that need no document are decided at subscribe time.
#[test]
fn an_authorize_field_selected_anonymously_refuses_the_plan() {
    let config = RuntimeConfig::default().with_field_authorizer(Arc::new(DenyReject));
    let exec = executor(schema(), config);
    let err = plan(&exec, "subscription { orderCreated { id owner_note } }", &json!({}), None)
        .unwrap_err();
    assert!(matches!(err, FraiseQLError::Authorization { .. }), "{err:?}");
}

// A planned subscription whose event the plan suppresses is not delivered, and the event
// counts as no match.
#[test]
fn the_manager_delivers_nothing_for_a_suppressed_event() {
    use crate::runtime::subscription::{
        SubscriptionEvent, SubscriptionManager, SubscriptionOperation,
    };
    let config = RuntimeConfig::default().with_field_authorizer(Arc::new(DenyReject));
    let exec = executor(schema(), config);
    let planned = plan(
        &exec,
        "subscription { orderCreated { id owner_note } }",
        &json!({}),
        Some(&principal(&[])),
    )
    .unwrap();
    let manager = SubscriptionManager::new(Arc::new(schema()));
    let mut rx = manager.receiver();
    manager
        .subscribe_planned(Arc::new(planned), json!({}), json!({}), "c1", vec![])
        .unwrap();
    let delivered = manager.publish_event(SubscriptionEvent::new(
        "Order",
        "o1",
        SubscriptionOperation::Create,
        event(),
    ));
    assert_eq!(delivered, 0);
    assert!(rx.try_recv().is_err(), "nothing was sent");
}
