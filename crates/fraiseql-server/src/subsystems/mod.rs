//! Serverless-functions subsystem types shared across the server.
//!
//! [`FunctionsSubsystem`] is assembled at startup by
//! [`loader::build_functions_subsystem`] from the `[functions]` section of the
//! compiled schema; [`BeforeMutationHooks`] is the cloneable hot-path snapshot
//! of it that lives in `AppState`.
//!
//! (The former `ServerSubsystems` bundle, its builder and
//! `validate_subsystems_config` were deleted in #874: nothing in the boot path
//! ever constructed them, so their advisories never reached an operator —
//! dead code that read like a running gate.)

/// Loads function modules from disk and assembles the functions-runtime subsystem.
#[cfg(feature = "functions-runtime")]
pub mod loader;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use fraiseql_functions::{FunctionObserver, triggers::TriggerRegistry};

use crate::schema::loader::FunctionsConfig;

// ── Subsystem structs ─────────────────────────────────────────────────────────

/// Functions subsystem: observer and trigger registry.
///
/// Assembled at server startup from the `[functions]` section of the compiled schema.
/// The observer dispatches events to function runtimes; the registry maps triggers to
/// function definitions and provides HTTP route matchers.
pub struct FunctionsSubsystem {
    /// Observer that dispatches trigger events to the appropriate function runtime.
    pub observer: Arc<FunctionObserver>,

    /// Registry mapping trigger types to function definitions.
    pub trigger_registry: TriggerRegistry,

    /// Loaded function modules keyed by function name.
    ///
    /// Populated at server startup by reading source files from `config.module_dir`.
    /// Used by the before-mutation chain and the after-mutation dispatcher.
    pub module_registry: std::collections::HashMap<String, fraiseql_functions::FunctionModule>,

    /// Schema-level functions configuration (definitions + module directory).
    pub config: FunctionsConfig,
}

// ── Before-mutation hook bundle ───────────────────────────────────────────────

/// Shared bundle of before-mutation state passed into `AppState` for handler access.
///
/// This is a lightweight, cloneable snapshot of the parts of [`FunctionsSubsystem`]
/// that are needed on the hot path for before-mutation checks. It is extracted once
/// at server startup and stored in `AppState` via an `Arc`.
pub struct BeforeMutationHooks {
    /// Registry of all loaded triggers, keyed by trigger type and mutation name.
    pub trigger_registry: TriggerRegistry,
    /// Loaded function modules keyed by function name.
    pub module_registry:  std::collections::HashMap<String, fraiseql_functions::FunctionModule>,
    /// Observer that dispatches events to the appropriate function runtime.
    pub observer:         std::sync::Arc<FunctionObserver>,

    /// Dead-letter queue for durable after:mutation dispatch: an invocation that
    /// exhausts its retries (or fails permanently) is pushed here rather than
    /// silently lost.
    #[cfg(feature = "functions-runtime")]
    pub dlq: std::sync::Arc<dyn fraiseql_observers::DeadLetterQueue>,

    /// Per-function dispatch settings (re-runnable flag + retry policy) resolved
    /// from the compiled schema, keyed by function name. Functions absent from
    /// the map fall back to the durable `FunctionDispatchSetting::default`.
    #[cfg(feature = "functions-runtime")]
    pub dispatch_settings:
        std::collections::HashMap<String, crate::routes::after_mutation::FunctionDispatchSetting>,

    /// Host-owned sender-identity resolver for the `send_email` op — resolves the
    /// `from` from the authenticated context (the #539 seam). `None` → `send_email`
    /// is unconfigured and fails loud. Set together with `email_transport` via
    /// [`with_email`](Self::with_email).
    #[cfg(feature = "functions-runtime")]
    pub sender_resolver: Option<std::sync::Arc<dyn fraiseql_functions::SenderIdentityResolver>>,

    /// Email transport for the `send_email` op. `None` → `send_email` fails loud.
    #[cfg(feature = "functions-runtime")]
    pub email_transport: Option<std::sync::Arc<dyn fraiseql_functions::EmailTransport>>,

    /// HMAC subkey for the per-dispatch idempotency token, derived from the server
    /// HMAC secret. `Some` → the token is signed (unforgeable, required before it is
    /// exposed in a VERP Return-Path); `None` → an unsigned digest (zero-config
    /// default). Set via [`with_idempotency_key`](Self::with_idempotency_key).
    #[cfg(feature = "functions-runtime")]
    pub idempotency_key: Option<std::sync::Arc<[u8]>>,

    /// Per-function `run_as` authority ceilings (#594), keyed by function name.
    /// A function absent from the map has no ceiling ⇒ its `fraiseql_query` bridge
    /// runs fail-closed (anonymous `system_job`; RLS/field-authz deny writes).
    /// Populated from the compiled schema's function definitions.
    #[cfg(feature = "functions-runtime")]
    pub run_as: std::collections::HashMap<String, fraiseql_functions::RunAs>,
}

