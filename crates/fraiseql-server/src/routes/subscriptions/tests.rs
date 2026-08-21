//! Tests for subscription tenant resolution (#331).
//!
//! The `WebSocket` upgrade must mirror the GraphQL handler's tenant dispatch — JWT
//! `tenant_id` precedence, strict cross-source conflict rejection, and
//! Host-domain resolution — rather than the former `None, None, false` call that
//! silently dropped all three.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // Reason: test code.

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
        super::{
            LiveSubscriptionPolicies, build_subscription_policies, derive_policy_conditions,
            resolve_subscription_rls,
        },
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

    /// #611 layer-1: a policy ADDED by a hot-reload applies to a NEW subscription — the live
    /// source is read per subscribe, and the newly-scoped entity fails closed for an
    /// unresolvable identity rather than staying deliver-all until restart.
    #[tokio::test]
    async fn policy_added_by_reload_applies_to_new_subscription_fail_closed() {
        use arc_swap::ArcSwap;
        // A live source the test swaps to simulate a hot-reload (exactly how the `/ws`
        // handler reads the reload-aware executor ArcSwap in production).
        let live_swap: Arc<ArcSwap<HashMap<String, SubscriptionPolicy>>> =
            Arc::new(ArcSwap::from_pointee(HashMap::new()));
        let live_swap_reader = live_swap.clone();
        let live: LiveSubscriptionPolicies = Arc::new(move || live_swap_reader.load_full());

        let manager = Arc::new(SubscriptionManager::new(Arc::new(CompiledSchema::default())));
        let state = SubscriptionState::new(manager).with_live_subscription_policies(Some(live));

        // Before the reload: no policy → unscoped (deliver-all), even for an anonymous sub.
        let before = resolve_subscription_rls(&state, "orderUpdated", None).await;
        assert_eq!(before.expect("no policy → ok"), Vec::new());

        // Reload adds a policy for orderUpdated.
        let mut reloaded = HashMap::new();
        reloaded.insert("orderUpdated".to_string(), policy());
        live_swap.store(Arc::new(reloaded));

        // A NEW anonymous subscription now sees the added policy → refused (fail-closed),
        // NOT deliver-all. This is the layer-1 guarantee that closes the fail-open window.
        let after = resolve_subscription_rls(&state, "orderUpdated", None).await;
        assert!(
            after.is_err(),
            "a hot-reloaded policy must apply to new subscriptions (fail-closed): {after:?}"
        );
    }

    /// #611 layer-1: the live source overrides the mount-time snapshot. A policy REMOVED by a
    /// hot-reload leaves new subscriptions unscoped (the operator's explicit choice), rather
    /// than keeping the stale snapshot's scoping.
    #[tokio::test]
    async fn live_source_overrides_mount_time_snapshot() {
        // Mount-time snapshot HAS a policy; the live source (post-reload) does NOT.
        let mut snapshot = HashMap::new();
        snapshot.insert("orderUpdated".to_string(), policy());
        let live_empty: LiveSubscriptionPolicies = Arc::new(|| Arc::new(HashMap::new()));

        let manager = Arc::new(SubscriptionManager::new(Arc::new(CompiledSchema::default())));
        let state = SubscriptionState::new(manager)
            .with_subscription_policies(Arc::new(snapshot))
            .with_live_subscription_policies(Some(live_empty));

        // The live (empty) source wins over the snapshot → unscoped, not the snapshot's refuse.
        let r = resolve_subscription_rls(&state, "orderUpdated", None).await;
        assert_eq!(
            r.expect("live source removed the policy → unscoped"),
            Vec::new(),
            "the live source must override the mount-time snapshot"
        );
    }

    /// Without a live source, behavior is unchanged: the mount-time snapshot is used and
    /// fails closed for a policy-declaring subscription with an anonymous principal.
    #[tokio::test]
    async fn no_live_source_falls_back_to_snapshot_fail_closed() {
        let mut snapshot = HashMap::new();
        snapshot.insert("orderUpdated".to_string(), policy());
        let manager = Arc::new(SubscriptionManager::new(Arc::new(CompiledSchema::default())));
        let state = SubscriptionState::new(manager).with_subscription_policies(Arc::new(snapshot));

        let r = resolve_subscription_rls(&state, "orderUpdated", None).await;
        assert!(r.is_err(), "snapshot policy still fails closed for an anonymous sub: {r:?}");
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
        let msg = create_next_message("op_1", "orderUpdated", &payload_with(Some(env)));
        let payload = msg.payload.expect("next payload");
        // Resolved data is untouched under the client's response key.
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
        let msg = create_next_message("op_1", "orderUpdated", &payload_with(None));
        let payload = msg.payload.expect("next payload");
        assert_eq!(payload["data"]["orderUpdated"]["status"], "PAID");
        assert!(
            payload.get("extensions").is_none(),
            "events without an envelope keep the plain next payload (back-compat)"
        );
    }

    /// #906: the frame is keyed by the client's response key, which is the root
    /// field's alias when it wrote one — not by the subscription's own name.
    #[test]
    fn keys_the_frame_by_the_clients_response_key() {
        let msg = create_next_message("op_1", "order", &payload_with(None));
        let payload = msg.payload.expect("next payload");
        assert_eq!(
            payload["data"]["order"]["status"], "PAID",
            "the alias must be the response key: {payload}"
        );
        assert!(
            payload["data"].get("orderUpdated").is_none(),
            "the subscription's own name must not appear when an alias was given: {payload}"
        );
    }
}

