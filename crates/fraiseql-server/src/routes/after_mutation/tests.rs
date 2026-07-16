use std::{collections::HashMap, sync::Arc};

use fraiseql_core::schema::{CompiledSchema, MutationDefinition, MutationOperation};
use fraiseql_functions::{
    FunctionDefinition, FunctionModule, FunctionObserver, RuntimeType, TriggerRegistry,
};
use serde_json::json;

use super::*;

fn module(name: &str) -> FunctionModule {
    FunctionModule {
        name:        name.to_string(),
        source_hash: "test".to_string(),
        bytecode:    bytes::Bytes::new(),
        runtime:     RuntimeType::Wasm,
    }
}

fn hooks(triggers: &[(&str, &str)], modules: &[&str]) -> BeforeMutationHooks {
    let definitions: Vec<FunctionDefinition> = triggers
        .iter()
        .map(|(name, trigger)| FunctionDefinition {
            name:        (*name).to_string(),
            trigger:     (*trigger).to_string(),
            runtime:     RuntimeType::Wasm,
            timeout_ms:  None,
            run_as:      None,
            when:        Vec::new(),
            re_runnable: false,
            retry:       None,
        })
        .collect();
    let trigger_registry =
        TriggerRegistry::load_from_definitions(&definitions).expect("valid trigger definitions");
    let module_registry: HashMap<String, FunctionModule> =
        modules.iter().map(|name| ((*name).to_string(), module(name))).collect();
    BeforeMutationHooks::new(trigger_registry, module_registry, Arc::new(FunctionObserver::new()))
}

fn schema_with(name: &str, return_type: &str, operation: MutationOperation) -> CompiledSchema {
    let mut definition = MutationDefinition::new(name, return_type);
    definition.operation = operation;
    let mut schema = CompiledSchema::default();
    schema.mutations.push(definition);
    schema
}

fn insert(table: &str) -> MutationOperation {
    MutationOperation::Insert {
        table: table.to_string(),
    }
}

#[test]
fn event_kind_maps_dml_verbs_and_skips_custom() {
    assert_eq!(event_kind_for(&insert("t")), Some(EventKind::Insert));
    assert_eq!(
        event_kind_for(&MutationOperation::Update {
            table: "t".to_string(),
        }),
        Some(EventKind::Update)
    );
    assert_eq!(
        event_kind_for(&MutationOperation::Delete {
            table: "t".to_string(),
        }),
        Some(EventKind::Delete)
    );
    assert_eq!(event_kind_for(&MutationOperation::Custom), None);
}

#[test]
fn plans_dispatch_for_matching_insert_trigger() {
    let hooks = hooks(&[("onUserCreated", "after:mutation:User:insert")], &["onUserCreated"]);
    let schema = schema_with("createUser", "User", insert("tb_user"));
    let response = json!({ "data": { "createUser": { "id": "u1", "name": "Ada" } } });

    let plans = plan_after_mutation_dispatch(&hooks, &schema, "createUser", &response);

    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].module.name, "onUserCreated");
    // The payload carries the new entity under `data.new`.
    let data = &plans[0].payload.data;
    assert_eq!(data["event_kind"], "insert");
    assert_eq!(data["new"]["name"], "Ada");
    assert!(data["old"].is_null());
}

#[test]
fn delete_reports_entity_as_old_not_new() {
    let hooks = hooks(&[("onUserDeleted", "after:mutation:User:delete")], &["onUserDeleted"]);
    let schema = schema_with(
        "deleteUser",
        "User",
        MutationOperation::Delete {
            table: "tb_user".to_string(),
        },
    );
    let response = json!({ "data": { "deleteUser": { "id": "u1" } } });

    let plans = plan_after_mutation_dispatch(&hooks, &schema, "deleteUser", &response);

    assert_eq!(plans.len(), 1);
    let data = &plans[0].payload.data;
    assert_eq!(data["event_kind"], "delete");
    assert_eq!(data["old"]["id"], "u1");
    assert!(data["new"].is_null());
}

#[test]
fn custom_mutation_emits_no_dispatch() {
    // A trigger keyed on the entity exists, but a Custom op has no event kind.
    let hooks = hooks(&[("onAnything", "after:mutation:Report")], &["onAnything"]);
    let schema = schema_with("generateReport", "Report", MutationOperation::Custom);
    let response = json!({ "data": { "generateReport": { "id": "r1" } } });

    let plans = plan_after_mutation_dispatch(&hooks, &schema, "generateReport", &response);

    assert!(plans.is_empty());
}

