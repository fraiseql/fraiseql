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

// ── AC 4: the subscription type's own row policy applies to the root after-image ──

/// `schema()` with `Order` carrying the keys the default policy reads (`tenant_id`,
/// `author_id`), under that policy.
fn row_policy_executor() -> Executor {
    let mut schema = schema();
    let order = schema.types.iter_mut().find(|t| t.name == "Order").unwrap();
    order.fields.push(FieldDefinition::nullable("tenant_id", FieldType::String));
    order.fields.push(FieldDefinition::nullable("author_id", FieldType::String));
    schema.build_indexes();
    let config = RuntimeConfig::default()
        .with_rls_policy(Arc::new(crate::security::DefaultRLSPolicy::new()));
    executor(schema, config)
}

fn tenant_principal() -> SecurityContext {
    SecurityContext {
        tenant_id: Some("t1".into()),
        ..principal(&[])
    }
}

// A subscription is a read of its type: an after-image the subscriber's row policy excludes
// is suppressed, as a query would not return that row.
#[test]
fn a_root_row_the_subscribers_policy_excludes_is_suppressed() {
    let exec = row_policy_executor();
    let planned = plan(
        &exec,
        "subscription { orderCreated { id } }",
        &json!({}),
        Some(&tenant_principal()),
    )
    .unwrap();
    let others = json!({"id": "o2", "tenant_id": "t1", "author_id": "someone-else"});
    assert_eq!(planned.deliver(&others), None, "another owner's order is not the subscriber's");
    let own = json!({"id": "o1", "tenant_id": "t1", "author_id": "user-1"});
    assert_eq!(planned.deliver(&own), Some(json!({"id": "o1"})));
}

// No principal, no policy to evaluate (#784): refused as a query is.
#[test]
fn an_anonymous_subscription_under_a_row_policy_is_refused() {
    let exec = row_policy_executor();
    let res = plan(&exec, "subscription { orderCreated { id } }", &json!({}), None);
    assert!(res.is_err(), "{res:?}");
}

// A policy that reads a key the type does not declare cannot be evaluated over the
// after-image: refused at subscribe, never delivered unfiltered.
#[test]
fn a_root_policy_the_after_image_cannot_answer_refuses_the_plan() {
    let config = RuntimeConfig::default()
        .with_rls_policy(Arc::new(crate::security::DefaultRLSPolicy::new()));
    let exec = executor(schema(), config);
    let res = plan(
        &exec,
        "subscription { orderCreated { id } }",
        &json!({}),
        Some(&tenant_principal()),
    );
    assert!(matches!(res, Err(FraiseQLError::Authorization { .. })), "{res:?}");
}

// ── AC 6: the manager delivers a gated type only through a plan ──

#[test]
fn an_unplanned_subscription_to_a_gated_type_is_refused() {
    use crate::runtime::subscription::SubscriptionManager;
    let manager = SubscriptionManager::new(Arc::new(schema()));
    let res = manager.subscribe("orderCreated", json!({}), json!({}), "c1");
    assert!(res.is_err(), "`Order` has scoped and authorize fields: {res:?}");
}

// Control: an ungated type still subscribes without a plan.
#[test]
fn an_unplanned_subscription_to_an_ungated_type_is_accepted() {
    use crate::runtime::subscription::SubscriptionManager;
    let mut schema = CompiledSchema::new();
    schema.types.push(TypeDefinition {
        fields: vec![FieldDefinition::new("id", FieldType::Id)],
        ..TypeDefinition::new("Ping", "v_ping")
    });
    schema.subscriptions.push(SubscriptionDefinition::new("pinged", "Ping"));
    schema.build_indexes();
    SubscriptionManager::new(Arc::new(schema))
        .subscribe("pinged", json!({}), json!({}), "c1")
        .expect("nothing to gate");
}

/// A policy over `Order` rows whose predicate and declared keys a test chooses.
struct ChosenPolicy {
    clause: crate::db::WhereClause,
    paths:  crate::security::ConstrainedPaths,
}

impl crate::security::RLSPolicy for ChosenPolicy {
    fn evaluate(
        &self,
        _context: &SecurityContext,
        _target: &crate::security::rls_policy::RlsTarget<'_>,
    ) -> Result<Option<crate::security::RlsWhereClause>> {
        Ok(Some(crate::security::RlsWhereClause::new(self.clause.clone())))
    }

