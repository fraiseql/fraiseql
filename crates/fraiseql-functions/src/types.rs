//! Core types for function execution.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::Digest;

/// Marker a function (or a host op) puts in a thrown error's message to signal the
/// failure is **permanent** — do not retry, dead-letter immediately.
///
/// Both runtimes surface a guest failure as a string, so permanence travels as this
/// sentinel substring: when the error message contains it, the runtime classifies
/// the failure as a client error (4xx) — which the durable dispatcher dead-letters
/// on the first attempt — rather than the default transient `Unsupported` (501).
///
/// A guest can also throw `Object.assign(new Error(msg), { fraiseqlPermanent: true })`
/// (Deno); the wrapper folds that into this marker. Host ops that already know a
/// failure is permanent (e.g. `send_email` on a denied identity or a rejected
/// recipient) prepend it automatically.
pub const PERMANENT_ERROR_MARKER: &str = "[fraiseql:permanent]";

/// Supported runtime types for serverless functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RuntimeType {
    /// `WebAssembly` Component Model runtime.
    Wasm,
    /// Deno (`JavaScript`/`TypeScript` via V8) runtime.
    Deno,
}

impl RuntimeType {
    /// Get supported file extensions for this runtime.
    #[must_use]
    pub const fn supported_extensions(&self) -> &[&str] {
        match self {
            RuntimeType::Wasm => &[".wasm"],
            RuntimeType::Deno => &[".js", ".ts", ".mjs", ".mts"],
        }
    }
}

/// A compiled function module ready for execution.
#[derive(Debug, Clone)]
pub struct FunctionModule {
    /// Unique name for this function.
    pub name:        String,
    /// Hash of the module source (for caching).
    pub source_hash: String,
    /// Compiled bytecode or source text.
    pub bytecode:    bytes::Bytes,
    /// Which runtime executes this module.
    pub runtime:     RuntimeType,
}

impl FunctionModule {
    /// Create a new WASM module from compiled bytecode.
    pub fn from_bytecode(name: String, bytecode: bytes::Bytes) -> Self {
        let source_hash = hex::encode(sha2::Sha256::digest(&bytecode));
        Self {
            name,
            source_hash,
            bytecode,
            runtime: RuntimeType::Wasm,
        }
    }

    /// Create a new source-based module (JavaScript/TypeScript).
    #[must_use]
    pub fn from_source(name: String, source: String, runtime: RuntimeType) -> Self {
        let bytecode = bytes::Bytes::from(source);
        let source_hash = hex::encode(sha2::Sha256::digest(&bytecode));
        Self {
            name,
            source_hash,
            bytecode,
            runtime,
        }
    }
}

/// Trigger event payload for a function invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPayload {
    /// Type of trigger: "mutation", "subscription", "cron", "webhook", etc.
    pub trigger_type: String,
    /// Entity name (e.g., "User", "Post").
    pub entity:       String,
    /// Event kind (e.g., "created", "updated", "deleted").
    pub event_kind:   String,
    /// Event data (JSON).
    pub data:         serde_json::Value,
    /// Timestamp when the event occurred.
    pub timestamp:    chrono::DateTime<chrono::Utc>,
}

/// The least-privilege authority ceiling a function's `fraiseql_query` bridge
/// writes run under (#594) — the function's `run_as`.
///
/// This is the same authority model scheduled sources use
/// ([`fraiseql_core::schema::RunAs`], `docs/architecture/sources.md:88-109`), applied
/// to event-dispatched functions: a *ceiling* the function can never exceed. A
/// [`FunctionDefinition`] with no `run_as` runs **fail-closed** — its host's
/// `fraiseql_query` executes under an anonymous [`system_job`] identity with no
/// roles/scopes/tenant, so RLS and field-authorization deny every write until an
/// operator grants a ceiling. Granting authority is a deliberate act, never a
/// default.
///
/// It is a distinct type from the core `RunAs` so the base `fraiseql-functions`
/// crate (used by the CLI, codegen, and authoring) need not depend on
/// `fraiseql-core`; the two share an identical JSON shape and the wiring layer
/// (`host-live`) converts to a `SecurityContext` via [`FunctionDefinition::identity`].
///
/// [`system_job`]: fraiseql_core::security::SecurityContext::system_job
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunAs {
    /// Roles granted to the function's background write identity (the RBAC ceiling).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<String>,

    /// Scopes granted to the function's background write identity.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,

    /// The single tenant this function's bridge writes are scoped to, if any. Unset
    /// ⇒ global/system (NULL tenant).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
}

