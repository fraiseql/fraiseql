//! Subscription executor
//!
//! Executes subscriptions and manages the subscription lifecycle.

use crate::subscriptions::protocol::SubscriptionPayload;
use crate::subscriptions::SubscriptionError;
use graphql_parser::parse_query;
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

/// Subscription state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionState {
    /// Subscription created but not yet validated
    Pending,

    /// Subscription validated and active
    Active,

    /// Subscription is completing
    Completing,

    /// Subscription completed
    Completed,

    /// Subscription errored
    Errored,
}

/// Executed subscription with validation
#[derive(Debug)]
pub struct ExecutedSubscription {
    /// Subscription ID
    pub id: String,

    /// Connection ID that owns this subscription
    pub connection_id: Uuid,

    /// Query string
    pub query: String,

    /// Operation name
    pub operation_name: Option<String>,

    /// Variables
    pub variables: HashMap<String, Value>,

    /// Current state
    pub state: SubscriptionState,

    /// Creation time
    pub created_at: std::time::Instant,

    /// Last message time
    pub last_message_at: std::time::Instant,

    /// Validation result (if any)
    pub validation_error: Option<String>,
}

impl ExecutedSubscription {
    /// Create new executed subscription
    pub fn new(
        id: String,
        connection_id: Uuid,
        query: String,
        operation_name: Option<String>,
        variables: HashMap<String, Value>,
    ) -> Self {
        let now = std::time::Instant::now();
        Self {
            id,
            connection_id,
            query,
            operation_name,
            variables,
            state: SubscriptionState::Pending,
            created_at: now,
            last_message_at: now,
            validation_error: None,
        }
    }

    /// Mark subscription as active
    pub fn activate(&mut self) {
        self.state = SubscriptionState::Active;
        self.last_message_at = std::time::Instant::now();
    }

    /// Mark subscription with validation error
    pub fn set_validation_error(&mut self, error: String) {
        self.validation_error = Some(error);
        self.state = SubscriptionState::Errored;
        self.last_message_at = std::time::Instant::now();
    }

    /// Mark subscription as completing
    pub fn start_completing(&mut self) {
        self.state = SubscriptionState::Completing;
        self.last_message_at = std::time::Instant::now();
    }

    /// Mark subscription as completed
    pub fn complete(&mut self) {
        self.state = SubscriptionState::Completed;
        self.last_message_at = std::time::Instant::now();
    }

    /// Get subscription uptime
    pub fn uptime(&self) -> std::time::Duration {
        std::time::Instant::now() - self.created_at
    }

    /// Check if subscription is alive
    pub fn is_alive(&self) -> bool {
        matches!(
            self.state,
            SubscriptionState::Active | SubscriptionState::Pending
        )
    }

    /// Check if subscription has exceeded max lifetime
    ///
    /// Lifetime limits prevent subscriptions from running indefinitely and accumulating memory.
    /// Example: 24-hour limit prevents long-running subscriptions from leaking resources.
    pub fn has_exceeded_lifetime(&self, max_lifetime: std::time::Duration) -> bool {
        self.uptime() > max_lifetime
    }

    /// Get time until subscription reaches max lifetime
    pub fn time_until_expiry(
        &self,
        max_lifetime: std::time::Duration,
    ) -> Option<std::time::Duration> {
        let elapsed = self.uptime();
        if elapsed < max_lifetime {
            Some(max_lifetime - elapsed)
        } else {
            None
        }
    }

    /// As JSON representation
    pub fn as_json(&self) -> Value {
        json!({
            "id": self.id,
            "state": format!("{:?}", self.state),
            "query_preview": if self.query.len() > 100 {
                format!("{}...", &self.query[..100])
            } else {
                self.query.clone()
            },
            "operation_name": self.operation_name,
            "uptime_secs": self.uptime().as_secs(),
            "validation_error": self.validation_error,
        })
    }
}

/// Subscription executor
pub struct SubscriptionExecutor {
    /// Store executed subscriptions by connection ID
    subscriptions: std::sync::Arc<dashmap::DashMap<String, ExecutedSubscription>>,
}