    fn constrained_paths(
        &self,
        _target: &crate::security::rls_policy::RlsTarget<'_>,
    ) -> crate::security::ConstrainedPaths {
        self.paths.clone()
    }
}

fn eq(key: &str) -> crate::db::WhereClause {
    crate::db::WhereClause::Field {
        path:     vec![key.to_string()],
        operator: crate::db::WhereOperator::Eq,
        value:    json!("t1"),
    }
}

fn plan_under(policy: ChosenPolicy) -> Result<super::SubscriptionPlan> {
    let mut schema = schema();
    let order = schema.types.iter_mut().find(|t| t.name == "Order").unwrap();
    order.fields.push(FieldDefinition::nullable("tenant_id", FieldType::String));
    order.fields.push(FieldDefinition::nullable("author_id", FieldType::String));
    schema.build_indexes();
    let exec = executor(schema, RuntimeConfig::default().with_rls_policy(Arc::new(policy)));
    plan(
        &exec,
        "subscription { orderCreated { id } }",
        &json!({}),
        Some(&tenant_principal()),
    )
}

// Each shape a document cannot answer refuses the subscription; the answerable one plans.
#[test]
fn a_root_policy_is_planned_only_in_the_shape_a_document_can_answer() {
    use crate::security::ConstrainedPaths::{Declared, Opaque};
    plan_under(ChosenPolicy {
        clause: eq("tenant_id"),
        paths:  Declared(vec!["tenant_id".to_string()]),
    })
    .expect("a declared equality over a declared key");
    for (why, policy) in [
        (
            "undeclared keys",
            ChosenPolicy {
                clause: eq("tenant_id"),
                paths:  Opaque,
            },
        ),
        (
            "a key outside its declaration",
            ChosenPolicy {
                clause: eq("author_id"),
                paths:  Declared(vec!["tenant_id".to_string()]),
            },
        ),
        (
            "a disjunction",
            ChosenPolicy {
                clause: crate::db::WhereClause::Or(vec![eq("tenant_id"), eq("author_id")]),
                paths:  Declared(vec!["tenant_id".to_string(), "author_id".to_string()]),
            },
        ),
    ] {
        let res = plan_under(policy);
        assert!(matches!(res, Err(FraiseQLError::Authorization { .. })), "{why}: {res:?}");
    }
}

// The gate the unplanned entries refuse is found at any level, and a type's role is one.
#[test]
fn an_unplanned_subscription_reaching_a_role_or_a_nested_gate_is_refused() {
    use crate::runtime::subscription::SubscriptionManager;
    let ungated = |name: &str, fields| TypeDefinition {
        fields,
        ..TypeDefinition::new(name, "v_x")
    };
    let mut role_gated = CompiledSchema::new();
    role_gated.types.push(TypeDefinition {
        requires_role: Some("finance".to_string()),
        ..ungated("Ping", vec![FieldDefinition::new("id", FieldType::Id)])
    });
    role_gated.subscriptions.push(SubscriptionDefinition::new("pinged", "Ping"));
    role_gated.build_indexes();
    let mut nested = CompiledSchema::new();
    nested.types.push(ungated(
        "Ping",
        vec![FieldDefinition::nullable(
            "order",
            FieldType::Object("Order".to_string()),
        )],
    ));
    nested.types.push(schema().types.remove(0));
    nested.subscriptions.push(SubscriptionDefinition::new("pinged", "Ping"));
    nested.security = Some(SecurityConfig::default());
    nested.build_indexes();
    for (why, schema) in [
        ("a role-gated type", role_gated),
        ("a nested scoped field", nested),
    ] {
        let res = SubscriptionManager::new(Arc::new(schema)).subscribe(
            "pinged",
            json!({}),
            json!({}),
            "c1",
        );
        assert!(res.is_err(), "{why}: {res:?}");
    }
}

// ── AC 7: every push seam carries only what the plan served ──

