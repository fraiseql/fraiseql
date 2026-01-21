//! Trait definitions for testable observer system architecture.
//!
//! These traits define the boundaries between components, enabling
//! mock implementations for testing without external dependencies.

use crate::config::ActionConfig;
use crate::event::EntityEvent;
use crate::error::Result;
use async_trait::async_trait;
use uuid::Uuid;

/// Event source abstraction for testing
///
/// Allows tests to provide pre-defined events without database connectivity.
#[async_trait]
pub trait EventSource: Send + Sync {
    /// Get the next event from the source
    ///
    /// Returns None when the source is exhausted
    async fn next_event(&mut self) -> Option<EntityEvent>;
}

/// Action execution abstraction for testing
///
/// Enables testing action execution without real external services.
#[async_trait]
pub trait ActionExecutor: Send + Sync {
    /// Execute an action on an event
    ///
    /// # Arguments
    /// * `event` - The triggering entity event
    /// * `action` - The action configuration to execute
    ///
    /// # Returns
    /// Result with action result on success or error
    async fn execute(
        &self,
        event: &EntityEvent,
        action: &ActionConfig,
    ) -> Result<ActionResult>;
}

/// Result of executing an action
#[derive(Debug, Clone)]
pub struct ActionResult {
    /// Type of action that was executed
    pub action_type: String,
    /// Whether the action succeeded
    pub success: bool,
    /// Status message
    pub message: String,
    /// Execution time in milliseconds
    pub duration_ms: f64,
}

/// Dead letter queue abstraction for testing
///
/// Allows testing failed action storage without database.
#[async_trait]
pub trait DeadLetterQueue: Send + Sync {
    /// Add a failed action to the DLQ
    ///
    /// # Arguments
    /// * `event` - The event that triggered the action
    /// * `action` - The action that failed
    /// * `error` - The error message
    ///
    /// # Returns
    /// UUID of the DLQ item
    async fn push(
        &self,
        event: EntityEvent,
        action: ActionConfig,
        error: String,
    ) -> Result<Uuid>;

    /// Get pending DLQ items
    ///
    /// # Arguments
    /// * `limit` - Maximum number of items to return
    async fn get_pending(&self, limit: i64) -> Result<Vec<DlqItem>>;

    /// Mark a DLQ item as successfully processed
    async fn mark_success(&self, id: Uuid) -> Result<()>;

    /// Mark a DLQ item as permanently failed
    async fn mark_retry_failed(&self, id: Uuid, error: &str) -> Result<()>;
}

/// Item in the dead letter queue
#[derive(Debug, Clone)]
pub struct DlqItem {
    /// Unique identifier
    pub id: Uuid,
    /// The event that failed
    pub event: EntityEvent,
    /// The action configuration
    pub action: ActionConfig,
    /// The error message
    pub error_message: String,
    /// Number of retry attempts
    pub attempts: u32,
}

/// Condition evaluator abstraction for testing
///
/// Enables testing condition logic without parsing overhead.
pub trait ConditionEvaluator: Send + Sync {
    /// Evaluate a condition expression against an event
    ///
    /// # Arguments
    /// * `condition` - The condition expression (e.g., "status == 'shipped'")
    /// * `event` - The event to evaluate against
    ///
    /// # Returns
    /// true if condition is met, false otherwise
    fn evaluate(&self, condition: &str, event: &EntityEvent) -> Result<bool>;
}

/// Template renderer abstraction for testing
///
/// Enables testing template rendering without template engine.
pub trait TemplateRenderer: Send + Sync {
    /// Render a template with event data
    ///
    /// # Arguments
    /// * `template` - The template string (Jinja-style: {{ field }})
    /// * `data` - The data to render with
    ///
    /// # Returns
    /// Rendered template string
    fn render(&self, template: &str, data: &serde_json::Value) -> Result<String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_result_creation() {
        let result = ActionResult {
            action_type: "email".to_string(),
            success: true,
            message: "Email sent".to_string(),
            duration_ms: 125.5,
        };

        assert_eq!(result.action_type, "email");
        assert!(result.success);
        assert_eq!(result.duration_ms, 125.5);
    }

    #[test]
    fn test_dlq_item_creation() {
        let event = EntityEvent::new(
            crate::event::EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            serde_json::json!({}),
        );

        let action = ActionConfig::Email {
            to: Some("user@example.com".to_string()),
            to_template: None,
            subject: Some("Test".to_string()),
            subject_template: None,
            body_template: Some("Body".to_string()),
            reply_to: None,
        };

        let item = DlqItem {
            id: Uuid::new_v4(),
            event,
            action,
            error_message: "SMTP failed".to_string(),
            attempts: 3,
        };

        assert_eq!(item.attempts, 3);
        assert_eq!(item.error_message, "SMTP failed");
    }
}