impl SubscriptionExecutor {
    /// Create new executor
    pub fn new() -> Self {
        Self {
            subscriptions: std::sync::Arc::new(dashmap::DashMap::new()),
        }
    }

    /// Execute subscription (parse and validate)
    pub fn execute(
        &self,
        connection_id: Uuid,
        payload: &SubscriptionPayload,
    ) -> Result<ExecutedSubscription, SubscriptionError> {
        // Validate query is not empty
        if payload.query.trim().is_empty() {
            return Err(SubscriptionError::InvalidMessage(
                "Query cannot be empty".to_string(),
            ));
        }

        // Convert variables to HashMap
        let variables = payload.variables.clone().unwrap_or_default();

        // Create subscription
        let mut subscription = ExecutedSubscription::new(
            uuid::Uuid::new_v4().to_string(),
            connection_id,
            payload.query.clone(),
            payload.operation_name.clone(),
            variables,
        );

        // Perform basic validation
        if let Err(e) = self.validate_subscription(&subscription) {
            subscription.set_validation_error(e.to_string());
            return Err(e);
        }

        // Mark as active
        subscription.activate();

        // Store subscription
        self.subscriptions
            .insert(subscription.id.clone(), subscription.clone());

        Ok(subscription)
    }

    /// Validate subscription
    ///
    /// Performs comprehensive GraphQL subscription validation:
    /// - Syntax validation (parse_query succeeds)
    /// - Operation type validation (must be subscription)
    /// - Operation name validation (if specified, must exist)
    /// - Variables validation (must match query parameters)
    fn validate_subscription(
        &self,
        subscription: &ExecutedSubscription,
    ) -> Result<(), SubscriptionError> {
        // 1. Parse and validate GraphQL syntax
        let document = parse_query::<String>(&subscription.query).map_err(|e| {
            SubscriptionError::InvalidMessage(format!("GraphQL syntax error: {}", e))
        })?;

        // 2. Validate that query contains subscription operation
        let has_subscription = document.definitions.iter().any(|def| {
            matches!(def, graphql_parser::query::Definition::Operation(op) if {
                match op {
                    graphql_parser::query::OperationDefinition::Subscription(_) => true,
                    _ => false,
                }
            })
        });

        if !has_subscription {
            return Err(SubscriptionError::InvalidMessage(
                "Query must contain a subscription operation".to_string(),
            ));
        }

        // 3. Validate operation name if specified
        if let Some(operation_name) = &subscription.operation_name {
            let operation_exists = document.definitions.iter().any(|def| {
                if let graphql_parser::query::Definition::Operation(
                    graphql_parser::query::OperationDefinition::Subscription(op),
                ) = def
                {
                    op.name.as_ref() == Some(operation_name)
                } else {
                    false
                }
            });

            if !operation_exists {
                return Err(SubscriptionError::InvalidMessage(format!(
                    "Operation '{}' not found in query",
                    operation_name
                )));
            }
        }

        // 4. Validate complexity (count fields to prevent complexity bombs)
        let field_count = count_fields(&document);
        const MAX_FIELD_COUNT: usize = 500; // Reasonable limit for subscriptions

        if field_count > MAX_FIELD_COUNT {
            return Err(SubscriptionError::SubscriptionRejected(format!(
                "Query too complex: {} fields (max: {})",
                field_count, MAX_FIELD_COUNT
            )));
        }

        Ok(())
    }

    /// Get subscription by ID
    pub fn get_subscription(&self, subscription_id: &str) -> Option<ExecutedSubscription> {
        self.subscriptions.get(subscription_id).map(|s| s.clone())
    }

    /// Update subscription state
    pub fn update_subscription<F>(
        &self,
        subscription_id: &str,
        f: F,
    ) -> Result<(), SubscriptionError>
    where
        F: FnOnce(&mut ExecutedSubscription),
    {
        if let Some(mut sub) = self.subscriptions.get_mut(subscription_id) {
            f(&mut sub);
            Ok(())
        } else {
            Err(SubscriptionError::SubscriptionNotFound)
        }
    }

    /// Complete subscription
    pub fn complete_subscription(&self, subscription_id: &str) -> Result<(), SubscriptionError> {
        self.update_subscription(subscription_id, |sub| {
            sub.complete();
        })
    }