// The broadcast payload is what every seam consumes — `/ws` sends its `data`, the Kafka
// mirror and the webhook adapter serialise its `event`. For a planned subscription the
// event must carry the served document, and no before-image (which no plan covers).
#[test]
fn a_planned_payload_carries_only_the_served_document() {
    use crate::runtime::subscription::{
        SubscriptionEvent, SubscriptionManager, SubscriptionOperation,
    };
    let exec = executor(schema(), RuntimeConfig::default());
    let planned = plan(&exec, "subscription { orderCreated { id } }", &json!({}), None).unwrap();
    let manager = SubscriptionManager::new(Arc::new(schema()));
    let mut rx = manager.receiver();
    manager
        .subscribe_planned(Arc::new(planned), json!({}), json!({}), "c1", vec![])
        .unwrap();
    let mut event = SubscriptionEvent::new("Order", "o1", SubscriptionOperation::Update, event());
    event.old_data = Some(json!({"id": "o1", "secret": "old"}));
    assert_eq!(manager.publish_event(event), 1);
    let payload = rx.try_recv().unwrap();
    assert_eq!(payload.data, json!({"id": "o1"}));
    assert_eq!(payload.event.data, json!({"id": "o1"}), "the event a seam serialises");
    assert_eq!(payload.event.old_data, None, "no plan covers the before-image");
}

// ── AC 7: a REST stream is planned as a `GET` of its resource with no `?select=` ──

#[test]
fn a_type_stream_reads_every_declared_field_masking_what_the_reader_may_not_read() {
    let mut schema = schema();
    // No `Reject` field and no `authorize` field: a whole-type read of `Order` would
    // otherwise be refused for this reader, as its `GET` is.
    schema.types[0].fields.retain(|f| f.name == "id" || f.name == "note");
    let exec = executor(schema, RuntimeConfig::default());
    let planned = exec.plan_type_stream("Order", Some(&principal(&[]))).unwrap();
    assert_eq!(planned.deliver(&event()), Some(json!({"id": "o1", "note": null})));
}

#[test]
fn a_type_stream_is_refused_what_its_get_is_refused() {
    let exec = executor(schema(), RuntimeConfig::default());
    let err = exec.plan_type_stream("Order", Some(&principal(&[]))).unwrap_err();
    assert!(matches!(err, FraiseQLError::Authorization { .. }), "a Reject field: {err:?}");
    let mut role_gated = schema();
    role_gated.types[0].fields.retain(|f| f.name == "id");
    role_gated.types[0].requires_role = Some("finance".to_string());
    let exec = executor(role_gated, RuntimeConfig::default());
    let err = exec.plan_type_stream("Order", Some(&principal(&[]))).unwrap_err();
    assert!(err.to_string().contains("requires a role"), "{err}");
    exec.plan_type_stream("Order", Some(&principal(&["finance"])))
        .expect("the role's holder may stream it");
}

// ── AA 5: a subscriber sees its own delivery position, never a server-wide one ──

// Positions count what this subscription delivered, gap-free: an event its plan suppressed
// (another owner's row) leaves no hole, and the Change-Spine `seq` — a server-wide position
// that counts every change, including the ones withheld — never reaches the payload.
#[test]
fn a_subscription_counts_its_own_deliveries_and_carries_no_server_position() {
    use crate::runtime::subscription::{
        ChangeSpineEnvelope, SubscriptionEvent, SubscriptionManager, SubscriptionOperation,
    };
    let exec = row_policy_executor();
    let planned = plan(
        &exec,
        "subscription { orderCreated { id } }",
        &json!({}),
        Some(&tenant_principal()),
    )
    .unwrap();
    let manager = SubscriptionManager::new(Arc::new(schema()));
    let mut rx = manager.receiver();
    manager
        .subscribe_planned(Arc::new(planned), json!({}), json!({}), "c1", vec![])
        .unwrap();
    let publish = |id: &str, author: &str, seq: i64| {
        manager.publish_event(
            SubscriptionEvent::new(
                "Order",
                id,
                SubscriptionOperation::Create,
                json!({"id": id, "tenant_id": "t1", "author_id": author}),
            )
            .with_change_spine(ChangeSpineEnvelope {
                seq: Some(seq),
                ..ChangeSpineEnvelope::default()
            }),
        )
    };
    publish("o1", "user-1", 41);
    publish("o2", "someone-else", 42);
    publish("o3", "user-1", 43);
    let first = rx.try_recv().unwrap();
    let second = rx.try_recv().unwrap();
    assert_eq!(
        (first.event.sequence_number, second.event.sequence_number),
        (1, 2),
        "positions count this subscription's deliveries, without the suppressed one"
    );
    for payload in [&first, &second] {
        assert!(
            payload.event.change_spine.as_ref().is_none_or(|e| e.seq.is_none()),
            "the Change-Spine seq is server-side: {:?}",
            payload.event.change_spine
        );
    }
}

// ── AI: the Change-Spine envelope names no principal but the subscriber ──

const SUBSCRIBER: &str = "5a1e0000-0000-4000-8000-000000000001";
const SOMEONE_ELSE: &str = "5a1e0000-0000-4000-8000-000000000002";

