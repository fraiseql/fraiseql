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

/// Plan the `after:ingest` dispatch for a persisted inbound message.
///
/// Finds the `after:ingest[:<source>]` triggers matching the message, pairs each
/// with its function module, and builds the event payload (the normalized
/// message as JSON). Like [`plan_after_mutation_dispatch`] this is pure,
/// always-compiled, and unit-tested: it needs no function runtime and returns an
/// empty vector when no trigger matches (the common fast path).
pub fn plan_after_ingest_dispatch(
    hooks: &BeforeMutationHooks,
    message: &fraiseql_functions::InboundMessage,
) -> Vec<AfterMutationDispatch> {
    hooks
        .trigger_registry
        .find_ingest_triggers(message)
        .into_iter()
        .filter_map(|trigger| {
            // A trigger whose module never loaded is silently skipped.
            let module = hooks.module_registry.get(&trigger.function_name)?.clone();
            let payload = trigger.build_payload(message);
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

/// Server-level defaults for durable dispatch, layered under per-function
/// settings from the compiled schema.
///
/// Built from the environment so production can tune durability without
/// recompiling the schema (mirroring `FRAISEQL_FUNCTIONS_ALLOWED_DOMAINS`):
///
/// - `FRAISEQL_FUNCTIONS_RETRY_MAX_ATTEMPTS` — default retry attempts.
/// - `FRAISEQL_FUNCTIONS_RETRY_INITIAL_DELAY_MS` — default initial backoff.
/// - `FRAISEQL_FUNCTIONS_RETRY_MAX_DELAY_MS` — default backoff cap.
/// - `FRAISEQL_FUNCTIONS_DLQ_MAX_SIZE` — dead-letter queue retention cap (unset = unbounded).
#[cfg(feature = "functions-runtime")]
#[derive(Debug, Clone)]
pub struct DispatchDefaults {
    /// Default retry policy for functions without an explicit `retry`.
    pub retry:        fraiseql_observers::RetryConfig,
    /// Dead-letter queue retention cap (`None` = unbounded).
    pub dlq_max_size: Option<usize>,
}

#[cfg(feature = "functions-runtime")]
impl DispatchDefaults {
    /// Read the defaults from the process environment.
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_getter(|key| std::env::var(key).ok())
    }

    /// Read the defaults from an arbitrary key→value getter.
    ///
    /// Factored out from [`from_env`](Self::from_env) so the env layering is unit
    /// testable without mutating global process state. An unset or unparseable
    /// variable leaves the corresponding default untouched.
    #[must_use]
    pub fn from_getter(get: impl Fn(&str) -> Option<String>) -> Self {
        let mut retry = fraiseql_observers::RetryConfig::default();
        if let Some(value) =
            get("FRAISEQL_FUNCTIONS_RETRY_MAX_ATTEMPTS").and_then(|s| s.parse().ok())
        {
            retry.max_attempts = value;
        }
        if let Some(value) =
            get("FRAISEQL_FUNCTIONS_RETRY_INITIAL_DELAY_MS").and_then(|s| s.parse().ok())
        {
            retry.initial_delay_ms = value;
        }
        if let Some(value) =
            get("FRAISEQL_FUNCTIONS_RETRY_MAX_DELAY_MS").and_then(|s| s.parse().ok())
        {
            retry.max_delay_ms = value;
        }
        let dlq_max_size = get("FRAISEQL_FUNCTIONS_DLQ_MAX_SIZE").and_then(|s| s.parse().ok());
        Self {
            retry,
            dlq_max_size,
        }
    }
}

/// Resolve per-function [`FunctionDispatchSetting`]s from the compiled schema.
///
/// Each function's `re_runnable` flag and optional `retry` policy come from its
/// [`FunctionDefinition`](fraiseql_functions::FunctionDefinition); a function
/// with no explicit `retry` inherits `defaults.retry`. The result keys settings
/// by function name for [`spawn_after_mutation`] to look up.
#[cfg(feature = "functions-runtime")]
#[must_use]
pub fn resolve_dispatch_settings(
    definitions: &[fraiseql_functions::FunctionDefinition],
    defaults: &DispatchDefaults,
) -> std::collections::HashMap<String, FunctionDispatchSetting> {
    definitions
        .iter()
        .map(|definition| {
            let retry = definition.retry.clone().unwrap_or_else(|| defaults.retry.clone());
            let setting = FunctionDispatchSetting {
                re_runnable: definition.re_runnable,
                policy:      fraiseql_observers::DispatchPolicy::new(
                    retry,
                    fraiseql_observers::FailurePolicy::Dlq,
                ),
            };
            (definition.name.clone(), setting)
        })
        .collect()
}

/// Runs after:mutation function plans durably: retry transient failures with
/// backoff and dead-letter what exhausts its retries, unless the function is
/// marked re-runnable (then a single fire-and-forget attempt).
#[cfg(feature = "functions-runtime")]
#[derive(Clone)]
struct DurableDispatcher {
    observer:        std::sync::Arc<fraiseql_functions::FunctionObserver>,
    host_config:     fraiseql_functions::host::live::HostContextConfig,
    limits:          fraiseql_functions::ResourceLimits,
    dlq:             std::sync::Arc<dyn fraiseql_observers::DeadLetterQueue>,
    /// Which trigger subsystem this dispatcher serves — tags dead-letter records
    /// so `after:mutation` and `after:ingest` failures are distinguishable.
    source:          fraiseql_observers::DispatchSource,
    /// Host-owned sender-identity resolver + email transport for the `send_email`
    /// op. `None` → the op fails loud on the built host. Threaded from the hooks so
    /// every dispatched function's fresh host can send from the connected user's
    /// verified address.
    sender_resolver: Option<std::sync::Arc<dyn fraiseql_functions::SenderIdentityResolver>>,
    email_transport: Option<std::sync::Arc<dyn fraiseql_functions::EmailTransport>>,
}

#[cfg(feature = "functions-runtime")]
impl DurableDispatcher {
    /// Run one function on a fresh live host context.
    ///
    /// `idempotency_token` is derived once per dispatch and passed to every retry
    /// attempt, so the fresh host each attempt builds carries the *same* token —
    /// the guest observes a stable, per-dispatch idempotency key across retries.
    async fn invoke_once(
        &self,
        module: &FunctionModule,
        payload: EventPayload,
        idempotency_token: &str,
    ) -> fraiseql_error::Result<fraiseql_functions::FunctionResult> {
        // Shared, runtime-agnostic host bridge: the observer dispatches the plan
        // to the WASM or Deno backend by the module's runtime, so the host type
        // must not be tied to either.
        let mut live = fraiseql_functions::host::live::LiveHostContext::new(
            payload.clone(),
            self.host_config.clone(),
        )
        .with_idempotency_token(idempotency_token);
        // Enable `send_email` (host-owned `from` + transport) when both are wired.
        if let (Some(resolver), Some(transport)) =
            (self.sender_resolver.as_ref(), self.email_transport.as_ref())
        {
            live =
                live.with_email(std::sync::Arc::clone(resolver), std::sync::Arc::clone(transport));
        }
        let host: std::sync::Arc<dyn fraiseql_functions::host::dyn_context::DynHostContext> =
            std::sync::Arc::new(live);
        self.observer
            .invoke_with_context(module, payload, host, self.limits.clone())
            .await
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

        // Derive the per-dispatch idempotency token ONCE, from the dispatch's
        // stable identity (never wall-clock/random), so every retry attempt below
        // sees the same token and a durable retry of a money/mail call stays
        // at-most-once. The trigger identity folds in entity + event_kind; the
        // payload data (which excludes the event timestamp) makes it resume-stable.
        let trigger_identity =
            format!("{}:{}:{}", payload.trigger_type, payload.entity, payload.event_kind);
        let idempotency_token = fraiseql_observers::derive_idempotency_token(
            self.source,
            &function_name,
            &trigger_identity,
            &payload.data,
        );

        if setting.re_runnable {
            // Fire-and-forget: a single attempt; a failure is re-runnable later
            // by design, so it is logged but never retried or dead-lettered.
            match self.invoke_once(&module, payload, &idempotency_token).await {
                Ok(_) => tracing::debug!(
                    function = %function_name,
                    "re-runnable function dispatched"
                ),
                Err(error) => tracing::warn!(
                    error = %error,
                    function = %function_name,
                    "re-runnable function failed (not retried)"
                ),
            }
            return;
        }

        // Durable dispatch: retry transient failures with backoff.
        let trigger_type = payload.trigger_type.clone();
        let attempts = std::sync::atomic::AtomicU32::new(0);
        let result =
            fraiseql_observers::run_with_retry(
                &setting.policy,
                // A 4xx client error (e.g. a malformed payload) will not succeed on
                // retry; everything else (5xx, timeouts, execution failures) is
                // treated as transient and retried.
                |error: &fraiseql_error::FraiseQLError| !error.is_client_error(),
                |n| {
                    attempts.store(n, std::sync::atomic::Ordering::Relaxed);
                    // Clone per attempt so the retry closure stays `FnMut`; the
                    // bytecode is `bytes::Bytes` (ref-counted), so this is cheap. The
                    // token is derived once above, so every attempt shares it.
                    let attempt_module = module.clone();
                    let attempt_payload = payload.clone();
                    let attempt_token = idempotency_token.clone();
                    async move {
                        self.invoke_once(&attempt_module, attempt_payload, &attempt_token).await
                    }
                },
            )
            .await;

        let Err(error) = result else {
            tracing::debug!(function = %function_name, "function dispatched");
            return;
        };

        // Exhausted (or permanently failed): dead-letter for inspection/replay.
        let attempts = attempts.load(std::sync::atomic::Ordering::Relaxed);
        let record = fraiseql_observers::FunctionDispatchRecord::new(
            self.source,
            function_name.clone(),
            trigger_type,
            idempotency_token,
            serde_json::to_value(&payload).unwrap_or(serde_json::Value::Null),
            error.to_string(),
            attempts,
        );
        match self.dlq.push_function(record).await {
            Ok(_) => tracing::error!(
                error = %error,
                function = %function_name,
                attempts,
                "function dead-lettered after exhausting retries"
            ),
            Err(dlq_error) => tracing::error!(
                error = %error,
                dlq_error = %dlq_error,
                function = %function_name,
                "function failed and could not be dead-lettered"
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
    spawn_dispatch(hooks, plans, fraiseql_observers::DispatchSource::AfterMutation);
}

/// Spawn each planned `after:ingest` invocation as a background task.
///
/// The inbound-ingestion analogue of [`spawn_after_mutation`]: it runs each
/// function on the same I/O-capable [`LiveHostContext`] with the same durability
/// (retry + dead-letter, or fire-and-forget for `re_runnable` functions), so an
/// `after:ingest` handler can classify a message and issue a mutation with the
/// same reliability guarantees. Dead-letter records are tagged
/// [`DispatchSource::AfterIngest`](fraiseql_observers::DispatchSource::AfterIngest).
///
/// [`LiveHostContext`]: fraiseql_functions::host::live::LiveHostContext
#[cfg(feature = "functions-runtime")]
pub fn spawn_after_ingest(hooks: &BeforeMutationHooks, plans: Vec<AfterMutationDispatch>) {
    spawn_dispatch(hooks, plans, fraiseql_observers::DispatchSource::AfterIngest);
}

/// Spawn each plan on a durable dispatcher tagged with `source`.
#[cfg(feature = "functions-runtime")]
fn spawn_dispatch(
    hooks: &BeforeMutationHooks,
    plans: Vec<AfterMutationDispatch>,
    source: fraiseql_observers::DispatchSource,
) {
    let dispatcher = DurableDispatcher {
        observer: std::sync::Arc::clone(&hooks.observer),
        host_config: host_context_config(),
        limits: fraiseql_functions::ResourceLimits::default(),
        dlq: std::sync::Arc::clone(&hooks.dlq),
        source,
        sender_resolver: hooks.sender_resolver.clone(),
        email_transport: hooks.email_transport.clone(),
    };

    for plan in plans {
        let setting = hooks.dispatch_settings.get(&plan.module.name).cloned().unwrap_or_default();
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