#[test]
fn unknown_mutation_emits_no_dispatch() {
    let hooks = hooks(&[("onUserCreated", "after:mutation:User:insert")], &["onUserCreated"]);
    let schema = schema_with("createUser", "User", insert("tb_user"));
    let response = json!({ "data": { "createPost": { "id": "p1" } } });

    let plans = plan_after_mutation_dispatch(&hooks, &schema, "createPost", &response);

    assert!(plans.is_empty());
}

#[test]
fn non_matching_entity_emits_no_dispatch() {
    // Trigger is for Post, but the mutation returns User.
    let hooks = hooks(&[("onPostCreated", "after:mutation:Post:insert")], &["onPostCreated"]);
    let schema = schema_with("createUser", "User", insert("tb_user"));
    let response = json!({ "data": { "createUser": { "id": "u1" } } });

    let plans = plan_after_mutation_dispatch(&hooks, &schema, "createUser", &response);

    assert!(plans.is_empty());
}

#[test]
fn wrong_event_kind_filter_emits_no_dispatch() {
    // Trigger only fires on update; the mutation is an insert.
    let hooks = hooks(&[("onUserUpdated", "after:mutation:User:update")], &["onUserUpdated"]);
    let schema = schema_with("createUser", "User", insert("tb_user"));
    let response = json!({ "data": { "createUser": { "id": "u1" } } });

    let plans = plan_after_mutation_dispatch(&hooks, &schema, "createUser", &response);

    assert!(plans.is_empty());
}

#[test]
fn all_kinds_trigger_matches_insert() {
    // No operation filter → matches every event kind for the entity.
    let hooks = hooks(&[("onUserChange", "after:mutation:User")], &["onUserChange"]);
    let schema = schema_with("createUser", "User", insert("tb_user"));
    let response = json!({ "data": { "createUser": { "id": "u1" } } });

    let plans = plan_after_mutation_dispatch(&hooks, &schema, "createUser", &response);

    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].module.name, "onUserChange");
}

#[test]
fn trigger_without_loaded_module_is_skipped() {
    // Trigger is registered but its module never loaded → dropped, not panicked.
    let hooks = hooks(&[("ghost", "after:mutation:User:insert")], &[]);
    let schema = schema_with("createUser", "User", insert("tb_user"));
    let response = json!({ "data": { "createUser": { "id": "u1" } } });

    let plans = plan_after_mutation_dispatch(&hooks, &schema, "createUser", &response);

    assert!(plans.is_empty());
}

#[test]
fn null_entity_yields_empty_new_but_still_dispatches() {
    let hooks = hooks(&[("onUserCreated", "after:mutation:User:insert")], &["onUserCreated"]);
    let schema = schema_with("createUser", "User", insert("tb_user"));
    let response = json!({ "data": { "createUser": null } });

    let plans = plan_after_mutation_dispatch(&hooks, &schema, "createUser", &response);

    assert_eq!(plans.len(), 1);
    assert!(plans[0].payload.data["new"].is_null());
}

// ── after:ingest planning ───────────────────────────────────────────────────

#[test]
fn plan_after_ingest_matches_source_and_builds_payload() {
    use fraiseql_functions::{InboundMessage, IngestSource};

    let hooks = hooks(
        &[
            ("onStripe", "after:ingest:webhook:stripe"),
            ("onEmail", "after:ingest:email"),
        ],
        &["onStripe", "onEmail"],
    );
    let message = InboundMessage::new(
        IngestSource::Webhook {
            provider: "stripe".to_string(),
        },
        "evt_1",
        chrono::Utc::now(),
    );

    let plans = plan_after_ingest_dispatch(&hooks, &message);

    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].module.name, "onStripe");
    assert_eq!(plans[0].payload.trigger_type, "after:ingest:webhook:stripe");
}

#[test]
fn plan_after_ingest_skips_when_no_trigger_matches() {
    use fraiseql_functions::{InboundMessage, IngestSource};

    let hooks = hooks(&[("onEmail", "after:ingest:email")], &["onEmail"]);
    let message = InboundMessage::new(
        IngestSource::Webhook {
            provider: "stripe".to_string(),
        },
        "evt_1",
        chrono::Utc::now(),
    );

    assert!(plan_after_ingest_dispatch(&hooks, &message).is_empty());
}