/// Plans `orderCreated { id }` for `who`, subscribes it with `user_context` (the transport's
/// view of the connection: its tenant), publishes `event` and returns the envelope the
/// subscriber received — `None` when the frame carries none.
fn delivered_envelope(
    who: &SecurityContext,
    user_context: serde_json::Value,
    event: crate::runtime::subscription::SubscriptionEvent,
) -> Option<crate::runtime::subscription::ChangeSpineEnvelope> {
    use crate::runtime::subscription::SubscriptionManager;
    let exec = executor(schema(), RuntimeConfig::default());
    let planned =
        plan(&exec, "subscription { orderCreated { id } }", &json!({}), Some(who)).unwrap();
    let manager = SubscriptionManager::new(Arc::new(schema()));
    let mut rx = manager.receiver();
    manager
        .subscribe_planned(Arc::new(planned), user_context, json!({}), "c1", vec![])
        .unwrap();
    assert_eq!(manager.publish_event(event), 1, "the subscriber may read the event");
    rx.try_recv().unwrap().event.change_spine
}

fn order_event() -> crate::runtime::subscription::SubscriptionEvent {
    crate::runtime::subscription::SubscriptionEvent::new(
        "Order",
        "o1",
        crate::runtime::subscription::SubscriptionOperation::Create,
        json!({"id": "o1"}),
    )
}

// An agent acting for someone else wrote the row: the subscriber may read the row, not learn
// who the agent acted for — another principal's identity, which no schema gate governs (a
// type masking its author field would otherwise name the author here). Nor the mutation's
// duration, which rows the subscriber cannot read shape. What describes the event stays.
#[test]
#[ignore = "AI reproduction: changeSpine names another principal and carries the duration"]
fn a_subscriber_is_not_told_whom_another_principals_agent_acted_for() {
    use crate::runtime::subscription::ChangeSpineEnvelope;
    let who = SecurityContext {
        user_id: SUBSCRIBER.into(),
        tenant_id: Some("t1".into()),
        ..principal(&[])
    };
    let event = order_event().with_tenant_id("t1").with_change_spine(ChangeSpineEnvelope {
        actor_type:     Some("ai_agent".into()),
        acting_for:     Some(SOMEONE_ELSE.into()),
        schema_version: Some("v3".into()),
        tenant_id:      Some("t1".into()),
        duration_ms:    Some(12),
        seq:            Some(42),
    });
    assert_eq!(
        delivered_envelope(&who, json!({"tenant_id": "t1"}), event),
        Some(ChangeSpineEnvelope {
            actor_type: Some("ai_agent".into()),
            schema_version: Some("v3".into()),
            tenant_id: Some("t1".into()),
            ..ChangeSpineEnvelope::default()
        }),
    );
}

// The agent acted for the subscriber: that names no one else, so it is kept — "my agent did
// this". The identity is compared as a UUID, not as text.
#[test]
fn a_subscriber_is_told_when_the_agent_acted_for_them() {
    use crate::runtime::subscription::ChangeSpineEnvelope;
    let who = SecurityContext {
        user_id: SUBSCRIBER.into(),
        ..principal(&[])
    };
    let own = SUBSCRIBER.to_uppercase();
    let event = order_event().with_change_spine(ChangeSpineEnvelope {
        actor_type: Some("ai_agent".into()),
        acting_for: Some(own.clone()),
        ..ChangeSpineEnvelope::default()
    });
    let envelope = delivered_envelope(&who, json!({}), event).expect("an envelope");
    assert_eq!(envelope.acting_for, Some(own));
}

// A subscriber with no tenant (single-tenant mode lets it read a tenant-stamped event) is not
// told the event's tenant, which is not its own; and an envelope left with nothing to say is
// not sent at all — the plain `next` frame of an unstamped event.
#[test]
#[ignore = "AI reproduction: changeSpine carries another tenant's id and the duration"]
fn a_subscriber_is_not_told_another_tenants_id_and_an_empty_envelope_is_not_sent() {
    use crate::runtime::subscription::ChangeSpineEnvelope;
    let event = order_event().with_tenant_id("t2").with_change_spine(ChangeSpineEnvelope {
        tenant_id: Some("t2".into()),
        duration_ms: Some(5),
        ..ChangeSpineEnvelope::default()
    });
    assert_eq!(delivered_envelope(&principal(&[]), json!({}), event), None);
}
