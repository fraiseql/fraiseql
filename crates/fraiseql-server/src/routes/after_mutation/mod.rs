//! After-mutation function-trigger dispatch (#460).
//!
//! When a GraphQL or REST mutation commits, the server looks up any matching
//! `after:mutation` function triggers and dispatches each as a fire-and-forget
//! task. Failures are logged; they never affect the mutation response.
//!
//! The work is split in two:
//!
//! - `plan_after_mutation_dispatch` — pure, always-compiled, and unit-tested. It maps a completed
//!   mutation to `(entity, event_kind)`, finds matching triggers, resolves their modules, and
//!   builds the event payloads. It has no side effects and needs no function runtime.
//! - `spawn_after_mutation` — gated behind `functions-runtime`. It runs each plan on a live,
//!   I/O-capable host context (`LiveHostContext`) via `FunctionObserver::invoke_with_context`, so
//!   side-effecting functions (webhooks, external provisioning) can reach the network.
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

/// Per-function dispatch settings resolved from the compiled schema.
///
/// Durable dispatch is the default (see ADR 0015): a transient failure is
/// retried per [`policy`](Self::policy) and, on exhaustion, dead-lettered.
/// Setting [`re_runnable`](Self::re_runnable) opts a function out into
/// fire-and-forget dispatch with no retry or dead-letter overhead.
#[cfg(feature = "functions-runtime")]
#[derive(Debug, Clone)]
pub struct FunctionDispatchSetting {
    /// Fire-and-forget (no retry, no DLQ) when `true`.
    pub re_runnable: bool,
    /// Retry + failure policy applied to durable dispatch.
    pub policy:      fraiseql_observers::DispatchPolicy,
}

#[cfg(feature = "functions-runtime")]
impl Default for FunctionDispatchSetting {
    /// Durable-by-default: retry per the default [`RetryConfig`] and dead-letter
    /// on exhaustion.
    ///
    /// [`RetryConfig`]: fraiseql_observers::RetryConfig
    fn default() -> Self {
        Self {
            re_runnable: false,
            policy:      fraiseql_observers::DispatchPolicy::new(
                fraiseql_observers::RetryConfig::default(),
                fraiseql_observers::FailurePolicy::Dlq,
            ),
        }
    }
}

/// Runs after:mutation function plans durably: retry transient failures with
/// backoff and dead-letter what exhausts its retries, unless the function is
/// marked re-runnable (then a single fire-and-forget attempt).
#[cfg(feature = "functions-runtime")]
#[derive(Clone)]
struct DurableDispatcher {
    observer:    std::sync::Arc<fraiseql_functions::FunctionObserver>,
    host_config: fraiseql_functions::host::live::HostContextConfig,
    limits:      fraiseql_functions::ResourceLimits,
    dlq:         std::sync::Arc<dyn fraiseql_observers::DeadLetterQueue>,
}

#[cfg(feature = "functions-runtime")]
impl DurableDispatcher {
    /// Run one function on a fresh live host context.
    async fn invoke_once(
        &self,
        module: &FunctionModule,
        payload: EventPayload,
    ) -> fraiseql_error::Result<fraiseql_functions::FunctionResult> {
        let host: std::sync::Arc<
            dyn fraiseql_functions::runtime::wasm::host_bridge::DynHostContext,
        > = std::sync::Arc::new(fraiseql_functions::host::live::LiveHostContext::new(
            payload.clone(),
            self.host_config.clone(),
        ));
        self.observer.invoke_with_context(module, payload, host, self.limits.clone()).await
    }