// ── Durable dispatch: retry + DLQ + re_runnable opt-out ─────────────────────

#[cfg(feature = "functions-runtime")]
mod durable_dispatch {
    use fraiseql_functions::{EventPayload, FunctionObserver, ResourceLimits};
    use fraiseql_observers::{
        BackoffStrategy, DeadLetterQueue, DispatchPolicy, FailurePolicy, RetryConfig,
    };

    use super::{
        super::{DurableDispatcher, FunctionDispatchSetting, host_context_config},
        module,
    };
    use crate::observers::runtime::InMemoryDlq;

    /// A dispatcher whose observer has no registered runtime, so every
    /// `invoke_with_context` returns a permanent-less `Unsupported` error
    /// (`501` → not a client error → transient), letting durable dispatch retry.
    fn failing_dispatcher(dlq: std::sync::Arc<dyn DeadLetterQueue>) -> DurableDispatcher {
        keyed_failing_dispatcher(dlq, None)
    }

    fn keyed_failing_dispatcher(
        dlq: std::sync::Arc<dyn DeadLetterQueue>,
        idempotency_key: Option<std::sync::Arc<[u8]>>,
    ) -> DurableDispatcher {
        DurableDispatcher {
            observer: std::sync::Arc::new(FunctionObserver::new()),
            host_config: host_context_config(),
            limits: ResourceLimits::default(),
            dlq,
            source: fraiseql_observers::DispatchSource::AfterMutation,
            sender_resolver: None,
            email_transport: None,
            idempotency_key,
            query_executor_factory: None,
            run_as: None,
        }
    }

    fn payload() -> EventPayload {
        EventPayload {
            trigger_type: "after:mutation:onUserCreated".to_string(),
            entity:       "User".to_string(),
            event_kind:   "insert".to_string(),
            data:         serde_json::json!({ "new": { "id": "u1" } }),
            timestamp:    chrono::Utc::now(),
        }
    }

    /// Zero-delay policy so retry tests run without real backoff waits.
    fn zero_delay_policy(max_attempts: u32) -> DispatchPolicy {
        DispatchPolicy::new(
            RetryConfig {
                max_attempts,
                initial_delay_ms: 0,
                max_delay_ms: 0,
                backoff_strategy: BackoffStrategy::Fixed,
            },
            FailurePolicy::Dlq,
        )
    }

    #[tokio::test]
    async fn durable_dispatch_dead_letters_after_exhausting_retries() {
        let dlq = std::sync::Arc::new(InMemoryDlq::new_with_max(None));
        let dispatcher = failing_dispatcher(dlq.clone());
        let setting = FunctionDispatchSetting {
            re_runnable: false,
            policy:      zero_delay_policy(3),
        };

        dispatcher.dispatch(module("onUserCreated"), payload(), &setting).await;

        assert_eq!(dlq.function_count(), 1, "an exhausted durable dispatch lands one DLQ row");
        let pending = dlq.get_pending_functions(10).await.expect("list pending function DLQ");
        assert_eq!(pending[0].function_name, "onUserCreated");
        assert_eq!(pending[0].attempts, 3, "the record captures every attempt made");
        assert_eq!(pending[0].trigger_type, "after:mutation:onUserCreated");
    }

    #[tokio::test]
    async fn dead_letter_carries_the_per_dispatch_idempotency_token() {
        let dlq = std::sync::Arc::new(InMemoryDlq::new_with_max(None));
        let dispatcher = failing_dispatcher(dlq.clone());
        let setting = FunctionDispatchSetting {
            re_runnable: false,
            policy:      zero_delay_policy(2),
        };
        let event = payload();

        dispatcher.dispatch(module("onUserCreated"), event.clone(), &setting).await;

        // The dispatcher derives the token ONCE and passes it to every attempt; it
        // is recorded on the dead-letter, and equals the pure derivation from the
        // dispatch's stable identity (source + function + trigger + payload data).
        // Same-token-every-attempt therefore holds by construction, and the derived
        // value is exactly what the guest's `fraiseql_idempotency_token()` returns.
        let expected = fraiseql_observers::derive_idempotency_token(
            None,
            fraiseql_observers::DispatchSource::AfterMutation,
            "onUserCreated",
            "after:mutation:onUserCreated:User:insert",
            &event.data,
        );
        let pending = dlq.get_pending_functions(10).await.expect("list pending function DLQ");
        assert_eq!(
            pending[0].idempotency_token, expected,
            "the dead-letter carries the derived per-dispatch token"
        );
        assert_eq!(expected.len(), 32, "the token is a 32-char hex send-id");
    }