#[cfg(feature = "host-live")]
impl RunAs {
    /// Build the background [`SecurityContext`](fraiseql_core::security::SecurityContext)
    /// this ceiling grants, identified as `system_job:<job_id>` and correlated by
    /// `request_id`. Mirrors
    /// [`SourceDefinition::identity`](fraiseql_core::schema::SourceDefinition::identity):
    /// the roles/scopes/tenant are the ceiling, the [`ActorType::SystemJob`] is
    /// recorded for audit, never an authorization input.
    ///
    /// [`ActorType::SystemJob`]: fraiseql_core::security::ActorType::SystemJob
    #[must_use]
    pub fn identity(
        &self,
        job_id: impl Into<String>,
        request_id: impl Into<String>,
    ) -> fraiseql_core::security::SecurityContext {
        fraiseql_core::security::SecurityContext::system_job(
            job_id,
            request_id,
            self.roles.clone(),
            self.scopes.clone(),
            self.tenant.clone().map(fraiseql_core::types::TenantId::from),
        )
    }
}

/// Definition of a serverless function for deployment and execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// Unique name for this function.
    pub name:       String,
    /// Trigger type and configuration (e.g., "after:mutation:createUser", "cron:0 * * * *",
    /// "<http:GET:/users/:id>").
    pub trigger:    String,
    /// Which runtime executes this function.
    pub runtime:    RuntimeType,
    /// Optional timeout in milliseconds (overrides defaults).
    /// - For `before:mutation` triggers: defaults to 500ms
    /// - For other triggers: defaults to 5s
    pub timeout_ms: Option<u64>,

    /// The authority ceiling this function's `fraiseql_query` bridge writes run
    /// under (#594). Absent ⇒ **fail-closed**: the bridge runs under an anonymous
    /// identity and RLS/field-authz deny writes. See [`RunAs`] and
    /// [`identity`](Self::identity).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_as: Option<RunAs>,

    /// Fire-and-forget opt-out for durable dispatch.
    ///
    /// After-mutation function dispatch is durable by default: a transient
    /// failure is retried with backoff and, once retries are exhausted, the
    /// invocation is dead-lettered so money- and send-path work is never
    /// silently lost. Set `re_runnable = true` for work that is safe to simply
    /// re-run later (e.g. LLM scoring) — such dispatch stays fire-and-forget with
    /// no retry or dead-letter overhead. See ADR 0015 for the rationale.
    #[serde(default)]
    pub re_runnable: bool,

    /// Per-function retry policy for durable dispatch.
    ///
    /// `None` uses the server default (overridable via `FRAISEQL_FUNCTIONS_RETRY_*`
    /// environment variables). Ignored when [`re_runnable`](Self::re_runnable) is
    /// `true`. Reuses the observer subsystem's [`RetryConfig`] so retry semantics
    /// are identical across both subsystems.
    ///
    /// [`RetryConfig`]: fraiseql_observers::RetryConfig
    #[serde(default)]
    pub retry: Option<fraiseql_observers::RetryConfig>,
}

impl FunctionDefinition {
    /// Create a new function definition.
    #[must_use]
    pub fn new(name: &str, trigger: &str, runtime: RuntimeType) -> Self {
        Self {
            name: name.to_string(),
            trigger: trigger.to_string(),
            runtime,
            timeout_ms: None,
            run_as: None,
            re_runnable: false,
            retry: None,
        }
    }

    /// Set the [`run_as`](Self::run_as) authority ceiling (#594).
    #[must_use]
    pub fn with_run_as(mut self, run_as: RunAs) -> Self {
        self.run_as = Some(run_as);
        self
    }