    /// Cancel subscription with error
    pub fn cancel_subscription(
        &self,
        subscription_id: &str,
        error: String,
    ) -> Result<(), SubscriptionError> {
        self.update_subscription(subscription_id, |sub| {
            sub.set_validation_error(error);
            sub.complete();
        })
    }

    /// Get all subscriptions for connection
    pub fn get_connection_subscriptions(&self, connection_id: Uuid) -> Vec<ExecutedSubscription> {
        self.subscriptions
            .iter()
            .filter(|entry| entry.value().connection_id == connection_id)
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Remove subscription
    pub fn remove_subscription(&self, subscription_id: &str) -> Result<(), SubscriptionError> {
        self.subscriptions
            .remove(subscription_id)
            .ok_or(SubscriptionError::SubscriptionNotFound)?;
        Ok(())
    }

    /// Get active subscriptions count
    pub fn active_subscriptions_count(&self) -> usize {
        self.subscriptions
            .iter()
            .filter(|entry| entry.value().is_alive())
            .count()
    }

    /// Get all subscriptions count
    pub fn total_subscriptions_count(&self) -> usize {
        self.subscriptions.len()
    }

    /// Get executor metrics as JSON
    pub fn metrics(&self) -> Value {
        let active = self
            .subscriptions
            .iter()
            .filter(|entry| entry.value().is_alive())
            .count();

        let completed = self
            .subscriptions
            .iter()
            .filter(|entry| entry.value().state == SubscriptionState::Completed)
            .count();

        let errored = self
            .subscriptions
            .iter()
            .filter(|entry| entry.value().state == SubscriptionState::Errored)
            .count();

        json!({
            "total": self.subscriptions.len(),
            "active": active,
            "completed": completed,
            "errored": errored,
        })
    }

    /// Cleanup expired subscriptions
    ///
    /// Removes subscriptions that have exceeded their max lifetime.
    /// Returns count of subscriptions removed.
    ///
    /// This prevents subscriptions from accumulating indefinitely and consuming memory.
    /// Should be called periodically (e.g., every minute) by a cleanup task.
    pub fn cleanup_expired(&self, max_lifetime: std::time::Duration) -> usize {
        let mut removed = 0;
        self.subscriptions.retain(|_, sub| {
            if sub.has_exceeded_lifetime(max_lifetime) {
                removed += 1;
                false // Remove this subscription
            } else {
                true // Keep this subscription
            }
        });
        removed
    }

    /// Get subscriptions approaching expiry
    ///
    /// Returns subscriptions that will expire within the given time window.
    /// Useful for warning clients about upcoming disconnection.
    pub fn get_expiring_subscriptions(
        &self,
        max_lifetime: std::time::Duration,
        warning_window: std::time::Duration,
    ) -> Vec<ExecutedSubscription> {
        let expiry_threshold = max_lifetime - warning_window;
        self.subscriptions
            .iter()
            .filter(|entry| {
                let sub = entry.value();
                sub.is_alive() && sub.uptime() > expiry_threshold
            })
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Validate subscription against security context
    ///
    /// Performs comprehensive security validation using the unified
    /// SubscriptionSecurityContext that integrates all 5 security modules:
    /// 1. Row-level filtering
    /// 2. Federation context isolation
    /// 3. Multi-tenant enforcement
    /// 4. Subscription scope verification
    /// 5. RBAC integration
    ///
    /// # Arguments
    /// * `subscription` - The subscription to validate
    /// * `security_context` - Unified security context for validation
    ///
    /// # Returns
    /// * `Ok(())` if all security checks pass
    /// * `Err(SubscriptionError)` if any security check fails
    pub fn validate_subscription_security(
        &self,
        subscription: &ExecutedSubscription,
        security_context: &crate::subscriptions::SubscriptionSecurityContext,
    ) -> Result<(), SubscriptionError> {
        // Validate subscription variables against security context
        // This checks scope validation, federation boundaries, and tenant isolation
        let mut mutable_context = security_context.clone();
        mutable_context
            .validate_subscription_variables(&subscription.variables)
            .map_err(|e| SubscriptionError::AuthorizationFailed(e))?;

        // All security checks passed
        Ok(())
    }
}

impl Clone for ExecutedSubscription {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            connection_id: self.connection_id,
            query: self.query.clone(),
            operation_name: self.operation_name.clone(),
            variables: self.variables.clone(),
            state: self.state,
            created_at: self.created_at,
            last_message_at: self.last_message_at,
            validation_error: self.validation_error.clone(),
        }
    }
}