    #[tokio::test]
    async fn keyed_dispatch_signs_the_idempotency_token() {
        let dlq = std::sync::Arc::new(InMemoryDlq::new_with_max(None));
        let key: std::sync::Arc<[u8]> = std::sync::Arc::from(
            fraiseql_observers::derive_idempotency_subkey(b"root-secret").as_slice(),
        );
        let dispatcher = keyed_failing_dispatcher(dlq.clone(), Some(key.clone()));
        let setting = FunctionDispatchSetting {
            re_runnable: false,
            policy:      zero_delay_policy(2),
        };
        let event = payload();

        dispatcher.dispatch(module("onUserCreated"), event.clone(), &setting).await;

        // With a key configured, the dispatched (and dead-lettered) token is the
        // HMAC-keyed derivation — and differs from the unsigned digest of the same
        // identity, proving the secret is actually applied end-to-end.
        let signed = fraiseql_observers::derive_idempotency_token(
            Some(&key[..]),
            fraiseql_observers::DispatchSource::AfterMutation,
            "onUserCreated",
            "after:mutation:onUserCreated:User:insert",
            &event.data,
        );
        let unsigned = fraiseql_observers::derive_idempotency_token(
            None,
            fraiseql_observers::DispatchSource::AfterMutation,
            "onUserCreated",
            "after:mutation:onUserCreated:User:insert",
            &event.data,
        );
        let pending = dlq.get_pending_functions(10).await.expect("list pending function DLQ");
        assert_eq!(pending[0].idempotency_token, signed, "the dispatch signs the token");
        assert_ne!(pending[0].idempotency_token, unsigned, "signed ≠ unsigned digest");
    }

    #[tokio::test]
    async fn re_runnable_dispatch_does_not_dead_letter() {
        let dlq = std::sync::Arc::new(InMemoryDlq::new_with_max(None));
        let dispatcher = failing_dispatcher(dlq.clone());
        let setting = FunctionDispatchSetting {
            re_runnable: true,
            policy:      zero_delay_policy(3),
        };

        dispatcher.dispatch(module("scoreDeal"), payload(), &setting).await;

        assert_eq!(
            dlq.function_count(),
            0,
            "a re-runnable dispatch is fire-and-forget: no retry, no DLQ"
        );
    }

    #[test]
    fn default_setting_is_durable() {
        // ADR 0015: dispatch is durable by default; re_runnable is the opt-out.
        assert!(!FunctionDispatchSetting::default().re_runnable);
    }
}

// ── Config surface: env overrides + per-function resolution ─────────────────

#[cfg(feature = "functions-runtime")]
mod dispatch_config {
    use std::collections::HashMap;

    use fraiseql_functions::{FunctionDefinition, RuntimeType};
    use fraiseql_observers::RetryConfig;

    use super::super::{DispatchDefaults, DlqStoreKind, resolve_dispatch_settings};

    #[test]
    fn dlq_store_resolves_from_compiled_value_and_env_override() {
        let no_env = |_: &str| None;

        // Compiled value, no env: honoured (case/space-insensitive).
        assert_eq!(DlqStoreKind::resolve(Some("postgres"), no_env), DlqStoreKind::Postgres);
        assert_eq!(DlqStoreKind::resolve(Some("  Postgres "), no_env), DlqStoreKind::Postgres);
        assert_eq!(DlqStoreKind::resolve(Some("memory"), no_env), DlqStoreKind::Memory);

        // Absent everywhere → the in-memory default.
        assert_eq!(DlqStoreKind::resolve(None, no_env), DlqStoreKind::Memory);

        // Env overrides the compiled value (production tuning without recompiling).
        let env_pg =
            |key: &str| (key == "FRAISEQL_FUNCTIONS_DLQ_STORE").then(|| "postgres".to_string());
        assert_eq!(DlqStoreKind::resolve(Some("memory"), env_pg), DlqStoreKind::Postgres);
        let env_mem =
            |key: &str| (key == "FRAISEQL_FUNCTIONS_DLQ_STORE").then(|| "memory".to_string());
        assert_eq!(DlqStoreKind::resolve(Some("postgres"), env_mem), DlqStoreKind::Memory);

        // Unknown value → fail-safe to memory (never a startup failure).
        assert_eq!(DlqStoreKind::resolve(Some("redis"), no_env), DlqStoreKind::Memory);
    }