    /// The background [`SecurityContext`] this function's `fraiseql_query` bridge
    /// writes run under (#594).
    ///
    /// Built from [`run_as`](Self::run_as) via [`SecurityContext::system_job`], so a
    /// function write is audited as `system_job:<function-name>` under
    /// [`ActorType::SystemJob`] — the same envelope a source write carries. **Absent
    /// `run_as` yields a fail-closed identity** (no roles, no scopes, no tenant →
    /// every authz/RLS decision denies). `request_id` correlates one dispatch
    /// (typically its per-dispatch idempotency token).
    ///
    /// [`SecurityContext`]: fraiseql_core::security::SecurityContext
    /// [`SecurityContext::system_job`]: fraiseql_core::security::SecurityContext::system_job
    /// [`ActorType::SystemJob`]: fraiseql_core::security::ActorType::SystemJob
    #[cfg(feature = "host-live")]
    #[must_use]
    pub fn identity(
        &self,
        request_id: impl Into<String>,
    ) -> fraiseql_core::security::SecurityContext {
        self.run_as.clone().unwrap_or_default().identity(&self.name, request_id)
    }

    /// Mark this function as re-runnable (fire-and-forget) dispatch.
    ///
    /// See [`re_runnable`](Self::re_runnable) and ADR 0015.
    #[must_use]
    pub const fn re_runnable(mut self) -> Self {
        self.re_runnable = true;
        self
    }

    /// Set a custom timeout for this function.
    #[must_use]
    pub const fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }

    /// Get the effective timeout for this function.
    #[must_use]
    pub fn effective_timeout(&self) -> Duration {
        match self.timeout_ms {
            Some(ms) => Duration::from_millis(ms),
            None => {
                // before:mutation defaults to 500ms; others default to 5s
                if self.trigger.starts_with("before:mutation") {
                    Duration::from_millis(500)
                } else {
                    Duration::from_secs(5)
                }
            },
        }
    }

    /// Check if this function is a before:mutation trigger.
    #[must_use]
    pub fn is_before_mutation(&self) -> bool {
        self.trigger.starts_with("before:mutation:")
    }

    /// Check if this function is an after:mutation trigger.
    #[must_use]
    pub fn is_after_mutation(&self) -> bool {
        self.trigger.starts_with("after:mutation:")
    }

    /// Check if this function is an after:storage trigger.
    #[must_use]
    pub fn is_after_storage(&self) -> bool {
        self.trigger.starts_with("after:storage:")
    }

    /// Check if this function is an after:ingest trigger.
    #[must_use]
    pub fn is_after_ingest(&self) -> bool {
        self.trigger == "after:ingest" || self.trigger.starts_with("after:ingest:")
    }

    /// Check if this function is a cron trigger.
    #[must_use]
    pub fn is_cron(&self) -> bool {
        self.trigger.starts_with("cron:")
    }

    /// Check if this function is an HTTP trigger.
    #[must_use]
    pub fn is_http(&self) -> bool {
        self.trigger.starts_with("http:")
    }
}

/// Log level for structured logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum LogLevel {
    /// Debug level.
    Debug,
    /// Info level.
    Info,
    /// Warning level.
    Warn,
    /// Error level.
    Error,
}

/// A single log entry from function execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    /// Log level.
    pub level:     LogLevel,
    /// Log message.
    pub message:   String,
    /// When the log was written.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Result of a function invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionResult {
    /// Return value from the function (may be None if function returns void).
    pub value:             Option<serde_json::Value>,
    /// All logs captured during execution.
    pub logs:              Vec<LogEntry>,
    /// Total execution duration.
    pub duration:          Duration,
    /// Peak memory usage in bytes.
    pub memory_peak_bytes: u64,
}

/// Resource limits for function execution.
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Maximum memory allocation in bytes.
    pub max_memory_bytes: u64,
    /// Maximum execution duration.
    pub max_duration:     Duration,
    /// Maximum number of log entries to capture.
    pub max_log_entries:  usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: 128 * 1024 * 1024,      // 128 MB
            max_duration:     Duration::from_secs(5), // 5 seconds
            max_log_entries:  10_000,
        }
    }
}

#[cfg(test)]
mod tests;
