//! After-mutation function-trigger dispatch (#460).
//!
//! When a GraphQL or REST mutation commits, the server looks up any matching
//! `after:mutation` function triggers and dispatches each as a fire-and-forget
//! task. Failures are logged; they never affect the mutation response.
//!
//! The work is split in two:
//!
//! - `plan_after_mutation_dispatch` — pure, always-compiled, and unit-tested. It
//!   maps a completed mutation to `(entity, event_kind)`, finds matching
//!   triggers, resolves their modules, and builds the event payloads. It has no
//!   side effects and needs no function runtime.
//! - `spawn_after_mutation` — gated behind `functions-runtime`. It runs each plan
//!   on a live, I/O-capable host context (`LiveHostContext`) via
//!   `FunctionObserver::invoke_with_context`, so side-effecting functions
//!   (webhooks, external provisioning) can reach the network.
//!
//! A stock server binary compiles only the planner; the runtime + live host
//! context are opt-in (see the crate's `functions-runtime` feature).

use fraiseql_core::schema::{CompiledSchema, MutationOperation};
use fraiseql_functions::{EntityEvent, EventKind, EventPayload, FunctionModule};

use crate::subsystems::BeforeMutationHooks;

/// A single resolved after:mutation invocation: the module to run and the event
/// payload to run it with.
pub struct AfterMutationDispatch {
    /// The function module to execute.
    pub module:  FunctionModule,
    /// The event payload (`after:mutation:<fn>` with `{event_kind, old, new}`).
    pub payload: EventPayload,
}

/// Map a mutation's SQL operation to the after:mutation [`EventKind`] it emits.
///
/// `Custom` mutations have no insert/update/delete semantics, so they produce no
/// after:mutation event and return `None`.
pub const fn event_kind_for(operation: &MutationOperation) -> Option<EventKind> {
    match operation {
        MutationOperation::Insert { .. } => Some(EventKind::Insert),
        MutationOperation::Update { .. } => Some(EventKind::Update),
        MutationOperation::Delete { .. } => Some(EventKind::Delete),
        // Custom (and any future non-DML variant) emits no entity event.
        _ => None,
    }
}

/// Plan the after:mutation dispatch for a committed mutation.
///
/// Resolves the mutation definition (→ entity type + DML verb), builds the
/// [`EntityEvent`] from the response, finds matching `after:mutation` triggers,
/// and pairs each with its function module. Returns an empty vector when the
/// operation is not a state-changing mutation, the mutation is unknown, or no
/// trigger matches — all of which are the common, allocation-cheap fast path.
///
/// `response_data` is the full GraphQL execution result (`{"data": {...}}`); the
/// affected entity is read from `data.<mutation_name>`.
pub fn plan_after_mutation_dispatch(
    hooks: &BeforeMutationHooks,
    schema: &CompiledSchema,
    mutation_name: &str,
    response_data: &serde_json::Value,
) -> Vec<AfterMutationDispatch> {
    let Some(definition) = schema.find_mutation(mutation_name) else {
        return Vec::new();
    };
    let Some(event_kind) = event_kind_for(&definition.operation) else {
        return Vec::new();
    };

    // The affected entity is flattened under `data.<mutation_name>` in the
    // GraphQL response. A null result (e.g. a no-op delete) carries no entity.
    let entity_value = response_data
        .get("data")
        .and_then(|data| data.get(mutation_name))
        .filter(|value| !value.is_null())
        .cloned();

    // A delete reports the removed row as the *old* state; insert/update report
    // the resulting row as the *new* state. The complementary pre-image is not
    // available on this path, so it stays `None`.
    let (old, new) = match event_kind {
        EventKind::Delete => (entity_value, None),
        _ => (None, entity_value),
    };

    let event = EntityEvent {
        entity: definition.return_type.clone(),
        event_kind,
        old,
        new,
        timestamp: chrono::Utc::now(),
    };

    hooks
        .observer
        .find_after_mutation_triggers(&hooks.trigger_registry, &event)
        .into_iter()
        .filter_map(|trigger| {
            // A trigger whose module never loaded is silently skipped: dispatch
            // is best-effort and must not block the response.
            let module = hooks.module_registry.get(&trigger.function_name)?.clone();
            let payload = trigger.build_payload(&event);
            Some(AfterMutationDispatch { module, payload })
        })
        .collect()
}

/// Spawn each planned after:mutation invocation as a fire-and-forget task.
///
/// Each task runs its module on a [`LiveHostContext`] so the function can perform
/// outbound I/O (HTTP, with the SSRF allowlist from
/// `FRAISEQL_FUNCTIONS_ALLOWED_DOMAINS`). Errors are logged, never propagated —
/// the mutation response has already been sent.
///
/// [`LiveHostContext`]: fraiseql_functions::host::live::LiveHostContext
#[cfg(feature = "functions-runtime")]
pub fn spawn_after_mutation(hooks: &BeforeMutationHooks, plans: Vec<AfterMutationDispatch>) {
    let config = host_context_config();
    let limits = fraiseql_functions::ResourceLimits::default();

    for plan in plans {
        let observer = std::sync::Arc::clone(&hooks.observer);
        let config = config.clone();
        let limits = limits.clone();
        tokio::spawn(async move {
            let function_name = plan.module.name.clone();
            let host: std::sync::Arc<
                dyn fraiseql_functions::runtime::wasm::host_bridge::DynHostContext,
            > = std::sync::Arc::new(fraiseql_functions::host::live::LiveHostContext::new(
                plan.payload.clone(),
                config,
            ));
            match observer.invoke_with_context(&plan.module, plan.payload, host, limits).await {
                Ok(_) => {
                    tracing::debug!(function = %function_name, "after:mutation function dispatched");
                },
                Err(error) => {
                    tracing::error!(
                        error = %error,
                        function = %function_name,
                        "after:mutation function failed",
                    );
                },
            }
        });
    }
}

/// Build the host-context config for after:mutation functions.
///
/// Outbound HTTP is deny-by-default; the SSRF allowlist is sourced from the
/// comma-separated `FRAISEQL_FUNCTIONS_ALLOWED_DOMAINS` environment variable so
/// production can grant outbound access without recompiling the schema.
#[cfg(feature = "functions-runtime")]
fn host_context_config() -> fraiseql_functions::host::live::HostContextConfig {
    let mut config = fraiseql_functions::host::live::HostContextConfig::default();
    if let Ok(domains) = std::env::var("FRAISEQL_FUNCTIONS_ALLOWED_DOMAINS") {
        config.allowed_domains = domains
            .split(',')
            .map(str::trim)
            .filter(|domain| !domain.is_empty())
            .map(String::from)
            .collect();
    }
    config
}

#[cfg(test)]
mod tests {
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
                name:       (*name).to_string(),
                trigger:    (*trigger).to_string(),
                runtime:    RuntimeType::Wasm,
                timeout_ms: None,
            })
            .collect();
        let trigger_registry =
            TriggerRegistry::load_from_definitions(&definitions).expect("valid trigger definitions");
        let module_registry: HashMap<String, FunctionModule> =
            modules.iter().map(|name| ((*name).to_string(), module(name))).collect();
        BeforeMutationHooks {
            trigger_registry,
            module_registry,
            observer: Arc::new(FunctionObserver::new()),
        }
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
}