    fn definition(name: &str, re_runnable: bool, retry: Option<RetryConfig>) -> FunctionDefinition {
        FunctionDefinition {
            name: name.to_string(),
            trigger: format!("after:mutation:Entity:insert@{name}"),
            runtime: RuntimeType::Wasm,
            timeout_ms: None,
            run_as: None,
            when: Vec::new(),
            re_runnable,
            retry,
        }
    }

    #[test]
    fn env_overrides_layer_over_retry_defaults() {
        let env: HashMap<&str, &str> = [
            ("FRAISEQL_FUNCTIONS_RETRY_MAX_ATTEMPTS", "9"),
            ("FRAISEQL_FUNCTIONS_DLQ_MAX_SIZE", "500"),
        ]
        .into_iter()
        .collect();

        let defaults =
            DispatchDefaults::from_getter(|key| env.get(key).map(|value| (*value).to_string()));

        assert_eq!(defaults.retry.max_attempts, 9, "env overrides the default attempts");
        assert_eq!(defaults.dlq_max_size, Some(500), "env sets the DLQ cap");
        // An untouched knob keeps the library default.
        assert_eq!(defaults.retry.initial_delay_ms, RetryConfig::default().initial_delay_ms);
    }

    #[test]
    fn unset_env_uses_library_defaults() {
        let defaults = DispatchDefaults::from_getter(|_| None);
        assert_eq!(defaults.retry.max_attempts, RetryConfig::default().max_attempts);
        assert_eq!(defaults.dlq_max_size, None, "unbounded DLQ when unset");
    }

    #[test]
    fn resolution_maps_re_runnable_and_per_function_retry() {
        let defaults = DispatchDefaults::from_getter(|_| None); // library defaults
        let per_function = RetryConfig {
            max_attempts: 5,
            ..RetryConfig::default()
        };
        let definitions = vec![
            definition("scoreDeal", true, None),
            definition("chargeCard", false, Some(per_function)),
            definition("sendEmail", false, None),
        ];

        let settings = resolve_dispatch_settings(&definitions, &defaults);

        assert!(settings["scoreDeal"].re_runnable, "re_runnable carried through");
        assert!(!settings["chargeCard"].re_runnable);
        assert_eq!(
            settings["chargeCard"].policy.retry.max_attempts, 5,
            "explicit per-function retry is used"
        );
        assert_eq!(
            settings["sendEmail"].policy.retry.max_attempts, defaults.retry.max_attempts,
            "a function with no retry inherits the default"
        );
    }
}

// ── #594: the fraiseql_query bridge wiring on the dispatched host ────────────

#[cfg(feature = "functions-runtime")]
mod query_bridge_wiring {
    #![allow(clippy::unwrap_used)] // Reason: test module — mutex locks are infallible here

    use std::sync::{Arc, Mutex};

    use fraiseql_core::security::SecurityContext;
    use fraiseql_functions::{
        EventPayload, FunctionObserver, HostContext, ResourceLimits, RunAs,
        host::live::QueryExecutor,
    };

    use super::super::{DurableDispatcher, QueryExecutorFactory, host_context_config};
    use crate::observers::runtime::InMemoryDlq;

    /// A mock executor returning a canned result. The identity it runs under is
    /// captured by the factory closure below (that is where the `run_as` identity is
    /// resolved), so this executor itself is stateless.
    struct RecordingExecutor;

    impl QueryExecutor for RecordingExecutor {
        fn execute_query(
            &self,
            _query: &str,
            _variables: Option<&serde_json::Value>,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = fraiseql_error::Result<serde_json::Value>>
                    + Send
                    + '_,
            >,
        > {
            Box::pin(async { Ok(serde_json::json!({ "data": { "ok": true } })) })
        }
    }