// =============================================================================
// Document validation on `/ws` (#1154)
// =============================================================================

mod document_validation {
    use fraiseql_core::schema::{
        AutoParams, CompiledSchema, FieldDefinition, FieldType, QueryDefinition,
        SubscriptionDefinition, TypeDefinition,
    };

    use super::super::{
        SubscriptionDocumentError, extract_subscription_root, validate_subscription_variables,
    };

    /// A schema that can adjudicate § 5.8.2: it carries input-type information,
    /// so an unknown type name is a positive contradiction rather than an
    /// absence of evidence.
    fn schema() -> CompiledSchema {
        let mut schema = CompiledSchema::default();
        let mut order = TypeDefinition::new("Order", "v_order");
        order.fields.push(FieldDefinition::new("id", FieldType::Id));
        order.fields.push(FieldDefinition::new("status", FieldType::String));
        schema.types.push(order);

        let mut orders = QueryDefinition::new("orders", "Order");
        orders.returns_list = true;
        orders.auto_params = AutoParams::all();
        schema.queries.push(orders);

        schema.subscriptions.push(SubscriptionDefinition::new("orderUpdated", "Order"));
        schema.build_indexes();
        schema
    }

    /// Refuse a document and return `(code, message)`.
    fn refuse(query: &str) -> (&'static str, String) {
        let schema = schema();
        let err = extract_subscription_root(query)
            .and_then(|(_, parsed)| validate_subscription_variables(&parsed, Some(&schema)))
            .expect_err("this document must be refused");
        (err.code(), err.message().to_string())
    }

    fn accept(query: &str) {
        let schema = schema();
        extract_subscription_root(query)
            .and_then(|(_, parsed)| validate_subscription_variables(&parsed, Some(&schema)))
            .unwrap_or_else(|e| panic!("this document must be accepted: {}", e.message()));
    }

    /// § 5.8.3 — the load-bearing one. Before this, `/ws` accepted the
    /// subscription and the argument carrying `$nope` was silently dropped.
    #[test]
    fn a_variable_the_subscription_never_defines_is_refused_as_a_validation_error() {
        let (code, message) = refuse("subscription S { orderUpdated(id: $nope) { id } }");
        assert_eq!(code, "VALIDATION_ERROR", "a well-formed document is not a parse failure");
        assert!(message.contains("$nope"), "message was: {message}");
    }

    /// § 5.8.2.
    #[test]
    fn a_variable_typed_with_a_name_the_schema_does_not_publish_is_refused() {
        let (code, message) =
            refuse("subscription S($w: NoSuchTypeAtAll) { orderUpdated(where: $w) { id } }");
        assert_eq!(code, "VALIDATION_ERROR");
        assert!(message.contains("NoSuchTypeAtAll"), "message was: {message}");
    }

    /// § 5.8.4.
    #[test]
    fn a_variable_the_subscription_never_uses_is_refused() {
        let (code, message) = refuse("subscription S($unused: ID) { orderUpdated { id } }");
        assert_eq!(code, "VALIDATION_ERROR");
        assert!(message.contains("$unused"), "message was: {message}");
    }

