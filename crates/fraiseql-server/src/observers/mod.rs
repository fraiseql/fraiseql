//! Observer management module for fraiseql-server.
//!
//! This module provides HTTP endpoints for managing observers (CRUD operations)
//! and integrates with the `fraiseql-observers` crate for event processing.
//!
//! # Architecture
//!
//! ```text
//! HTTP API (this module)
//!     ↓
//! tb_observer (database)
//!     ↓
//! ObserverRuntime (runtime.rs)
//!     ↓
//! ChangeLogListener → ObserverExecutor
//!     ↓
//! Actions (webhook, email, etc.)
//! ```
//!
//! # Features
//!
//! - CRUD operations for observer definitions
//! - Runtime execution of observers via change log polling
//! - Execution logging and statistics
//! - Multi-tenancy support via `fk_customer_org`
//! - Soft delete support

pub mod changelog_handlers;
pub mod config;
pub mod declared_env;
pub mod dlq_handlers;
pub mod handlers;
/// Postgres-backed durable function-dispatch dead-letter store (#598).
///
/// Only meaningful when function dispatch is compiled in, so gated on
/// `functions-runtime` (which brings the `fraiseql-functions` migration + the
/// dispatcher that writes to it).
#[cfg(feature = "functions-runtime")]
pub mod pg_function_dlq;
pub mod repository;
pub mod routes;
pub mod runtime;
mod scope;

#[cfg(test)]
mod tests;

pub use changelog_handlers::ChangelogState;
use chrono::{DateTime, Utc};
pub use config::ObserverManagementConfig;
pub use dlq_handlers::DlqState;
use fraiseql_observers::ActionConfig;
pub use handlers::{ObserverState, RuntimeHealthState};
pub use repository::{ObserverRepository, Reconciled};
pub use routes::{
    observer_changelog_routes, observer_dlq_routes, observer_routes, observer_runtime_routes,
};
pub use runtime::{ObserverRuntime, ObserverRuntimeConfig, PreparedObservers, RuntimeHealth};
pub use scope::{ObserverScope, OperatorOnly, ScopeRefusal};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Observer definition from the database.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Observer {
    /// Internal primary key (Trinity pattern)
    pub pk_observer: i64,

    /// External UUID for API references
    pub id: Uuid,

    /// Human-readable name
    pub name: String,

    /// Description of what this observer does
    pub description: Option<String>,

    /// Entity type to observe (None = all types)
    pub entity_type: Option<String>,

    /// Event type to observe (None = all events)
    pub event_type: Option<String>,

    /// Condition expression (DSL filter)
    pub condition_expression: Option<String>,

    /// Actions to execute as JSON
    pub actions: serde_json::Value,

    /// Whether this observer is enabled
    pub enabled: bool,

    /// Priority for ordering (lower = higher priority)
    pub priority: i32,

    /// Retry configuration as JSON
    pub retry_config: serde_json::Value,

    /// Timeout for action execution (milliseconds)
    pub timeout_ms: i32,

    /// Customer organization ID (multi-tenancy)
    pub fk_customer_org: Option<i64>,

    /// When the observer was created
    pub created_at: DateTime<Utc>,

    /// When the observer was last updated
    pub updated_at: DateTime<Utc>,

    /// Who created the observer
    pub created_by: Option<String>,

    /// Who last updated the observer
    pub updated_by: Option<String>,

    /// Soft delete timestamp (None = not deleted)
    pub deleted_at: Option<DateTime<Utc>>,

    /// Who owns the row: `None` for the admin API, the owner a schema's declared observers
    /// are stored under ([`declared_owner`]) otherwise.
    pub managed_by: Option<String>,
}

/// The owner every schema's declared observers were stored under, in
/// `tb_observer.managed_by`, before an owner carried its project's name.
///
/// One owner for every schema let a second schema booted against the same database remove
/// the first one's observers. Rows still under it are inherited by the first named owner
/// that reconciles ([`ObserverRepository::reconcile_declared`]). A schema that declares no
/// observer and writes no `[project] name` still reconciles under it, so the rows it once
/// declared are removed.
pub const MANAGED_BY_SCHEMA: &str = "schema";

/// What precedes the project name in the owner of a schema's declared observers.
pub const MANAGED_BY_PROJECT_PREFIX: &str = "schema:";