    /// Build a factory that records the identity each dispatched host runs under.
    fn recording_factory() -> (QueryExecutorFactory, Arc<Mutex<Option<SecurityContext>>>) {
        let captured: Arc<Mutex<Option<SecurityContext>>> = Arc::new(Mutex::new(None));
        let sink = Arc::clone(&captured);
        let factory: QueryExecutorFactory = Arc::new(move |identity: SecurityContext| {
            *sink.lock().unwrap() = Some(identity);
            Arc::new(RecordingExecutor) as Arc<dyn QueryExecutor>
        });
        (factory, captured)
    }

    fn dispatcher(
        query_executor_factory: Option<QueryExecutorFactory>,
        run_as: Option<RunAs>,
    ) -> DurableDispatcher {
        DurableDispatcher {
            observer: Arc::new(FunctionObserver::new()),
            host_config: host_context_config(),
            limits: ResourceLimits::default(),
            dlq: Arc::new(InMemoryDlq::new_with_max(None)),
            source: fraiseql_observers::DispatchSource::AfterMutation,
            sender_resolver: None,
            email_transport: None,
            idempotency_key: None,
            query_executor_factory,
            run_as,
        }
    }

    fn payload() -> EventPayload {
        EventPayload {
            trigger_type: "after:mutation:recordApproval".to_string(),
            entity:       "Order".to_string(),
            event_kind:   "update".to_string(),
            data:         serde_json::json!({ "new": { "id": 1 } }),
            timestamp:    chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn without_a_factory_the_bridge_is_unconfigured() {
        // No factory ⇒ the dispatched host has no query executor, so a function's
        // `fraiseql_query` fails "query executor not configured" — the pre-#594
        // behavior, preserved for a server with no request-path executor.
        let host = dispatcher(None, None).build_host("notify", payload(), "tok-1");
        let err = host
            .query("mutation { x }", serde_json::json!({}))
            .await
            .expect_err("no executor");
        assert!(err.to_string().contains("query executor not configured"));
    }

    #[tokio::test]
    async fn a_factory_wires_the_bridge_under_the_run_as_ceiling() {
        // #594: with a factory, the dispatched host CAN issue `fraiseql_query`, and
        // it runs under this function's `run_as` identity (audited as
        // `system_job:<function-name>`).
        let (factory, captured) = recording_factory();
        let run_as = RunAs {
            roles:  vec!["order_writer".to_string()],
            scopes: vec!["write:order".to_string()],
            tenant: Some("acme".to_string()),
        };
        let host = dispatcher(Some(factory), Some(run_as)).build_host(
            "recordApproval",
            payload(),
            "tok-2",
        );

        let value = host
            .query("mutation { recordApproval(id: 1) { id } }", serde_json::json!({}))
            .await
            .expect("the bridge is wired");
        assert_eq!(value, serde_json::json!({ "data": { "ok": true } }));

        // The write ran under the function's ceiling, audited as its system job.
        let identity = captured.lock().unwrap().clone().expect("identity captured");
        assert_eq!(identity.user_id.0, "system_job:recordApproval");
        assert!(identity.has_role("order_writer"));
        assert!(identity.has_scope("write:order"));
        assert_eq!(identity.request_id, "tok-2", "identity correlates the dispatch token");
    }

    #[tokio::test]
    async fn a_function_without_run_as_is_fail_closed() {
        // A factory but no `run_as` ⇒ the bridge is wired but runs under an anonymous
        // system_job with no authority: RLS/field-authz deny writes.
        let (factory, captured) = recording_factory();
        let host = dispatcher(Some(factory), None).build_host("purge", payload(), "tok-3");
        let _ = host.query("query { me { id } }", serde_json::json!({})).await;

        let identity = captured.lock().unwrap().clone().expect("identity captured");
        assert_eq!(identity.user_id.0, "system_job:purge");
        assert!(identity.roles.is_empty(), "fail-closed: no roles");
        assert!(identity.scopes.is_empty(), "fail-closed: no scopes");
        assert!(identity.tenant_id.is_none(), "fail-closed: no tenant");
    }
}

// ── #597: `when` predicates gate the planner (no dispatch on non-match) ──────

#[test]
fn when_predicate_produces_no_dispatch_on_non_matching_update() {
    use fraiseql_functions::{RuntimeType, triggers::TriggerPredicate};

    // A function that only fires when `status` transitions to "approved".
    let mut def = FunctionDefinition::new(
        "notify_approved",
        "after:mutation:Order:update",
        RuntimeType::Wasm,
    );
    def.when = vec![TriggerPredicate {
        field:      "status".to_string(),
        eq:         None,
        changed_to: Some(json!("approved")),
    }];

    let registry = TriggerRegistry::load_from_definitions(&[def]).expect("valid when");
    let module_registry: HashMap<String, FunctionModule> =
        HashMap::from([("notify_approved".to_string(), module("notify_approved"))]);
    let hooks =
        BeforeMutationHooks::new(registry, module_registry, Arc::new(FunctionObserver::new()));

    let schema = schema_with(
        "updateOrder",
        "Order",
        MutationOperation::Update {
            table: "tb_order".to_string(),
        },
    );

    // The after:mutation ROUTE path carries only the after-image (no pre-image), so
    // `changed_to` gates on `new.status == v`. An update whose result is NOT the
    // target value produces no dispatch record at all (predicate false).
    let other_value = json!({ "data": { "updateOrder": { "id": "o1", "status": "rejected" } } });
    assert!(
        plan_after_mutation_dispatch(&hooks, &schema, "updateOrder", &other_value).is_empty(),
        "a predicate-false update produces no dispatch (no record, not a skipped dispatch)"
    );

    // An update to the target value fires. (On the route path the pre-image is
    // absent, so `changed_to` cannot distinguish a transition from a re-save — full
    // transition detection needs the pre-image, i.e. the after:capture path with
    // `pre_image=True`. Documented in functions.md.)
    let to_target = json!({ "data": { "updateOrder": { "id": "o1", "status": "approved" } } });
    assert_eq!(
        plan_after_mutation_dispatch(&hooks, &schema, "updateOrder", &to_target).len(),
        1,
        "an update to the target value fires the predicate function"
    );
}

// ── #366: after:capture dispatch (loop-safe, predicate-aware) ───────────────

#[cfg(feature = "functions-runtime")]
mod after_capture {
    #![allow(clippy::unwrap_used)] // Reason: test module

    use fraiseql_functions::RuntimeType;
    use fraiseql_observers::{
        DispatchSource, EntityEvent as ObserverEntityEvent, EventKind as ObserverEventKind,
    };

    use super::{
        super::{
            CAPTURED_WRITE_MARKER, dispatch_idempotency_token, observer_event_to_capture,
            plan_after_capture_dispatch,
        },
        *,
    };

    fn capture_hooks(name: &str, trigger: &str) -> BeforeMutationHooks {
        let def = FunctionDefinition::new(name, trigger, RuntimeType::Wasm);
        let registry =
            TriggerRegistry::load_from_definitions(&[def]).expect("valid capture trigger");
        let modules: HashMap<String, FunctionModule> =
            HashMap::from([(name.to_string(), module(name))]);
        BeforeMutationHooks::new(registry, modules, Arc::new(FunctionObserver::new()))
    }

    fn captured_insert(
        entity: &str,
        data: serde_json::Value,
        cdc: Option<&str>,
    ) -> ObserverEntityEvent {
        let mut e = ObserverEntityEvent::new(
            ObserverEventKind::Created,
            entity.to_string(),
            uuid::Uuid::new_v4(),
            data,
        );
        e.cdc_source = cdc.map(String::from);
        e
    }

    #[test]
    fn dispatches_only_genuinely_captured_writes() {
        let hooks = capture_hooks("onCapture", "after:capture:Order:insert");
        let data = json!({ "id": "o1", "status": "new" });

        // A captured write (cdc_source = fallback_trigger) → one after:capture plan.
        let captured = captured_insert("Order", data.clone(), Some(CAPTURED_WRITE_MARKER));
        let (event, cdc) = observer_event_to_capture(&captured).expect("insert converts");
        let plans = plan_after_capture_dispatch(&hooks, &event, cdc.as_deref());
        assert_eq!(plans.len(), 1, "a captured write drives after:capture");
        assert_eq!(plans[0].payload.trigger_type, "after:capture:onCapture");
        assert_eq!(plans[0].payload.data["new"]["id"], "o1");

        // An executor/mediated write (no marker) → NO dispatch (loop safety).
        let executor_write = captured_insert("Order", data, None);
        let (event, cdc) = observer_event_to_capture(&executor_write).expect("insert converts");
        assert!(
            plan_after_capture_dispatch(&hooks, &event, cdc.as_deref()).is_empty(),
            "a non-captured (executor) write never dispatches after:capture — loop safety"
        );
    }

    #[test]
    fn delete_reports_removed_row_as_old() {
        let hooks = capture_hooks("onDelete", "after:capture:Order:delete");
        let mut ev = ObserverEntityEvent::new(
            ObserverEventKind::Deleted,
            "Order".to_string(),
            uuid::Uuid::new_v4(),
            json!({ "id": "o9" }),
        );
        ev.cdc_source = Some(CAPTURED_WRITE_MARKER.to_string());

        let (event, cdc) = observer_event_to_capture(&ev).expect("delete converts");
        let plans = plan_after_capture_dispatch(&hooks, &event, cdc.as_deref());
        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].payload.data["old"]["id"], "o9",
            "delete reports the removed row as old"
        );
        assert!(plans[0].payload.data["new"].is_null());
    }