    #[test]
    fn a_valid_subscription_still_establishes() {
        accept("subscription S($id: ID) { orderUpdated(id: $id) { id status } }");
        accept("subscription { orderUpdated { id } }");
    }

    /// **The scope guard.** `extract_subscription_root` selects the *subscription*
    /// operation. Reaching for `parse_query` would take the document's first
    /// operation and validate `Q` — leaving `S`, the one that runs, unchecked.
    #[test]
    fn the_subscription_operation_is_validated_not_the_documents_first_operation() {
        let (code, message) = refuse(
            "query Q($ok: ID) { orders(limit: 1) { id } } \
             subscription S { orderUpdated(id: $neverDefined) { id } }",
        );
        assert_eq!(code, "VALIDATION_ERROR");
        assert!(
            message.contains("$neverDefined"),
            "the *subscription*'s variable must be the one reported: {message}"
        );
    }

    /// The structural guards keep reporting a **parse** error: a document one
    /// connection operation cannot serve is not a schema disagreement.
    #[test]
    fn the_multi_operation_and_multi_root_guards_stay_parse_errors() {
        for query in [
            "subscription A { orderUpdated { id } } subscription B { orderUpdated { id } }",
            "subscription S { orderUpdated { id } orderDeleted { id } }",
            "query Q { orders { id } }",
            "subscription S { orderUpdated { id }",
        ] {
            let schema = schema();
            let err = extract_subscription_root(query)
                .and_then(|(_, parsed)| validate_subscription_variables(&parsed, Some(&schema)))
                .expect_err("must be refused");
            assert_eq!(err.code(), "PARSE_ERROR", "for: {query}");
        }
    }

    /// Fail-open where the schema is absent: the two schema-free rules still
    /// run, and § 5.8.2 — which needs a published surface to contradict — does
    /// not guess.
    #[test]
    fn without_a_schema_the_two_schema_free_rules_still_run() {
        let (_, parsed) = extract_subscription_root(
            "subscription S($w: NoSuchTypeAtAll) { orderUpdated(where: $w) { id } }",
        )
        .expect("parses");
        validate_subscription_variables(&parsed, None)
            .expect("§ 5.8.2 cannot be adjudicated without a schema");

        let (_, parsed) =
            extract_subscription_root("subscription S { orderUpdated(id: $nope) { id } }")
                .expect("parses");
        assert!(
            matches!(
                validate_subscription_variables(&parsed, None),
                Err(SubscriptionDocumentError::Validation(_))
            ),
            "§ 5.8.3 needs no schema, so it must still run"
        );
    }

    /// **The test that matters.** The two surfaces must refuse the same document
    /// with the same message — a client that moves a document from `/graphql` to
    /// `/ws` should not discover a different set of rules.
    #[test]
    fn graphql_and_ws_refuse_the_same_document_with_the_same_message() {
        use fraiseql_core::{
            graphql::parse_query,
            runtime::{
                collect_variable_references, validate_variable_types, validate_variable_uses,
                validate_variables_used,
            },
        };

        let schema = schema();
        // One document per rule, written as a subscription so both surfaces see
        // the identical operation.
        for document in [
            "subscription S { orderUpdated(id: $nope) { id } }",
            "subscription S($w: NoSuchTypeAtAll) { orderUpdated(where: $w) { id } }",
            "subscription S($unused: ID) { orderUpdated { id } }",
        ] {
            // What `/graphql` would say, through the same validators
            // `classify_query_with_parse` runs, in the same order.
            let parsed = parse_query(document).expect("parses");
            let operation_name = parsed.operation_name.as_deref();
            let defined: Vec<String> = parsed.variables.iter().map(|v| v.name.clone()).collect();
            let referenced = collect_variable_references(&parsed).expect("walk succeeds");
            let http = validate_variable_uses(operation_name, &defined, &referenced)
                .and_then(|()| validate_variable_types(&schema, operation_name, &parsed.variables))
                .and_then(|()| validate_variables_used(operation_name, &defined, &referenced))
                .expect_err("the /graphql surface refuses this document");

            let (code, ws) = refuse(document);
            assert_eq!(code, "VALIDATION_ERROR", "for: {document}");
            assert_eq!(
                ws,
                http.to_string(),
                "the two surfaces must refuse `{document}` identically"
            );
        }
    }
}