/// The owner the observers `schema` declares are stored under, in
/// `tb_observer.managed_by`: `schema:<[project] name>`.
///
/// Two projects on one database each keep their own declared observers, because the store
/// keeps owners apart. Two schemas compiled under one project name are one owner: each
/// boot removes what the other declares, as a redeploy of one project would.
///
/// The server writes these rows at boot from the schema, so the admin API changes nothing
/// of one but whether it is enabled: any other change would last until the next boot.
///
/// # Errors
///
/// Returns `ServerError::ConfigError` for a schema that declares observers and carries no
/// project name: stored under a shared owner, they would be removed by the next schema
/// booted against the database.
pub fn declared_owner(
    schema: &fraiseql_core::schema::CompiledSchema,
) -> Result<String, crate::ServerError> {
    match (schema.project_name.as_deref(), schema.observers.first()) {
        (Some(project), _) => Ok(format!("{MANAGED_BY_PROJECT_PREFIX}{project}")),
        (None, None) => Ok(MANAGED_BY_SCHEMA.to_owned()),
        (None, Some(observer)) => Err(crate::ServerError::ConfigError(format!(
            "the schema declares observers (`{}`) and carries no project name, which is the \
             owner they are stored under. Set `[project] name` in fraiseql.toml and compile \
             again",
            observer.name
        ))),
    }
}

/// Log what a reconcile of `owner`'s declared observers changed: at boot, and on a
/// schema reload that changes them.
pub(crate) fn log_reconciled(owner: &str, outcome: &Reconciled) {
    if !(outcome.created.is_empty() && outcome.updated.is_empty()) {
        tracing::info!(
            owner,
            created = ?outcome.created,
            updated = ?outcome.updated,
            "Stored the observers the schema declares"
        );
    }
    // Two schemas compiled under one project name are one owner, and each boot removes
    // what the other declares: the log is where an operator sees that happen.
    for (name, held_by) in outcome.removed_with_owner(owner) {
        tracing::warn!(
            observer = name,
            owner = held_by,
            "Observer removed because the current schema no longer declares it"
        );
    }
    for name in &outcome.taken_over {
        tracing::warn!(
            observer = %name,
            owner,
            "Observer stored before owners carried a project name is now this project's"
        );
    }
    for name in &outcome.adopted {
        tracing::warn!(
            observer = %name,
            "Observer adopted from admin API: the schema declares an observer of this name, \
             and its declaration replaced the stored one"
        );
    }
    for name in &outcome.disabled {
        tracing::warn!(
            observer = %name,
            "A declared observer is disabled and does not fire; enable it with \
             POST /api/observers/{{id}}/enable"
        );
    }
}

/// Placeholder substituted for secret-bearing values in API response bodies.
const REDACTED_PLACEHOLDER: &str = "[REDACTED]";

/// Redact secret-bearing values from an observer `actions` JSON array in place.
///
/// Two classes of secret are masked with [`REDACTED_PLACEHOLDER`] while the rest
/// of the action is preserved so an operator can still see how it is configured
/// (R8):
///
/// - every webhook `headers` **value** (e.g. `Authorization: Bearer …`), keeping the header names;
/// - the per-subscription webhook `signing_secret` **literal** (#467), a secret stored at rest.
///   (`signing_secret_env` and `headers_env` hold env-var *names*, not secrets, and are left
///   visible.)
///
/// Non-array `actions` and non-object entries are left untouched.
fn redact_action_secrets(actions: &mut serde_json::Value) {
    let Some(entries) = actions.as_array_mut() else {
        return;
    };
    for entry in entries {
        let Some(obj) = entry.as_object_mut() else {
            continue;
        };
        if let Some(headers) = obj.get_mut("headers").and_then(serde_json::Value::as_object_mut) {
            for value in headers.values_mut() {
                *value = serde_json::Value::String(REDACTED_PLACEHOLDER.to_string());
            }
        }
        if let Some(secret) = obj.get_mut("signing_secret") {
            if !secret.is_null() {
                *secret = serde_json::Value::String(REDACTED_PLACEHOLDER.to_string());
            }
        }
    }
}

impl Observer {
    /// Return a copy with secret-bearing fields redacted, destined for an HTTP
    /// response body.
    ///
    /// Defense in depth (Phase 03 R8): the observer admin API is gated behind the
    /// `fraiseql:admin` scope, but webhook secrets in `actions[].headers` must never
    /// travel in a response body regardless — they could otherwise be captured by
    /// proxies, browser history, or logs. The redaction is applied to every read and
    /// write response that echoes an observer.
    #[must_use]
    pub fn with_redacted_secrets(mut self) -> Self {
        redact_action_secrets(&mut self.actions);
        self
    }
}