    #[test]
    fn predicates_compose_on_capture_payloads() {
        use fraiseql_functions::triggers::TriggerPredicate;
        // A capture function that only fires when status == "shipped".
        let mut def =
            FunctionDefinition::new("onShipped", "after:capture:Order:insert", RuntimeType::Wasm);
        def.when = vec![TriggerPredicate {
            field:      "status".to_string(),
            eq:         Some(json!("shipped")),
            changed_to: None,
        }];
        let registry = TriggerRegistry::load_from_definitions(&[def]).expect("valid");
        let modules: HashMap<String, FunctionModule> =
            HashMap::from([("onShipped".to_string(), module("onShipped"))]);
        let hooks = BeforeMutationHooks::new(registry, modules, Arc::new(FunctionObserver::new()));

        // status=new → predicate false → no dispatch.
        let e = captured_insert("Order", json!({ "status": "new" }), Some(CAPTURED_WRITE_MARKER));
        let (ev, cdc) = observer_event_to_capture(&e).unwrap();
        assert!(plan_after_capture_dispatch(&hooks, &ev, cdc.as_deref()).is_empty());

        // status=shipped → predicate true → dispatch.
        let e =
            captured_insert("Order", json!({ "status": "shipped" }), Some(CAPTURED_WRITE_MARKER));
        let (ev, cdc) = observer_event_to_capture(&e).unwrap();
        assert_eq!(plan_after_capture_dispatch(&hooks, &ev, cdc.as_deref()).len(), 1);
    }