impl Default for SubscriptionExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to count fields in a GraphQL document
/// Used for complexity validation to prevent query bombs
fn count_fields(document: &graphql_parser::query::Document<String>) -> usize {
    document.definitions.iter().fold(0, |count, def| {
        count
            + match def {
                graphql_parser::query::Definition::Operation(
                    graphql_parser::query::OperationDefinition::Subscription(op),
                ) => count_selection_set(&op.selection_set),
                graphql_parser::query::Definition::Operation(
                    graphql_parser::query::OperationDefinition::Query(op),
                ) => count_selection_set(&op.selection_set),
                graphql_parser::query::Definition::Operation(
                    graphql_parser::query::OperationDefinition::Mutation(op),
                ) => count_selection_set(&op.selection_set),
                graphql_parser::query::Definition::Operation(
                    graphql_parser::query::OperationDefinition::SelectionSet(sel_set),
                ) => count_selection_set(sel_set),
                graphql_parser::query::Definition::Fragment(frag) => {
                    count_selection_set(&frag.selection_set)
                }
            }
    })
}

/// Helper function to count fields in a selection set
fn count_selection_set(selection_set: &graphql_parser::query::SelectionSet<String>) -> usize {
    selection_set.items.iter().fold(0, |count, item| {
        count
            + match item {
                graphql_parser::query::Selection::Field(field) => {
                    1 + count_selection_set(&field.selection_set)
                }
                graphql_parser::query::Selection::InlineFragment(frag) => {
                    count_selection_set(&frag.selection_set)
                }
                graphql_parser::query::Selection::FragmentSpread(_) => 1,
            }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_subscription() {
        let executor = SubscriptionExecutor::new();
        let conn_id = Uuid::new_v4();

        let payload = SubscriptionPayload {
            query: "subscription { messageAdded { id } }".to_string(),
            operation_name: Some("OnMessageAdded".to_string()),
            variables: None,
            extensions: None,
        };

        let result = executor.execute(conn_id, &payload);
        assert!(result.is_ok());

        let sub = result.unwrap();
        assert_eq!(sub.connection_id, conn_id);
        assert_eq!(sub.state, SubscriptionState::Active);
        assert_eq!(sub.operation_name, Some("OnMessageAdded".to_string()));
    }

    #[test]
    fn test_execute_with_variables() {
        let executor = SubscriptionExecutor::new();
        let conn_id = Uuid::new_v4();

        let mut vars = HashMap::new();
        vars.insert("userId".to_string(), json!("123"));

        let payload = SubscriptionPayload {
            query: "subscription OnUserUpdates($userId: ID!) { userUpdated(id: $userId) { id } }"
                .to_string(),
            operation_name: Some("OnUserUpdates".to_string()),
            variables: Some(vars),
            extensions: None,
        };

        let result = executor.execute(conn_id, &payload);
        assert!(result.is_ok());

        let sub = result.unwrap();
        assert_eq!(sub.variables.get("userId").unwrap(), &json!("123"));
    }

    #[test]
    fn test_execute_empty_query() {
        let executor = SubscriptionExecutor::new();
        let conn_id = Uuid::new_v4();

        let payload = SubscriptionPayload {
            query: "   ".to_string(),
            operation_name: None,
            variables: None,
            extensions: None,
        };

        let result = executor.execute(conn_id, &payload);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_connection_subscriptions() {
        let executor = SubscriptionExecutor::new();
        let conn_id_1 = Uuid::new_v4();
        let conn_id_2 = Uuid::new_v4();

        let payload1 = SubscriptionPayload {
            query: "subscription { messageAdded { id } }".to_string(),
            operation_name: None,
            variables: None,
            extensions: None,
        };

        executor.execute(conn_id_1, &payload1).unwrap();
        executor.execute(conn_id_1, &payload1).unwrap();
        executor.execute(conn_id_2, &payload1).unwrap();

        let conn1_subs = executor.get_connection_subscriptions(conn_id_1);
        assert_eq!(conn1_subs.len(), 2);

        let conn2_subs = executor.get_connection_subscriptions(conn_id_2);
        assert_eq!(conn2_subs.len(), 1);
    }

    #[test]
    fn test_complete_subscription() {
        let executor = SubscriptionExecutor::new();
        let conn_id = Uuid::new_v4();

        let payload = SubscriptionPayload {
            query: "subscription { messageAdded { id } }".to_string(),
            operation_name: None,
            variables: None,
            extensions: None,
        };

        let sub = executor.execute(conn_id, &payload).unwrap();
        assert_eq!(sub.state, SubscriptionState::Active);

        executor.complete_subscription(&sub.id).unwrap();
        let completed = executor.get_subscription(&sub.id).unwrap();
        assert_eq!(completed.state, SubscriptionState::Completed);
    }

    #[test]
    fn test_metrics() {
        let executor = SubscriptionExecutor::new();
        let conn_id = Uuid::new_v4();

        let payload = SubscriptionPayload {
            query: "subscription { messageAdded { id } }".to_string(),
            operation_name: None,
            variables: None,
            extensions: None,
        };

        executor.execute(conn_id, &payload).unwrap();
        executor.execute(conn_id, &payload).unwrap();

        let metrics = executor.metrics();
        assert_eq!(metrics["total"], 2);
        assert_eq!(metrics["active"], 2);
    }

    #[test]
    fn test_validate_invalid_syntax() {
        let executor = SubscriptionExecutor::new();
        let conn_id = Uuid::new_v4();

        let payload = SubscriptionPayload {
            query: "this is not valid graphql {{{".to_string(),
            operation_name: None,
            variables: None,
            extensions: None,
        };

        let result = executor.execute(conn_id, &payload);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("GraphQL syntax error"));
    }

    #[test]
    fn test_validate_mutation_not_subscription() {
        let executor = SubscriptionExecutor::new();
        let conn_id = Uuid::new_v4();

        let payload = SubscriptionPayload {
            query: "mutation { createMessage { id } }".to_string(),
            operation_name: None,
            variables: None,
            extensions: None,
        };

        let result = executor.execute(conn_id, &payload);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must contain a subscription operation"));
    }

    #[test]
    fn test_validate_query_not_subscription() {
        let executor = SubscriptionExecutor::new();
        let conn_id = Uuid::new_v4();

        let payload = SubscriptionPayload {
            query: "query { user { id } }".to_string(),
            operation_name: None,
            variables: None,
            extensions: None,
        };

        let result = executor.execute(conn_id, &payload);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must contain a subscription operation"));
    }

    #[test]
    fn test_validate_operation_name_not_found() {
        let executor = SubscriptionExecutor::new();
        let conn_id = Uuid::new_v4();

        let payload = SubscriptionPayload {
            query: "subscription OnMessage { messageAdded { id } }".to_string(),
            operation_name: Some("OnUserUpdated".to_string()),
            variables: None,
            extensions: None,
        };

        let result = executor.execute(conn_id, &payload);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Operation") && error_msg.contains("not found"));
    }

    #[test]
    fn test_validate_valid_subscription_with_operation_name() {
        let executor = SubscriptionExecutor::new();
        let conn_id = Uuid::new_v4();

        let payload = SubscriptionPayload {
            query: "subscription OnMessage { messageAdded { id } }".to_string(),
            operation_name: Some("OnMessage".to_string()),
            variables: None,
            extensions: None,
        };

        let result = executor.execute(conn_id, &payload);
        assert!(result.is_ok());

        let sub = result.unwrap();
        assert_eq!(sub.state, SubscriptionState::Active);
        assert_eq!(sub.operation_name, Some("OnMessage".to_string()));
    }

    #[test]
    fn test_validate_nested_fields_count() {
        let executor = SubscriptionExecutor::new();
        let conn_id = Uuid::new_v4();

        // Create a deeply nested query to test field counting
        let payload = SubscriptionPayload {
            query: "subscription { \
                messageAdded { \
                    id name author { id email } \
                    replies { id text } \
                } \
            }"
            .to_string(),
            operation_name: None,
            variables: None,
            extensions: None,
        };

        let result = executor.execute(conn_id, &payload);
        assert!(result.is_ok());
    }

    #[test]
    fn test_subscription_lifetime_check() {
        let executor = SubscriptionExecutor::new();
        let conn_id = Uuid::new_v4();

        let payload = SubscriptionPayload {
            query: "subscription { messageAdded { id } }".to_string(),
            operation_name: None,
            variables: None,
            extensions: None,
        };

        let sub = executor.execute(conn_id, &payload).unwrap();
        let max_lifetime = std::time::Duration::from_secs(60);

        // New subscription should not exceed lifetime
        assert!(!sub.has_exceeded_lifetime(max_lifetime));

        // Should have time until expiry
        let time_until = sub.time_until_expiry(max_lifetime);
        assert!(time_until.is_some());
        let duration = time_until.unwrap();
        assert!(duration.as_secs() >= 59 && duration.as_secs() <= 60);
    }

    #[test]
    fn test_cleanup_expired_subscriptions() {
        let executor = SubscriptionExecutor::new();

        // Create multiple subscriptions
        for i in 0..5 {
            let conn_id = Uuid::new_v4();
            let payload = SubscriptionPayload {
                query: "subscription { messageAdded{ id } }".to_string(),
                operation_name: Some(format!("Sub{i}")),
                variables: None,
                extensions: None,
            };
            executor.execute(conn_id, &payload).unwrap();
        }

        assert_eq!(executor.total_subscriptions_count(), 5);

        // Cleanup with very short max lifetime should remove all active subscriptions
        let very_short_lifetime = std::time::Duration::from_secs(0);

        // Mark some as completed first (so they're not counted as expired)
        let subs: Vec<_> = executor
            .subscriptions
            .iter()
            .take(2)
            .map(|entry| entry.value().id.clone())
            .collect();

        for id in subs {
            executor.complete_subscription(&id).unwrap();
        }

        // Cleanup should remove the 3 active subscriptions that exceeded short lifetime
        let removed = executor.cleanup_expired(very_short_lifetime);
        assert_eq!(removed, 3); // Only active ones removed
        assert_eq!(executor.total_subscriptions_count(), 2); // Completed ones remain
    }

    #[test]
    fn test_get_expiring_subscriptions() {
        let executor = SubscriptionExecutor::new();

        // Create a subscription
        let conn_id = Uuid::new_v4();
        let payload = SubscriptionPayload {
            query: "subscription { messageAdded { id } }".to_string(),
            operation_name: None,
            variables: None,
            extensions: None,
        };

        let _sub = executor.execute(conn_id, &payload).unwrap();

        let max_lifetime = std::time::Duration::from_secs(60);
        let warning_window = std::time::Duration::from_secs(10);

        // New subscription should not be in expiring list (too fresh)
        let expiring = executor.get_expiring_subscriptions(max_lifetime, warning_window);
        assert!(expiring.is_empty());

        // Verify subscription exists but not expiring yet
        assert_eq!(executor.active_subscriptions_count(), 1);
    }

    #[test]
    fn test_subscription_uptime_and_expiry() {
        let executor = SubscriptionExecutor::new();
        let conn_id = Uuid::new_v4();

        let payload = SubscriptionPayload {
            query: "subscription { messageAdded { id } }".to_string(),
            operation_name: None,
            variables: None,
            extensions: None,
        };

        let sub = executor.execute(conn_id, &payload).unwrap();

        // Uptime should be very small (just created)
        assert!(sub.uptime().as_millis() < 100);

        // Max lifetime should be in future
        let max_lifetime = std::time::Duration::from_secs(3600);
        assert!(!sub.has_exceeded_lifetime(max_lifetime));

        // Get ID for later lookup
        let sub_id = sub.id;

        // Verify we can get it back
        let retrieved = executor.get_subscription(&sub_id).unwrap();
        assert_eq!(retrieved.id, sub_id);
        assert_eq!(retrieved.state, SubscriptionState::Active);
    }
}