    /// Dispatch a single plan under its [`FunctionDispatchSetting`].
    ///
    /// Re-runnable → one attempt, errors logged and dropped. Durable → retry
    /// transient failures per the policy; on a permanent error or exhausted
    /// retries, dead-letter the invocation so it is inspectable and replayable.
    async fn dispatch(
        &self,
        module: FunctionModule,
        payload: EventPayload,
        setting: &FunctionDispatchSetting,
    ) {
        let function_name = module.name.clone();

        if setting.re_runnable {
            // Fire-and-forget: a single attempt; a failure is re-runnable later
            // by design, so it is logged but never retried or dead-lettered.
            match self.invoke_once(&module, payload).await {
                Ok(_) => tracing::debug!(
                    function = %function_name,
                    "re-runnable after:mutation function dispatched"
                ),
                Err(error) => tracing::warn!(
                    error = %error,
                    function = %function_name,
                    "re-runnable after:mutation function failed (not retried)"
                ),
            }
            return;
        }

        // Durable dispatch: retry transient failures with backoff.
        let trigger_type = payload.trigger_type.clone();
        let attempts = std::sync::atomic::AtomicU32::new(0);
        let result = fraiseql_observers::run_with_retry(
            &setting.policy,
            // A 4xx client error (e.g. a malformed payload) will not succeed on
            // retry; everything else (5xx, timeouts, execution failures) is
            // treated as transient and retried.
            |error: &fraiseql_error::FraiseQLError| !error.is_client_error(),
            |n| {
                attempts.store(n, std::sync::atomic::Ordering::Relaxed);
                // Clone per attempt so the retry closure stays `FnMut`; the
                // bytecode is `bytes::Bytes` (ref-counted), so this is cheap.
                let attempt_module = module.clone();
                let attempt_payload = payload.clone();
                async move { self.invoke_once(&attempt_module, attempt_payload).await }
            },
        )
        .await;

        let Err(error) = result else {
            tracing::debug!(function = %function_name, "after:mutation function dispatched");
            return;
        };

        // Exhausted (or permanently failed): dead-letter for inspection/replay.
        let attempts = attempts.load(std::sync::atomic::Ordering::Relaxed);
        let record = fraiseql_observers::FunctionDispatchRecord::new(
            fraiseql_observers::DispatchSource::AfterMutation,
            function_name.clone(),
            trigger_type,
            serde_json::to_value(&payload).unwrap_or(serde_json::Value::Null),
            error.to_string(),
            attempts,
        );
        match self.dlq.push_function(record).await {
            Ok(_) => tracing::error!(
                error = %error,
                function = %function_name,
                attempts,
                "after:mutation function dead-lettered after exhausting retries"
            ),
            Err(dlq_error) => tracing::error!(
                error = %error,
                dlq_error = %dlq_error,
                function = %function_name,
                "after:mutation function failed and could not be dead-lettered"
            ),
        }
    }
}

/// Spawn each planned after:mutation invocation as a background task.
///
/// Each task runs its module on a [`LiveHostContext`] so the function can perform
/// outbound I/O (HTTP, with the SSRF allowlist from
/// `FRAISEQL_FUNCTIONS_ALLOWED_DOMAINS`). Dispatch is durable by default —
/// transient failures are retried and exhausted ones are dead-lettered
/// (`hooks.dlq`) — unless the function is marked `re_runnable`, in which case it
/// stays fire-and-forget. Either way the mutation response has already been sent,
/// so nothing here propagates back to the client.
///
/// Per-function settings come from `hooks.dispatch_settings`; a function absent
/// from that map uses the durable [`FunctionDispatchSetting::default`].
///
/// [`LiveHostContext`]: fraiseql_functions::host::live::LiveHostContext
#[cfg(feature = "functions-runtime")]
pub fn spawn_after_mutation(hooks: &BeforeMutationHooks, plans: Vec<AfterMutationDispatch>) {
    let dispatcher = DurableDispatcher {
        observer:    std::sync::Arc::clone(&hooks.observer),
        host_config: host_context_config(),
        limits:      fraiseql_functions::ResourceLimits::default(),
        dlq:         std::sync::Arc::clone(&hooks.dlq),
    };

    for plan in plans {
        let setting =
            hooks.dispatch_settings.get(&plan.module.name).cloned().unwrap_or_default();
        let dispatcher = dispatcher.clone();
        tokio::spawn(async move {
            dispatcher.dispatch(plan.module, plan.payload, &setting).await;
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
mod tests;