    // #366: a capture dispatch's idempotency token is stable per change-log row, so a
    // redelivery after a crash (the reader re-processing the same row) re-derives the
    // same token — the at-most-once guarantee for a money/mail after:capture side
    // effect. The token keys on the row image (entity + event kind + old/new), never
    // the processing timestamp or the event id.
    #[test]
    fn capture_dispatch_token_is_stable_per_change_log_row() {
        let hooks = capture_hooks("reconcile", "after:capture:Order:insert");

        // Derive the dispatch token for a captured row via the real capture path.
        // Each call mints a fresh event id + timestamp (as a redelivery would), so an
        // equal token proves the derivation ignores them.
        let token = |data: serde_json::Value| -> String {
            let event = captured_insert("Order", data, Some(CAPTURED_WRITE_MARKER));
            let (fn_event, cdc) = observer_event_to_capture(&event).expect("capture event");
            let plans = plan_after_capture_dispatch(&hooks, &fn_event, cdc.as_deref());
            let plan = plans.first().expect("one capture plan");
            dispatch_idempotency_token(
                None,
                DispatchSource::AfterCapture,
                "reconcile",
                &plan.payload,
            )
        };

        let row = json!({ "id": "o-1", "status": "approved" });
        let first = token(row.clone());
        let redelivery = token(row); // same row image, fresh event id + timestamp
        assert_eq!(
            first, redelivery,
            "redelivery of the same captured row re-derives the same token (at-most-once)"
        );

        // A genuinely different row image derives a different token.
        let other = token(json!({ "id": "o-2", "status": "approved" }));
        assert_ne!(first, other, "a different captured row derives a different token");
    }
}
