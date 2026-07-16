//! Tests for subscription tenant resolution (#331).
//!
//! The `WebSocket` upgrade must mirror the GraphQL handler's tenant dispatch — JWT
//! `tenant_id` precedence, strict cross-source conflict rejection, and
//! Host-domain resolution — rather than the former `None, None, false` call that
//! silently dropped all three.

#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code.

use std::{collections::HashMap, sync::Arc};

use axum::http::{HeaderMap, HeaderName, HeaderValue};
use chrono::Utc;
use fraiseql_core::{
    runtime::SubscriptionManager,
    schema::CompiledSchema,
    security::SecurityContext,
    types::{TenantId, UserId},
};

use super::{DomainRegistry, SubscriptionState, resolve_subscription_tenant};

/// Build a `SubscriptionState` with the given strict flag and Host→key mappings.
fn state(strict: bool, domains: &[(&str, &str)]) -> SubscriptionState {
    let manager = Arc::new(SubscriptionManager::new(Arc::new(CompiledSchema::default())));
    let registry = DomainRegistry::new();
    for (host, key) in domains {
        registry.register(*host, *key);
    }
    SubscriptionState::new(manager).with_tenant_context(Arc::new(registry), strict)
}

/// A minimal `SecurityContext` carrying a JWT `tenant_id`.
fn ctx_with_tenant(tenant: &str) -> SecurityContext {
    SecurityContext {
        user_id:          UserId::new("user-1"),
        roles:            vec![],
        tenant_id:        Some(TenantId::new(tenant)),
        scopes:           vec![],
        attributes:       HashMap::new(),
        request_id:       "req-test".to_string(),
        ip_address:       None,
        authenticated_at: Utc::now(),
        expires_at:       Utc::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
    let mut h = HeaderMap::new();
    for (k, v) in pairs {
        let name: HeaderName = k.parse().expect("valid header name");
        let value: HeaderValue = v.parse().expect("valid header value");
        h.insert(name, value);
    }
    h
}

#[test]
fn jwt_tenant_takes_precedence_over_header() {
    let ctx = ctx_with_tenant("bar");
    let h = headers(&[("X-Tenant-ID", "foo")]);
    let resolved = resolve_subscription_tenant(Some(&ctx), &h, &state(false, &[])).unwrap();
    assert_eq!(
        resolved.as_deref(),
        Some("bar"),
        "JWT tenant_id must win over the X-Tenant-ID header (was dropped pre-#331)",
    );
}

#[test]
fn strict_rejects_conflicting_jwt_and_header() {
    let ctx = ctx_with_tenant("bar");
    let h = headers(&[("X-Tenant-ID", "foo")]);
    let result = resolve_subscription_tenant(Some(&ctx), &h, &state(true, &[]));
    assert!(result.is_err(), "strict mode must reject a JWT/header tenant conflict");
}

#[test]
fn header_resolves_when_no_jwt() {
    let h = headers(&[("X-Tenant-ID", "foo")]);
    let resolved = resolve_subscription_tenant(None, &h, &state(false, &[])).unwrap();
    assert_eq!(resolved.as_deref(), Some("foo"));
}

#[test]
fn domain_registry_is_consulted_under_strict() {
    // Host maps to "bar"; the header says "foo" → conflict → strict Err. Proves
    // the domain registry is now threaded into the subscription path (was None).
    let h = headers(&[("X-Tenant-ID", "foo"), ("Host", "tenant-a.example.com")]);
    let result =
        resolve_subscription_tenant(None, &h, &state(true, &[("tenant-a.example.com", "bar")]));
    assert!(result.is_err(), "Host-vs-header conflict must be rejected under strict mode");
}

#[test]
fn no_tenant_sources_resolves_to_none() {
    let resolved =
        resolve_subscription_tenant(None, &HeaderMap::new(), &state(false, &[])).unwrap();
    assert!(resolved.is_none());
}

/// Row-level visibility policy derivation on the `/ws` seam (#596). Exercises the
/// security-critical adapter (`derive_policy_conditions`) and the mount-time policy-map
/// builder (`build_subscription_policies`) directly — the same enforcement point both
/// WS subprotocols route through in `handle_client_message`.
mod row_visibility_596 {
    use fraiseql_core::{
        schema::{SubscriptionDefinition, SubscriptionPolicy, TypeDefinition},
        security::ENRICHED_NAMESPACE_PREFIX,
    };

    use super::{
        super::{build_subscription_policies, derive_policy_conditions},
        *,
    };

    fn policy() -> SubscriptionPolicy {
        SubscriptionPolicy {
            owner_path:     "$.owner_id".to_string(),
            identity_field: "user_id".to_string(),
            bypass_roles:   vec!["admin".to_string()],
        }
    }

    /// A `SecurityContext` for user `sub`, with optional server-resolved enriched fields
    /// and roles.
    fn principal(sub: &str, enriched: &[(&str, &str)], roles: &[&str]) -> SecurityContext {
        let mut attributes = HashMap::new();
        for (field, value) in enriched {
            attributes.insert(
                format!("{ENRICHED_NAMESPACE_PREFIX}{field}"),
                serde_json::Value::String((*value).to_string()),
            );
        }
        SecurityContext {
            user_id: UserId::new(sub),
            roles: roles.iter().map(|r| (*r).to_string()).collect(),
            tenant_id: None,
            scopes: vec![],
            attributes,
            request_id: "req-596".to_string(),
            ip_address: None,
            authenticated_at: Utc::now(),
            expires_at: Utc::now(),
            issuer: None,
            audience: None,
            email: None,
            display_name: None,
        }
    }

    #[test]
    fn resolvable_identity_yields_a_server_owned_owner_condition() {
        let ctx = principal("alice", &[("user_id", "alice")], &[]);
        let conds =
            derive_policy_conditions(&policy(), Some(&ctx)).expect("resolvable → conditions");
        assert_eq!(conds, vec![("owner_id".to_string(), serde_json::json!("alice"))]);
    }

    #[test]
    fn bypass_role_gets_full_visibility_no_condition() {
        let ctx = principal("root", &[("user_id", "root")], &["admin"]);
        let conds = derive_policy_conditions(&policy(), Some(&ctx)).expect("bypass → ok");
        assert!(conds.is_empty(), "a bypass role adds no owner condition (full visibility)");
    }

    #[test]
    fn anonymous_subscriber_is_refused_fail_closed() {
        // No principal at all → cannot resolve the owner → refuse (never deliver-all).
        assert!(
            derive_policy_conditions(&policy(), None).is_err(),
            "an anonymous subscriber must be refused for a policy-declaring entity"
        );
    }

    #[test]
    fn enrichment_outage_or_missing_field_is_refused_fail_closed() {
        // Authenticated principal, but enrichment produced no `user_id` field (outage,
        // denial, or NULL) → refuse rather than deliver every row.
        let ctx = principal("alice", &[], &[]);
        assert!(
            derive_policy_conditions(&policy(), Some(&ctx)).is_err(),
            "an unresolvable enriched identity must refuse the subscription"
        );
    }

    #[test]
    fn forged_plain_attribute_cannot_widen_visibility() {
        // A client that smuggles a plain (non-enriched) `user_id` attribute must not
        // resolve the owner — the derivation reads ONLY the server-resolved
        // `fraiseql.enriched.*` namespace.
        let mut ctx = principal("mallory", &[], &[]);
        ctx.attributes.insert("user_id".to_string(), serde_json::json!("victim"));
        assert!(
            derive_policy_conditions(&policy(), Some(&ctx)).is_err(),
            "a forgeable plain attribute must not resolve the owner identity"
        );
    }

    #[test]
    fn build_map_keys_policies_by_subscription_field() {
        let schema = CompiledSchema {
            types: vec![
                TypeDefinition::new("Order", "v_order").with_subscription_policy(policy()),
                TypeDefinition::new("Ping", "v_ping"), // no policy
            ],
            subscriptions: vec![
                SubscriptionDefinition::new("orderUpdated", "Order"),
                SubscriptionDefinition::new("pinged", "Ping"),
            ],
            ..Default::default()
        };
        let map = build_subscription_policies(&schema);
        assert!(map.contains_key("orderUpdated"), "policy-declaring entity is mapped");
        assert!(!map.contains_key("pinged"), "an entity without a policy is not mapped");
        assert_eq!(map.len(), 1);
    }
}

/// `create_next_message` wire-contract (#425): the Change-Spine envelope rides in
/// the graphql-transport-ws `extensions.changeSpine` slot, leaving `data` untouched;
/// events without an envelope keep the plain payload.
mod create_next_message_tests {
    use fraiseql_core::runtime::subscription::{
        ChangeSpineEnvelope, SubscriptionEvent, SubscriptionId, SubscriptionOperation,
        SubscriptionPayload,
    };

    use super::super::create_next_message;

    fn payload_with(envelope: Option<ChangeSpineEnvelope>) -> SubscriptionPayload {
        let mut event = SubscriptionEvent::new(
            "Order",
            "ord_1",
            SubscriptionOperation::Update,
            serde_json::json!({ "id": "ord_1" }),
        );
        if let Some(env) = envelope {
            event = event.with_change_spine(env);
        }
        SubscriptionPayload {
            subscription_id: SubscriptionId::new(),
            subscription_name: "orderUpdated".to_string(),
            event,
            data: serde_json::json!({ "id": "ord_1", "status": "PAID" }),
        }
    }

    #[test]
    fn attaches_envelope_under_extensions_change_spine() {
        let env = ChangeSpineEnvelope {
            actor_type: Some("human_user".to_string()),
            schema_version: Some("v3".to_string()),
            seq: Some(42),
            ..Default::default()
        };
        let msg = create_next_message("op_1", &payload_with(Some(env)));
        let payload = msg.payload.expect("next payload");
        // Resolved data is untouched under `data.<subscriptionName>`.
        assert_eq!(payload["data"]["orderUpdated"]["status"], "PAID");
        // Envelope rides in extensions.changeSpine, camelCase, nulls omitted.
        let cs = &payload["extensions"]["changeSpine"];
        assert_eq!(cs["actorType"], "human_user");
        assert_eq!(cs["schemaVersion"], "v3");
        assert_eq!(cs["seq"], 42);
        assert!(cs.get("actingFor").is_none(), "unset envelope fields are omitted");
    }

    #[test]
    fn no_envelope_emits_no_extensions() {
        let msg = create_next_message("op_1", &payload_with(None));
        let payload = msg.payload.expect("next payload");
        assert_eq!(payload["data"]["orderUpdated"]["status"], "PAID");
        assert!(
            payload.get("extensions").is_none(),
            "events without an envelope keep the plain next payload (back-compat)"
        );
    }
}