/// The `actions` of a request, each read by [`ActionConfig::from_authored`]: the admin
/// API refuses what the compiler refuses of a declared observer, with the same text, and
/// stores the action the runtime reads. An action carrying a key the runtime ignores is
/// refused, by that key, instead of being stored and doing nothing.
///
/// The admin API also takes a webhook's or a Slack action's destination as a literal URL:
/// `url_env` and `webhook_url_env` are refused here (see [`literal_destination`]).
fn authored_actions<'de, D>(deserializer: D) -> Result<Vec<ActionConfig>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;

    let written: Vec<serde_json::Value> = Vec::deserialize(deserializer)?;
    written
        .into_iter()
        .enumerate()
        .map(|(index, action)| {
            let read = ActionConfig::from_authored(&action).map_err(|e| e.to_string());
            read.and_then(|read| literal_destination(&read).map(|()| read))
                .map_err(|e| D::Error::custom(format!("actions[{index}]: {e}")))
        })
        .collect()
}

/// Refuse a destination the admin API does not take: a webhook's or a Slack action's URL
/// read from an environment variable. An observer the schema declares may name one; one
/// created through the admin API names its URL.
fn literal_destination(action: &ActionConfig) -> Result<(), String> {
    match action {
        ActionConfig::Webhook {
            url_env: Some(_), ..
        } => Err("the admin API takes a webhook's destination as a literal URL: set `url`, \
                  not `url_env`"
            .to_string()),
        ActionConfig::Slack {
            webhook_url_env: Some(_),
            ..
        } => Err("the admin API takes a Slack action's destination as a literal URL: set \
                  `webhook_url`, not `webhook_url_env`"
            .to_string()),
        _ => Ok(()),
    }
}

/// [`authored_actions`] for a request that may leave `actions` unchanged.
fn authored_actions_if_sent<'de, D>(deserializer: D) -> Result<Option<Vec<ActionConfig>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // A present key that is `null` asks for no change, as an absent one does.
    let sent: Option<Vec<serde_json::Value>> = Option::deserialize(deserializer)?;
    sent.map(|written| {
        authored_actions(serde::de::value::SeqDeserializer::new(written.into_iter()))
    })
    .transpose()
    .map_err(|e: serde_json::Error| serde::de::Error::custom(e.to_string()))
}

/// A field an update can clear: a key left out is `None` (no change, through
/// `#[serde(default)]`), a key sent as `null` is `Some(None)`, and a value is `Some(Some(_))`.
#[allow(clippy::option_option)] // Reason: the three states are the field's, which serde fills
fn null_if_sent<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// Request to create a new observer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateObserverRequest {
    /// Human-readable name (required)
    pub name: String,

    /// Description of what this observer does
    #[serde(default)]
    pub description: Option<String>,

    /// Entity type to observe (None = all types)
    #[serde(default)]
    pub entity_type: Option<String>,

    /// Event type to observe (None = all events)
    #[serde(default)]
    pub event_type: Option<String>,

    /// Condition expression (DSL filter)
    #[serde(default)]
    pub condition_expression: Option<String>,

    /// Actions to execute
    #[serde(deserialize_with = "authored_actions")]
    pub actions: Vec<ActionConfig>,

    /// Whether this observer is enabled (default: true)
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Priority for ordering (default: 100)
    #[serde(default = "default_priority")]
    pub priority: i32,

    /// Retry configuration
    #[serde(default)]
    pub retry_config: Option<RetryConfig>,

    /// Timeout for action execution in milliseconds (default: 30000)
    #[serde(default = "default_timeout")]
    pub timeout_ms: i32,
}