impl BeforeMutationHooks {
    /// Create a hook bundle with default durable-dispatch wiring: an unbounded
    /// in-memory dead-letter queue and no per-function overrides (every function
    /// uses the durable default).
    ///
    /// For the full compiled-schema resolution (per-function settings +
    /// `FRAISEQL_FUNCTIONS_*` env overrides), use
    /// [`FunctionsSubsystem::into_before_mutation_hooks`] instead.
    #[must_use]
    pub fn new(
        trigger_registry: TriggerRegistry,
        module_registry: std::collections::HashMap<String, fraiseql_functions::FunctionModule>,
        observer: Arc<FunctionObserver>,
    ) -> Self {
        Self {
            trigger_registry,
            module_registry,
            observer,
            #[cfg(feature = "functions-runtime")]
            dlq: Arc::new(crate::observers::runtime::InMemoryDlq::new_with_max(None)),
            #[cfg(feature = "functions-runtime")]
            dispatch_settings: std::collections::HashMap::new(),
            #[cfg(feature = "functions-runtime")]
            sender_resolver: None,
            #[cfg(feature = "functions-runtime")]
            email_transport: None,
            #[cfg(feature = "functions-runtime")]
            idempotency_key: None,
            #[cfg(feature = "functions-runtime")]
            run_as: std::collections::HashMap::new(),
        }
    }

    /// Attach the HMAC subkey that signs the per-dispatch idempotency token.
    ///
    /// Derived once from the server HMAC secret
    /// ([`fraiseql_observers::derive_idempotency_subkey`]). `None` leaves the token
    /// as an unsigned digest — the zero-config default; a signed token is required
    /// before it is exposed externally as a VERP Return-Path (P04b).
    #[cfg(feature = "functions-runtime")]
    #[must_use]
    pub fn with_idempotency_key(mut self, key: Option<std::sync::Arc<[u8]>>) -> Self {
        self.idempotency_key = key;
        self
    }

    /// Enable the `send_email` host op for dispatched functions by attaching a
    /// sender-identity resolver (the host-owned `from`) and an email transport.
    ///
    /// Without both, `send_email` fails loud (mirrors the `sql_query`
    /// fail-loud-until-wired stance). The resolver is the #539 seam —
    /// `LoginEmailSender` (from = login email) by default, a DB-backed resolver
    /// where the sending mailbox differs; the transport is the per-connected-account
    /// SMTP relay ([`SmtpMailboxTransport`](crate::inbound::email::SmtpMailboxTransport)).
    #[cfg(feature = "functions-runtime")]
    #[must_use]
    pub fn with_email(
        mut self,
        sender_resolver: Arc<dyn fraiseql_functions::SenderIdentityResolver>,
        email_transport: Arc<dyn fraiseql_functions::EmailTransport>,
    ) -> Self {
        self.sender_resolver = Some(sender_resolver);
        self.email_transport = Some(email_transport);
        self
    }

    /// Replace the dead-letter store (#598).
    ///
    /// The default from [`FunctionsSubsystem::into_before_mutation_hooks`] is the
    /// in-memory store; the serve path swaps in the Postgres-backed
    /// [`PgFunctionDlq`](crate::observers::pg_function_dlq::PgFunctionDlq) when
    /// `[functions] dlq_store = "postgres"` and a database pool is available, so a
    /// dead-lettered dispatch survives a restart.
    #[cfg(feature = "functions-runtime")]
    #[must_use]
    pub fn with_dlq(mut self, dlq: Arc<dyn fraiseql_observers::DeadLetterQueue>) -> Self {
        self.dlq = dlq;
        self
    }
}

#[cfg(feature = "functions-runtime")]
impl FunctionsSubsystem {
    /// Assemble the before-mutation hook bundle for `AppState`.
    ///
    /// Resolves each function's durable-dispatch settings (re-runnable flag +
    /// retry policy) from the compiled schema, layering the
    /// `FRAISEQL_FUNCTIONS_*` environment overrides via `DispatchDefaults::from_env`,
    /// and creates the shared dead-letter queue (capped by
    /// `FRAISEQL_FUNCTIONS_DLQ_MAX_SIZE`). Consumes the subsystem because the hook
    /// bundle takes ownership of its trigger registry, modules, and observer.
    #[must_use]
    pub fn into_before_mutation_hooks(self) -> BeforeMutationHooks {
        use crate::routes::after_mutation::{DispatchDefaults, resolve_dispatch_settings};

        let defaults = DispatchDefaults::from_env();
        let dispatch_settings = resolve_dispatch_settings(&self.config.definitions, &defaults);
        let dlq = std::sync::Arc::new(crate::observers::runtime::InMemoryDlq::new_with_max(
            defaults.dlq_max_size,
        ));

        // #594: collect each function's `run_as` ceiling, keyed by name. A function
        // with no `run_as` is simply absent (fail-closed at dispatch time).
        let run_as = self
            .config
            .definitions
            .iter()
            .filter_map(|def| def.run_as.clone().map(|ceiling| (def.name.clone(), ceiling)))
            .collect();

        BeforeMutationHooks {
            trigger_registry: self.trigger_registry,
            module_registry: self.module_registry,
            observer: self.observer,
            dlq,
            dispatch_settings,
            sender_resolver: None,
            email_transport: None,
            idempotency_key: None,
            run_as,
        }
    }
}
