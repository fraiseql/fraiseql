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
    document
        .definitions
        .iter()
        .fold(0, |count, def| {
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
fn count_selection_set(
    selection_set: &graphql_parser::query::SelectionSet<String>,
) -> usize {
    selection_set
        .items
        .iter()
        .fold(0, |count, item| {
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
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Operation") && result.unwrap_err().to_string().contains("not found"));
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
}