/// Request to update an existing observer.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateObserverRequest {
    /// Human-readable name
    #[serde(default)]
    pub name: Option<String>,

    /// Description: `None` leaves it unchanged, `Some(None)` clears it, `Some(Some(_))`
    /// replaces it.
    #[serde(
        default,
        deserialize_with = "null_if_sent",
        skip_serializing_if = "Option::is_none"
    )]
    pub description: Option<Option<String>>,

    /// Entity type to observe
    #[serde(default)]
    pub entity_type: Option<String>,

    /// Event type to observe
    #[serde(default)]
    pub event_type: Option<String>,

    /// Condition expression: `None` leaves it unchanged (the key is left out), `Some(None)`
    /// clears it (the key is sent as `null`), `Some(Some(_))` replaces it.
    #[serde(
        default,
        deserialize_with = "null_if_sent",
        skip_serializing_if = "Option::is_none"
    )]
    pub condition_expression: Option<Option<String>>,

    /// Actions to execute
    #[serde(default, deserialize_with = "authored_actions_if_sent")]
    pub actions: Option<Vec<ActionConfig>>,

    /// Whether this observer is enabled
    #[serde(default)]
    pub enabled: Option<bool>,

    /// Priority for ordering
    #[serde(default)]
    pub priority: Option<i32>,

    /// Retry configuration: `None` leaves it unchanged, `Some(None)` resets it to
    /// [`RetryConfig::default`], what a create without it stores, and `Some(Some(_))`
    /// replaces it.
    #[serde(
        default,
        deserialize_with = "null_if_sent",
        skip_serializing_if = "Option::is_none"
    )]
    pub retry_config: Option<Option<RetryConfig>>,

    /// Timeout in milliseconds: `None` leaves it unchanged, `Some(None)` resets it to 30000,
    /// what a create without it stores, and `Some(Some(_))` replaces it.
    #[serde(
        default,
        deserialize_with = "null_if_sent",
        skip_serializing_if = "Option::is_none"
    )]
    pub timeout_ms: Option<Option<i32>>,
}

impl UpdateObserverRequest {
    /// Whether the request changes nothing but `enabled`: the one change the admin API
    /// makes to an observer the schema declares.
    #[must_use]
    pub const fn changes_only_enabled(&self) -> bool {
        let Self {
            name,
            description,
            entity_type,
            event_type,
            condition_expression,
            actions,
            enabled: _,
            priority,
            retry_config,
            timeout_ms,
        } = self;
        name.is_none()
            && description.is_none()
            && entity_type.is_none()
            && event_type.is_none()
            && condition_expression.is_none()
            && actions.is_none()
            && priority.is_none()
            && retry_config.is_none()
            && timeout_ms.is_none()
    }
}

/// Retry configuration for observer actions.
///
/// `deny_unknown_fields` (#612 item 11): the runtime observer's `RetryConfig`
/// reads `backoff_strategy`, so the pre-#612 DTO field name `backoff` was
/// silently ignored and every observer defaulted to exponential backoff. The
/// field is now `backoff_strategy` and an unknown key (the old `backoff`, or a
/// typo) is rejected at create time rather than silently dropped.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (default: 3)
    #[serde(default = "default_max_attempts")]
    pub max_attempts: i32,

    /// Backoff strategy: "fixed", "linear", "exponential".
    ///
    /// Serialized into the observer's `retry_config` JSONB and consumed by the
    /// runtime `fraiseql_observers::config::runtime::RetryConfig::backoff_strategy`.
    #[serde(default = "default_backoff")]
    pub backoff_strategy: String,

    /// Initial delay in milliseconds (default: 1000)
    #[serde(default = "default_initial_delay")]
    pub initial_delay_ms: i64,

    /// Maximum delay in milliseconds (default: 60000)
    #[serde(default = "default_max_delay")]
    pub max_delay_ms: i64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts:     default_max_attempts(),
            backoff_strategy: default_backoff(),
            initial_delay_ms: default_initial_delay(),
            max_delay_ms:     default_max_delay(),
        }
    }
}

/// Observer execution log entry.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ObserverLog {
    /// Internal primary key
    pub pk_observer_log: i64,

    /// External UUID
    pub id: Uuid,

    /// Reference to the observer
    pub fk_observer: i64,

    /// Event ID that triggered this execution
    pub event_id: Uuid,

    /// Entity type
    pub entity_type: String,

    /// Entity ID
    pub entity_id: Uuid,

    /// Event type (INSERT, UPDATE, DELETE)
    pub event_type: String,

    /// Execution status
    pub status: String,

    /// Action index in the actions array
    pub action_index: Option<i32>,

    /// Action type
    pub action_type: Option<String>,

    /// When execution started
    pub started_at: Option<DateTime<Utc>>,

    /// When execution completed
    pub completed_at: Option<DateTime<Utc>>,

    /// Duration in milliseconds
    pub duration_ms: Option<i32>,

    /// Error code (if failed)
    pub error_code: Option<String>,

    /// Error message (if failed)
    pub error_message: Option<String>,

    /// Retry attempt number
    pub attempt_number: i32,

    /// Trace ID for distributed tracing
    pub trace_id: Option<String>,

    /// When the log entry was created
    pub created_at: DateTime<Utc>,
}

