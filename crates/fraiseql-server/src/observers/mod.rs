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

pub mod config;
pub mod handlers;
pub mod repository;
pub mod routes;
pub mod runtime;

pub use config::ObserverManagementConfig;
pub use handlers::{ObserverState, RuntimeHealthState};
pub use repository::ObserverRepository;
pub use routes::{observer_routes, observer_runtime_routes};
pub use runtime::{ObserverRuntime, ObserverRuntimeConfig, RuntimeHealth};

use chrono::{DateTime, Utc};
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

    /// Description
    #[serde(default)]
    pub description: Option<String>,

    /// Entity type to observe
    #[serde(default)]
    pub entity_type: Option<String>,

    /// Event type to observe
    #[serde(default)]
    pub event_type: Option<String>,

    /// Condition expression
    #[serde(default)]
    pub condition_expression: Option<String>,

    /// Actions to execute
    #[serde(default)]
    pub actions: Option<Vec<ActionConfig>>,

    /// Whether this observer is enabled
    #[serde(default)]
    pub enabled: Option<bool>,

    /// Priority for ordering
    #[serde(default)]
    pub priority: Option<i32>,

    /// Retry configuration
    #[serde(default)]
    pub retry_config: Option<RetryConfig>,

    /// Timeout in milliseconds
    #[serde(default)]
    pub timeout_ms: Option<i32>,
}

/// Action configuration for an observer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionConfig {
    /// HTTP webhook action
    Webhook {
        url: String,
        #[serde(default = "default_method")]
        method: String,
        #[serde(default)]
        headers: Option<std::collections::HashMap<String, String>>,
        #[serde(default)]
        body_template: Option<String>,
    },

    /// Email notification action
    Email {
        to: String,
        #[serde(default)]
        cc: Option<String>,
        subject_template: String,
        body_template: String,
    },

    /// Slack message action
    Slack {
        webhook_url: String,
        #[serde(default)]
        channel: Option<String>,
        message_template: String,
    },

    /// Database function call
    Database {
        function_name: String,
        #[serde(default)]
        params: Option<serde_json::Value>,
    },

    /// Log action (for debugging)
    Log {
        #[serde(default = "default_log_level")]
        level: String,
        message_template: String,
    },
}

/// Retry configuration for observer actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (default: 3)
    #[serde(default = "default_max_attempts")]
    pub max_attempts: i32,

    /// Backoff strategy: "fixed", "linear", "exponential"
    #[serde(default = "default_backoff")]
    pub backoff: String,

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
            max_attempts: 3,
            backoff: "exponential".to_string(),
            initial_delay_ms: 1000,
            max_delay_ms: 60000,
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

/// Observer statistics from vw_observer_stats view.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ObserverStats {
    pub pk_observer: i64,
    pub observer_id: Uuid,
    pub observer_name: String,
    pub entity_type: Option<String>,
    pub event_type: Option<String>,
    pub enabled: bool,
    pub total_executions: i64,
    pub successful_executions: i64,
    pub failed_executions: i64,
    pub timeout_executions: i64,
    pub skipped_executions: i64,
    pub success_rate_pct: Option<f64>,
    pub avg_duration_ms: Option<f64>,
    pub max_duration_ms: Option<i32>,
    pub min_duration_ms: Option<i32>,
    pub last_execution_at: Option<DateTime<Utc>>,
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
    pub data: Vec<T>,
    pub page: i64,
    pub page_size: i64,
    pub total_count: i64,
    pub total_pages: i64,
}

impl<T> PaginatedResponse<T> {
    pub fn new(data: Vec<T>, page: i64, page_size: i64, total_count: i64) -> Self {
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
fn default_true() -> bool {
    true
}
fn default_priority() -> i32 {
    100
}
fn default_timeout() -> i32 {
    30000
}
fn default_method() -> String {
    "POST".to_string()
}
fn default_log_level() -> String {
    "info".to_string()
}
fn default_max_attempts() -> i32 {
    3
}
fn default_backoff() -> String {
    "exponential".to_string()
}
fn default_initial_delay() -> i64 {
    1000
}
fn default_max_delay() -> i64 {
    60000
}
fn default_page() -> i64 {
    1
}
fn default_page_size() -> i64 {
    20
}
