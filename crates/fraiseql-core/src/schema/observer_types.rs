//! Observer definition types for database change event listeners.

use serde::{Deserialize, Serialize};

/// Observer definition - database change event listener.
///
/// Observers trigger actions (webhooks, notifications) when database
/// changes occur, enabling event-driven architectures.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::{ObserverDefinition, RetryConfig};
///
/// let observer = ObserverDefinition {
///     name: "onHighValueOrder".to_string(),
///     entity: "Order".to_string(),
///     event: "INSERT".to_string(),
///     condition: Some("total > 1000".to_string()),
///     actions: vec![
///         serde_json::json!({
///             "type": "webhook",
///             "url": "https://api.example.com/high-value-orders"
///         }),
///     ],
///     retry: RetryConfig {
///         max_attempts: 3,
///         backoff_strategy: "exponential".to_string(),
///         initial_delay_ms: 1000,
///         max_delay_ms: 60000,
///     },
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObserverDefinition {
    /// Observer name (unique identifier).
    pub name: String,

    /// Entity type to observe (e.g., "Order", "User").
    pub entity: String,

    /// Event type: INSERT, UPDATE, or DELETE.
    pub event: String,

    /// Optional condition expression in FraiseQL DSL.
    /// Example: "total > 1000" or "`status.changed()` and status == 'shipped'"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,

    /// Actions to execute when observer triggers.
    /// Each action is a JSON object with a "type" field (webhook, slack, email).
    pub actions: Vec<serde_json::Value>,

    /// Retry configuration for action execution.
    pub retry: RetryConfig,
}

impl ObserverDefinition {
    /// Create a new observer definition.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        entity: impl Into<String>,
        event: impl Into<String>,
    ) -> Self {
        Self {
            name:      name.into(),
            entity:    entity.into(),
            event:     event.into(),
            condition: None,
            actions:   Vec::new(),
            retry:     RetryConfig::default(),
        }
    }

    /// Set the condition expression.
    #[must_use]
    pub fn with_condition(mut self, condition: impl Into<String>) -> Self {
        self.condition = Some(condition.into());
        self
    }

    /// Add an action to this observer.
    #[must_use]
    pub fn with_action(mut self, action: serde_json::Value) -> Self {
        self.actions.push(action);
        self
    }

    /// Add multiple actions to this observer.
    #[must_use]
    pub fn with_actions(mut self, actions: Vec<serde_json::Value>) -> Self {
        self.actions = actions;
        self
    }

    /// Set the retry configuration.
    #[must_use]
    pub fn with_retry(mut self, retry: RetryConfig) -> Self {
        self.retry = retry;
        self
    }

    /// Check if this observer has a condition.
    #[must_use]
    pub const fn has_condition(&self) -> bool {
        self.condition.is_some()
    }

    /// Get the number of actions.
    #[must_use]
    pub const fn action_count(&self) -> usize {
        self.actions.len()
    }
}

/// Retry configuration for observer actions.
///
/// Controls how failed actions are retried with configurable
/// backoff strategies.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::RetryConfig;
///
/// let retry = RetryConfig {
///     max_attempts: 5,
///     backoff_strategy: "exponential".to_string(),
///     initial_delay_ms: 1000,
///     max_delay_ms: 60000,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts.
    pub max_attempts: u32,

    /// Backoff strategy: exponential, linear, or fixed.
    pub backoff_strategy: String,

    /// Initial delay in milliseconds.
    pub initial_delay_ms: u32,

    /// Maximum delay in milliseconds (cap for exponential backoff).
    pub max_delay_ms: u32,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts:     3,
            backoff_strategy: "exponential".to_string(),
            initial_delay_ms: 1000,
            max_delay_ms:     60000,
        }
    }
}

impl RetryConfig {
    /// Create a new retry configuration.
    #[must_use]
    pub fn new(
        max_attempts: u32,
        backoff_strategy: impl Into<String>,
        initial_delay_ms: u32,
        max_delay_ms: u32,
    ) -> Self {
        Self {
            max_attempts,
            backoff_strategy: backoff_strategy.into(),
            initial_delay_ms,
            max_delay_ms,
        }
    }

    /// Create exponential backoff configuration.
    #[must_use]
    pub fn exponential(max_attempts: u32, initial_delay_ms: u32, max_delay_ms: u32) -> Self {
        Self::new(max_attempts, "exponential", initial_delay_ms, max_delay_ms)
    }

    /// Create linear backoff configuration.
    #[must_use]
    pub fn linear(max_attempts: u32, initial_delay_ms: u32, max_delay_ms: u32) -> Self {
        Self::new(max_attempts, "linear", initial_delay_ms, max_delay_ms)
    }

    /// Create fixed delay configuration.
    #[must_use]
    pub fn fixed(max_attempts: u32, delay_ms: u32) -> Self {
        Self::new(max_attempts, "fixed", delay_ms, delay_ms)
    }

    /// Check if backoff strategy is exponential.
    #[must_use]
    pub fn is_exponential(&self) -> bool {
        self.backoff_strategy == "exponential"
    }

    /// Check if backoff strategy is linear.
    #[must_use]
    pub fn is_linear(&self) -> bool {
        self.backoff_strategy == "linear"
    }

    /// Check if backoff strategy is fixed.
    #[must_use]
    pub fn is_fixed(&self) -> bool {
        self.backoff_strategy == "fixed"
    }
}
