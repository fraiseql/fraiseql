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
        DurableDispatcher {
            observer: std::sync::Arc::new(FunctionObserver::new()),
            host_config: host_context_config(),
            limits: ResourceLimits::default(),
            dlq,
            source: fraiseql_observers::DispatchSource::AfterMutation,
            sender_resolver: None,
            email_transport: None,
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

    use super::super::{DispatchDefaults, resolve_dispatch_settings};

    fn definition(name: &str, re_runnable: bool, retry: Option<RetryConfig>) -> FunctionDefinition {
        FunctionDefinition {
            name: name.to_string(),
            trigger: format!("after:mutation:Entity:insert@{name}"),
            runtime: RuntimeType::Wasm,
            timeout_ms: None,
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
