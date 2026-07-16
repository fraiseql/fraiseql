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
        // #597: evaluate the trigger's `when` predicates on the row images BEFORE
        // resolving the module or building a payload — a false predicate produces no
        // dispatch record at all (not a skipped/failed dispatch), and no runtime spins.
        .filter(|trigger| trigger.predicates_hold(&event))
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
// Reason: the after:ingest planner is inbound-only (its callers live behind
// `inbound`/`inbound-email`), yet stays always-compiled so its pure logic is
// unit-tested without the runtime. A functions-runtime build without `inbound`
// (e.g. `sources`) therefore legitimately has no caller.
#[cfg_attr(not(feature = "inbound"), allow(dead_code))]
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

/// The captured-write discriminator (#366): a row written by the shipped
/// external-write capture trigger carries this `cdc_source` marker; FraiseQL's own
/// executor-written rows do not. `after:capture` dispatch keys on it so a
/// mediated/executor write never re-enters the capture path (loop safety).
pub const CAPTURED_WRITE_MARKER: &str = "fallback_trigger";

/// Plan the `after:capture` dispatch for an externally-captured change (#366).
///
/// Mirrors [`plan_after_mutation_dispatch`] but resolves `after:capture` triggers
/// (from the change-log reader, not the mutation route) and builds an
/// `after:capture:<fn>` payload. **Loop safety:** returns an empty plan unless the
/// event is a genuinely-captured write (`cdc_source == "fallback_trigger"`) — an
/// executor/mediated write (no marker) never dispatches, so a Phase-02 bridge write
/// from a capture-dispatched function cannot re-enter the capture path.
///
/// The [`when`](fraiseql_functions::FunctionDefinition::when) predicates (Phase 04)
/// are evaluated identically to the mutation path. Payload contract: `new` = the
/// after-image, `old` = the pre-image when the event carried one (else `None` —
/// "degraded but valid"); no mutation name, no input echo.
#[must_use]
pub fn plan_after_capture_dispatch(
    hooks: &BeforeMutationHooks,
    event: &EntityEvent,
    cdc_source: Option<&str>,
) -> Vec<AfterMutationDispatch> {
    // Only genuinely-captured writes drive after:capture — the loop-safety gate.
    if cdc_source != Some(CAPTURED_WRITE_MARKER) {
        return Vec::new();
    }

    hooks
        .observer
        .find_after_capture_triggers(&hooks.trigger_registry, event)
        .into_iter()
        // Phase 04 predicates evaluate identically on the capture payload.
        .filter(|trigger| trigger.predicates_hold(event))
        .filter_map(|trigger| {
            let module = hooks.module_registry.get(&trigger.function_name)?.clone();
            // Capture payload: `after:capture:<fn>` with {event_kind, old, new}.
            let payload = EventPayload {
                trigger_type: format!("after:capture:{}", trigger.function_name),
                entity:       event.entity.clone(),
                event_kind:   event.event_kind.to_string(),
                data:         serde_json::json!({
                    "event_kind": event.event_kind.as_str(),
                    "old": event.old,
                    "new": event.new,
                }),
                timestamp:    event.timestamp,
            };
            Some(AfterMutationDispatch { module, payload })
        })
        .collect()
}