/// Observer statistics from `vw_observer_stats` view.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ObserverStats {
    /// Internal primary key of the observer.
    pub pk_observer:           i64,
    /// External UUID of the observer.
    pub observer_id:           Uuid,
    /// Name of the observer.
    pub observer_name:         String,
    /// Entity type filter (if set on the observer).
    pub entity_type:           Option<String>,
    /// Event type filter (if set on the observer).
    pub event_type:            Option<String>,
    /// Whether the observer is currently enabled.
    pub enabled:               bool,
    /// Total number of executions recorded.
    pub total_executions:      i64,
    /// Number of executions that completed successfully.
    pub successful_executions: i64,
    /// Number of executions that ended in failure.
    pub failed_executions:     i64,
    /// Number of executions that timed out.
    pub timeout_executions:    i64,
    /// Number of executions that were skipped (condition not met).
    pub skipped_executions:    i64,
    /// Percentage of successful executions (0–100).
    pub success_rate_pct:      Option<f64>,
    /// Average execution duration in milliseconds.
    pub avg_duration_ms:       Option<f64>,
    /// Maximum execution duration in milliseconds.
    pub max_duration_ms:       Option<i32>,
    /// Minimum execution duration in milliseconds.
    pub min_duration_ms:       Option<i32>,
    /// Timestamp of the most recent execution.
    pub last_execution_at:     Option<DateTime<Utc>>,
}

/// Query parameters for listing observers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListObserversQuery {
    /// Filter by entity type
    #[serde(default)]
    pub entity_type: Option<String>,

    /// Filter by event type
    #[serde(default)]
    pub event_type: Option<String>,

    /// Filter by enabled status
    #[serde(default)]
    pub enabled: Option<bool>,

    /// Include deleted observers
    #[serde(default)]
    pub include_deleted: bool,

    /// Page number (1-based)
    #[serde(default = "default_page")]
    pub page: i64,

    /// Page size
    #[serde(default = "default_page_size")]
    pub page_size: i64,
}

/// Query parameters for listing observer logs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListObserverLogsQuery {
    /// Filter by observer ID
    #[serde(default)]
    pub observer_id: Option<Uuid>,

    /// Filter by status
    #[serde(default)]
    pub status: Option<String>,

    /// Filter by event ID
    #[serde(default)]
    pub event_id: Option<Uuid>,

    /// Filter by trace ID
    #[serde(default)]
    pub trace_id: Option<String>,

    /// Page number (1-based)
    #[serde(default = "default_page")]
    pub page: i64,

    /// Page size
    #[serde(default = "default_page_size")]
    pub page_size: i64,
}

/// Paginated response wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    /// Items on the current page.
    pub data:        Vec<T>,
    /// Current page number (1-based).
    pub page:        i64,
    /// Number of items per page.
    pub page_size:   i64,
    /// Total number of items across all pages.
    pub total_count: i64,
    /// Total number of pages.
    pub total_pages: i64,
}

impl<T> PaginatedResponse<T> {
    /// Construct a paginated response from a page of data and the total item count.
    #[must_use]
    pub const fn new(data: Vec<T>, page: i64, page_size: i64, total_count: i64) -> Self {
        let total_pages = (total_count + page_size - 1) / page_size;
        Self {
            data,
            page,
            page_size,
            total_count,
            total_pages,
        }
    }
}

// Default value functions for serde
const fn default_true() -> bool {
    true
}
const fn default_priority() -> i32 {
    100
}
const fn default_timeout() -> i32 {
    30000
}
/// The retry policy's defaults are the runtime's, by name: a field the server fills in
/// when it stores a policy is the value the runtime would give a document without it.
fn default_max_attempts() -> i32 {
    i32::from(fraiseql_observers::config::DEFAULT_RETRY_MAX_ATTEMPTS)
}
fn default_backoff() -> String {
    fraiseql_observers::BackoffStrategy::default().as_str().to_string()
}
fn default_initial_delay() -> i64 {
    i64::from(fraiseql_observers::config::DEFAULT_RETRY_INITIAL_DELAY_MS)
}
fn default_max_delay() -> i64 {
    i64::from(fraiseql_observers::config::DEFAULT_RETRY_MAX_DELAY_MS)
}
const fn default_page() -> i64 {
    1
}
const fn default_page_size() -> i64 {
    20
}

/// Install what a suite that boots an observer runtime must supply itself: the change log
/// the runtime reads, which is the application's (`fraiseql setup` installs it). The
/// runtime's own tables are the server's provisioning step.
#[cfg(test)]
pub(crate) async fn install_runtime_schema_for_tests(pool: &sqlx::PgPool) {
    crate::migration_lock::run_migration(
        pool,
        fraiseql_observers::migrations::entity_change_log_contract_sql(),
    )
    .await
    .expect("install the change log");
}