/// Convert a change-log reader [`EntityEvent`](fraiseql_observers::EntityEvent) to
/// the functions [`EntityEvent`] used by after:capture dispatch, plus its
/// `cdc_source` (#366).
///
/// `new` = the after-image (`data`) for INSERT/UPDATE; a DELETE reports the removed
/// row as `old`. A `Custom` event has no entity-event semantics and yields `None`.
/// The full pre-image for an UPDATE is not carried on this path, so `old` stays
/// `None` there (the same pre-image limitation the after:mutation route path has —
/// see `functions.md`); `changed_to` on capture-updates therefore gates on the
/// after-value.
#[cfg(feature = "functions-runtime")]
#[must_use]
pub fn observer_event_to_capture(
    event: &fraiseql_observers::EntityEvent,
) -> Option<(EntityEvent, Option<String>)> {
    use fraiseql_observers::EventKind as ObserverEventKind;
    let event_kind = match event.event_type {
        ObserverEventKind::Created => EventKind::Insert,
        ObserverEventKind::Updated => EventKind::Update,
        ObserverEventKind::Deleted => EventKind::Delete,
        // A non-DML custom event (or any future kind) has no insert/update/delete
        // semantics — no after:capture entity event.
        _ => return None,
    };
    let (old, new) = match event_kind {
        EventKind::Delete => (Some(event.data.clone()), None),
        _ => (None, Some(event.data.clone())),
    };
    let fn_event = EntityEvent {
        entity: event.entity_type.clone(),
        event_kind,
        old,
        new,
        timestamp: event.timestamp,
    };
    Some((fn_event, event.cdc_source.clone()))
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

/// A `run_as`-parameterized `fraiseql_query` executor builder (#594).
///
/// Given a per-dispatch [`SecurityContext`](fraiseql_core::security::SecurityContext)
/// (the function's `run_as` identity), produces the
/// [`QueryExecutor`](fraiseql_functions::host::live::QueryExecutor) the dispatched
/// host's `fraiseql_query` runs through. Type-erased over the database adapter so the
/// non-generic dispatcher can hold it; built at the route layer where the adapter is
/// known via [`make_query_executor_factory`].
#[cfg(feature = "functions-runtime")]
pub type QueryExecutorFactory = std::sync::Arc<
    dyn Fn(
            fraiseql_core::security::SecurityContext,
        ) -> std::sync::Arc<dyn fraiseql_functions::host::live::QueryExecutor>
        + Send
        + Sync,
>;

/// Build a [`QueryExecutorFactory`] over the request-path executor handle (#594).
///
/// Captures the hot-reloadable `Arc<ArcSwap<Executor<A>>>` so each dispatched
/// function's `fraiseql_query` runs against the current schema snapshot under its own
/// `run_as` identity — the same [`RunAsQueryExecutor`](crate::query_bridge::RunAsQueryExecutor)
/// scheduled sources use. Called from the route handlers (which know the adapter `A`)
/// and passed into [`spawn_after_mutation`] / [`spawn_after_ingest`].
#[cfg(feature = "functions-runtime")]
#[must_use]
pub fn make_query_executor_factory<A>(
    executor: std::sync::Arc<arc_swap::ArcSwap<fraiseql_core::runtime::Executor<A>>>,
) -> QueryExecutorFactory
where
    A: fraiseql_core::db::traits::DatabaseAdapter + 'static,
{
    std::sync::Arc::new(move |identity| {
        std::sync::Arc::new(crate::query_bridge::RunAsQueryExecutor::new(
            std::sync::Arc::clone(&executor),
            identity,
        )) as std::sync::Arc<dyn fraiseql_functions::host::live::QueryExecutor>
    })
}

/// Runs after:mutation function plans durably: retry transient failures with
/// backoff and dead-letter what exhausts its retries, unless the function is
/// marked re-runnable (then a single fire-and-forget attempt).
#[cfg(feature = "functions-runtime")]
#[derive(Clone)]
struct DurableDispatcher {
    observer:               std::sync::Arc<fraiseql_functions::FunctionObserver>,
    host_config:            fraiseql_functions::host::live::HostContextConfig,
    limits:                 fraiseql_functions::ResourceLimits,
    dlq:                    std::sync::Arc<dyn fraiseql_observers::DeadLetterQueue>,
    /// Which trigger subsystem this dispatcher serves — tags dead-letter records
    /// so `after:mutation` and `after:ingest` failures are distinguishable.
    source:                 fraiseql_observers::DispatchSource,
    /// Host-owned sender-identity resolver + email transport for the `send_email`
    /// op. `None` → the op fails loud on the built host. Threaded from the hooks so
    /// every dispatched function's fresh host can send from the connected user's
    /// verified address.
    sender_resolver:        Option<std::sync::Arc<dyn fraiseql_functions::SenderIdentityResolver>>,
    email_transport:        Option<std::sync::Arc<dyn fraiseql_functions::EmailTransport>>,
    /// HMAC subkey for the per-dispatch idempotency token. `Some` → the token is
    /// signed (unforgeable, required before it is exposed in a VERP Return-Path);
    /// `None` → an unsigned digest (the zero-config default). Derived once from the
    /// server HMAC secret and shared across every dispatch.
    idempotency_key:        Option<std::sync::Arc<[u8]>>,
    /// The `fraiseql_query` bridge builder (#594). `Some` → the dispatched host can
    /// issue queries/mutations under this function's `run_as` ceiling; `None` → no
    /// executor is wired (`fraiseql_query` fails "query executor not configured", the
    /// pre-#594 behavior for a server with no request-path executor).
    query_executor_factory: Option<QueryExecutorFactory>,
    /// This function's `run_as` ceiling (#594). Absent ⇒ fail-closed: the bridge runs
    /// under an anonymous `system_job` identity and RLS/field-authz deny writes.
    /// Resolved per-plan from the function definition at spawn time.
    run_as:                 Option<fraiseql_functions::RunAs>,
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
        let live = self.build_host(&module.name, payload.clone(), idempotency_token);
        let host: std::sync::Arc<dyn fraiseql_functions::host::dyn_context::DynHostContext> =
            std::sync::Arc::new(live);
        self.observer
            .invoke_with_context(module, payload, host, self.limits.clone())
            .await
    }

    /// Build the fresh live host for one dispatch of `function_name`.
    ///
    /// Wires the per-dispatch idempotency token, the `send_email` op (when
    /// configured), and — the #594 change — the `fraiseql_query` bridge under the
    /// function's `run_as` ceiling when a [`QueryExecutorFactory`] is present. The
    /// identity is built from [`run_as`](Self::run_as) via
    /// [`RunAs::identity`](fraiseql_functions::RunAs::identity), so an absent ceiling
    /// yields a fail-closed anonymous `system_job` identity (reads/writes denied by
    /// RLS/field-authz) and the write is audited as `system_job:<function_name>`.
    ///
    /// Extracted from [`invoke_once`](Self::invoke_once) so the host-wiring is unit
    /// testable without spinning a V8/WASM isolate (mirrors the sources poller's
    /// `build_host`).
    fn build_host(
        &self,
        function_name: &str,
        payload: EventPayload,
        idempotency_token: &str,
    ) -> fraiseql_functions::host::live::LiveHostContext {
        let mut live =
            fraiseql_functions::host::live::LiveHostContext::new(payload, self.host_config.clone())
                .with_idempotency_token(idempotency_token);
        // Enable `send_email` (host-owned `from` + transport) when both are wired.
        if let (Some(resolver), Some(transport)) =
            (self.sender_resolver.as_ref(), self.email_transport.as_ref())
        {
            live =
                live.with_email(std::sync::Arc::clone(resolver), std::sync::Arc::clone(transport));
        }
        // #594: wire the `fraiseql_query` bridge under this function's `run_as`
        // ceiling. The request-path `run_as` executor is the same seam scheduled
        // sources use; an absent ceiling is fail-closed (anonymous system_job).
        if let Some(factory) = self.query_executor_factory.as_ref() {
            let identity = self
                .run_as
                .clone()
                .unwrap_or_default()
                .identity(function_name, idempotency_token);
            live = live.with_executor(factory(identity));
        }
        live
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
            self.idempotency_key.as_deref(),
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
                // A transient error may request a minimum backoff (greylisting: an
                // SMTP tempfail clears in minutes, not the policy's seconds).
                |error: &fraiseql_error::FraiseQLError| match error {
                    fraiseql_error::FraiseQLError::ServiceUnavailable {
                        retry_after: Some(secs),
                        ..
                    } => Some(std::time::Duration::from_secs(*secs)),
                    _ => None,
                },
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
pub fn spawn_after_mutation(
    hooks: &BeforeMutationHooks,
    plans: Vec<AfterMutationDispatch>,
    query_executor_factory: Option<QueryExecutorFactory>,
) {
    spawn_dispatch(
        hooks,
        plans,
        fraiseql_observers::DispatchSource::AfterMutation,
        query_executor_factory,
    );
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
// Gated on `inbound` (not merely `functions-runtime`): the after:ingest dispatcher
// is only ever called from the inbound path (webhook + poll-IMAP email sinks), so a
// functions-runtime build without `inbound` (e.g. `sources`) must not compile it
// uncalled.
#[cfg(feature = "inbound")]
pub fn spawn_after_ingest(
    hooks: &BeforeMutationHooks,
    plans: Vec<AfterMutationDispatch>,
    query_executor_factory: Option<QueryExecutorFactory>,
) {
    spawn_dispatch(
        hooks,
        plans,
        fraiseql_observers::DispatchSource::AfterIngest,
        query_executor_factory,
    );
}

/// Spawn each plan on a durable dispatcher tagged with `source`.
///
/// Each plan's dispatcher is resolved with the function's own `run_as` ceiling from
/// `hooks.run_as` (#594), so its `fraiseql_query` bridge — built via
/// `query_executor_factory` — writes under that function's identity (or fail-closed
/// anonymous when the function declares none).
#[cfg(feature = "functions-runtime")]
fn spawn_dispatch(
    hooks: &BeforeMutationHooks,
    plans: Vec<AfterMutationDispatch>,
    source: fraiseql_observers::DispatchSource,
    query_executor_factory: Option<QueryExecutorFactory>,
) {
    let dispatcher = DurableDispatcher {
        observer: std::sync::Arc::clone(&hooks.observer),
        host_config: host_context_config(),
        limits: fraiseql_functions::ResourceLimits::default(),
        dlq: std::sync::Arc::clone(&hooks.dlq),
        source,
        sender_resolver: hooks.sender_resolver.clone(),
        email_transport: hooks.email_transport.clone(),
        idempotency_key: hooks.idempotency_key.clone(),
        query_executor_factory,
        // Set per-plan below from the function's definition.
        run_as: None,
    };

    for plan in plans {
        let setting = hooks.dispatch_settings.get(&plan.module.name).cloned().unwrap_or_default();
        let mut dispatcher = dispatcher.clone();
        dispatcher.run_as = hooks.run_as.get(&plan.module.name).cloned();
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
pub fn host_context_config() -> fraiseql_functions::host::live::HostContextConfig {
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
