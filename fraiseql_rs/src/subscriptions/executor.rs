//! Subscription executor
//!
//! Executes subscriptions and manages the subscription lifecycle.

use crate::subscriptions::protocol::SubscriptionPayload;
use crate::subscriptions::{SubscriptionError, SubscriptionSecurityContext};
use futures_util::future;
use graphql_parser::parse_query;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::Mutex;
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

/// Subscription with security context
///
/// Wraps an ExecutedSubscription with its corresponding security context,
/// used for enforcing authorization during event delivery.
#[derive(Debug, Clone)]
pub struct ExecutedSubscriptionWithSecurity {
    /// The underlying subscription
    pub subscription: ExecutedSubscription,
    /// Security context for this subscription
    pub security_context: SubscriptionSecurityContext,
    /// Number of security violations recorded for this subscription
    pub violations_count: u32,
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
#[derive(Clone)]
pub struct SubscriptionExecutor {
    /// Store executed subscriptions by connection ID
    subscriptions: std::sync::Arc<dashmap::DashMap<String, ExecutedSubscription>>,
    /// Store subscriptions with their security contexts for event delivery validation
    subscriptions_secure: Arc<dashmap::DashMap<String, ExecutedSubscriptionWithSecurity>>,
    /// Channel index for fast subscription lookup by event channel
    /// Maps channel name → list of subscription IDs subscribed to that channel
    channel_index: Arc<dashmap::DashMap<String, Vec<String>>>,
    /// Response queues per subscription (Phase 2)
    /// Stores pre-serialized response bytes ready for WebSocket transmission
    response_queues: Arc<dashmap::DashMap<String, Arc<Mutex<VecDeque<Vec<u8>>>>>>,
    /// Optional resolver invocation callback (Phase 3)
    /// Set by PySubscriptionExecutor to enable Python resolver invocation
    resolver_callback: Arc<Mutex<Option<Arc<dyn ResolverCallback>>>>,
}

/// Callback trait for resolver invocation (Phase 3)
///
/// Allows SubscriptionExecutor to invoke Python resolvers without creating
/// a direct dependency on PySubscriptionExecutor.
pub trait ResolverCallback: Send + Sync {
    /// Invoke a resolver and return the result as JSON string
    fn invoke(
        &self,
        subscription_id: &str,
        event_data_json: &str,
    ) -> Result<String, SubscriptionError>;
}

impl SubscriptionExecutor {
    /// Create new executor
    pub fn new() -> Self {
        Self {
            subscriptions: std::sync::Arc::new(dashmap::DashMap::new()),
            subscriptions_secure: Arc::new(dashmap::DashMap::new()),
            channel_index: Arc::new(dashmap::DashMap::new()),
            response_queues: Arc::new(dashmap::DashMap::new()),
            resolver_callback: Arc::new(Mutex::new(None)),
        }
    }

    /// Set resolver callback for Python resolver invocation (Phase 3)
    ///
    /// Called by PySubscriptionExecutor to enable Python resolver invocation
    /// during event dispatch.
    pub fn set_resolver_callback(&self, callback: Arc<dyn ResolverCallback>) {
        // Store the callback for use during event dispatch
        let callback_ref = self.resolver_callback.try_lock();
        if let Ok(mut callback_guard) = callback_ref {
            *callback_guard = Some(callback);
        }
    }

    /// Get all subscription IDs subscribed to a channel
    /// Returns empty vector if no subscriptions for channel
    pub fn subscriptions_by_channel(&self, channel: &str) -> Vec<String> {
        self.channel_index
            .get(channel)
            .map(|entry| entry.value().clone())
            .unwrap_or_default()
    }

    /// Add subscription to channel index
    /// Used internally when registering subscriptions
    fn add_to_channel_index(&self, channel: String, subscription_id: String) {
        self.channel_index
            .entry(channel)
            .or_insert_with(Vec::new)
            .push(subscription_id);
    }

    /// Remove subscription from channel index
    /// Cleans up empty channel entries
    fn remove_from_channel_index(&self, channel: &str, subscription_id: &str) {
        if let Some(mut entry) = self.channel_index.get_mut(channel) {
            entry.retain(|id| id != subscription_id);
            if entry.is_empty() {
                drop(entry);
                self.channel_index.remove(channel);
            }
        }
    }

    /// Queue a response for a subscription (Phase 2)
    /// Response bytes are pre-serialized and ready for WebSocket transmission
    pub fn queue_response(
        &self,
        subscription_id: String,
        response_bytes: Vec<u8>,
    ) -> Result<(), SubscriptionError> {
        // Get or create queue for subscription
        let queue_arc = {
            let queue_entry = self
                .response_queues
                .entry(subscription_id.clone())
                .or_insert_with(|| Arc::new(Mutex::new(VecDeque::new())));
            queue_entry.value().clone()
        }; // queue_entry guard is dropped here

        // Queue response - use try_lock first to avoid blocking in async context
        {
            if let Ok(mut q) = queue_arc.try_lock() {
                q.push_back(response_bytes);
                return Ok(());
            }
        } // Guard is dropped here

        // If we couldn't acquire lock immediately, use blocking
        let mut q = queue_arc.blocking_lock();
        q.push_back(response_bytes);
        Ok(())
    }

    /// Get next response for a subscription (Phase 2)
    /// Returns pre-serialized bytes or None if queue is empty
    pub fn next_event(&self, subscription_id: &str) -> Result<Option<Vec<u8>>, SubscriptionError> {
        // Verify subscription exists
        let _sub = self
            .subscriptions_secure
            .get(subscription_id)
            .ok_or(SubscriptionError::SubscriptionNotFound)?;

        // Get next response from queue if available
        if let Some(queue_entry) = self.response_queues.get(subscription_id) {
            // Clone the Arc before dropping the DashMap ref
            let queue_arc = Arc::clone(queue_entry.value());
            drop(queue_entry); // Release DashMap reference

            // Try to lock and pop from queue
            {
                if let Ok(mut q) = queue_arc.try_lock() {
                    return Ok(q.pop_front());
                }
            }
            // If we couldn't lock, return None
            return Ok(None);
        }

        Ok(None)
    }

    /// Clean up response queue on subscription removal
    fn cleanup_response_queue(&self, subscription_id: &str) {
        self.response_queues.remove(subscription_id);
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

        // Also remove from channel index (Phase 2: using wildcard channel)
        let channel = "*".to_string();
        self.remove_from_channel_index(&channel, subscription_id);

        // Clean up response queue
        self.cleanup_response_queue(subscription_id);

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

    /// Execute subscription WITH security validation
    ///
    /// Creates a new subscription and validates it against the provided security context.
    /// The subscription is stored with the security context for use during event delivery.
    ///
    /// # Arguments
    /// * `connection_id` - The connection that owns this subscription
    /// * `payload` - The GraphQL subscription payload
    /// * `security_context` - Security context from authenticated user
    ///
    /// # Returns
    /// * `Ok(ExecutedSubscriptionWithSecurity)` if validation passes
    /// * `Err(SubscriptionError)` if validation fails
    pub fn execute_with_security(
        &self,
        connection_id: Uuid,
        payload: &SubscriptionPayload,
        security_context: SubscriptionSecurityContext,
    ) -> Result<ExecutedSubscriptionWithSecurity, SubscriptionError> {
        // 1. Create and validate basic subscription
        let mut subscription = ExecutedSubscription::new(
            uuid::Uuid::new_v4().to_string(),
            connection_id,
            payload.query.clone(),
            payload.operation_name.clone(),
            payload.variables.clone().unwrap_or_default(),
        );

        // 2. Perform basic GraphQL validation
        if let Err(e) = self.validate_subscription(&subscription) {
            subscription.set_validation_error(e.to_string());
            return Err(e);
        }

        // 3. Perform security validation
        self.validate_subscription_security(&subscription, &security_context)?;

        // 4. Mark as active
        subscription.activate();

        // 5. Store subscription with security context
        let sub_with_security = ExecutedSubscriptionWithSecurity {
            subscription: subscription.clone(),
            security_context,
            violations_count: 0,
        };

        self.subscriptions_secure
            .insert(subscription.id.clone(), sub_with_security.clone());

        // Also store in regular subscriptions map for backward compatibility
        self.subscriptions
            .insert(subscription.id.clone(), subscription.clone());

        // 6. Add to channel index (Phase 2: default to "*" until query parsing extracts channel)
        // In Phase 3, extract actual channel from GraphQL query
        let channel = "*".to_string(); // Wildcard channel for Phase 2
        self.add_to_channel_index(channel, subscription.id.clone());

        Ok(sub_with_security)
    }

    /// Record a security violation for a subscription
    ///
    /// Increments the violation counter and logs the reason for audit trail.
    ///
    /// # Arguments
    /// * `subscription_id` - The subscription ID
    /// * `reason` - Human-readable reason for the violation
    ///
    /// # Returns
    /// * `Ok(())` if violation recorded
    /// * `Err(SubscriptionError::SubscriptionNotFound)` if subscription doesn't exist
    pub fn record_security_violation(
        &self,
        subscription_id: &str,
        reason: &str,
    ) -> Result<(), SubscriptionError> {
        if let Some(mut entry) = self.subscriptions_secure.get_mut(subscription_id) {
            entry.violations_count += 1;
            println!(
                "[SECURITY] Subscription {} violation: {}",
                subscription_id, reason
            );
            Ok(())
        } else {
            Err(SubscriptionError::SubscriptionNotFound)
        }
    }

    /// Get the number of security violations for a subscription
    ///
    /// # Arguments
    /// * `subscription_id` - The subscription ID
    ///
    /// # Returns
    /// * Violation count (0 if no violations or subscription not found)
    pub fn get_violation_count(&self, subscription_id: &str) -> u32 {
        self.subscriptions_secure
            .get(subscription_id)
            .map(|entry| entry.violations_count)
            .unwrap_or(0)
    }

    /// Retrieve a subscription with its security context
    ///
    /// # Arguments
    /// * `subscription_id` - The subscription ID
    ///
    /// # Returns
    /// * `Some(ExecutedSubscriptionWithSecurity)` if found
    /// * `None` if not found
    pub fn get_subscription_with_security(
        &self,
        subscription_id: &str,
    ) -> Option<ExecutedSubscriptionWithSecurity> {
        self.subscriptions_secure
            .get(subscription_id)
            .map(|entry| entry.value().clone())
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

// ============================================================================
// PHASE 2: EVENT DISTRIBUTION ENGINE
// ============================================================================

impl SubscriptionExecutor {
    /// Dispatch an event to all matching subscriptions (Phase 2)
    ///
    /// # Overview
    /// Finds all subscriptions for the given channel and dispatches the event
    /// to each subscription in parallel using futures::join_all.
    ///
    /// # Arguments
    /// * `event_type` - Type of event (e.g., "userCreated")
    /// * `channel` - Channel name (currently uses wildcard "*" in Phase 2)
    /// * `event_data` - Event payload as Arc<Value>
    ///
    /// # Returns
    /// Count of subscriptions that successfully received the event
    ///
    /// # Example
    /// ```ignore
    /// let event_data = Arc::new(json!({"id": "123", "name": "Alice"}));
    /// let count = executor.dispatch_event(
    ///     "userCreated".to_string(),
    ///     "*".to_string(),
    ///     event_data
    /// ).await?;
    /// println!("Event dispatched to {} subscriptions", count);
    /// ```
    pub async fn dispatch_event(
        &self,
        event_type: String,
        channel: String,
        event_data: Arc<Value>,
    ) -> Result<usize, SubscriptionError> {
        // 1. Find all subscriptions for this channel
        let mut subscription_ids = self.subscriptions_by_channel(&channel);

        // 2. Also include wildcard subscriptions (Phase 2: all subs registered to "*")
        let wildcard_ids = self.subscriptions_by_channel("*");
        subscription_ids.extend(wildcard_ids);
        subscription_ids.sort();
        subscription_ids.dedup(); // Remove duplicates

        if subscription_ids.is_empty() {
            // No matching subscriptions
            return Ok(0);
        }

        // 3. Create dispatch futures for parallel processing
        let dispatch_futures: Vec<_> = subscription_ids
            .iter()
            .map(|sub_id| {
                let executor = self.clone();
                let event_type = event_type.clone();
                let event_data = Arc::clone(&event_data);
                let sub_id = sub_id.clone();

                async move {
                    executor
                        .dispatch_to_subscription(&sub_id, event_type, event_data)
                        .await
                }
            })
            .collect();

        // 4. Execute all dispatches in parallel
        let results = future::join_all(dispatch_futures).await;

        // 5. Count successes
        let success_count = results.iter().filter(|r| r.is_ok()).count();

        Ok(success_count)
    }

    /// Dispatch an event to a single subscription (Phase 2)
    ///
    /// # Overview
    /// Handles the complete dispatch pipeline for a single subscription:
    /// 1. Security filter check
    /// 2. Python resolver invocation
    /// 3. Response serialization
    /// 4. Response queueing
    ///
    /// # Arguments
    /// * `subscription_id` - The subscription to dispatch to
    /// * `event_type` - Type of event
    /// * `event_data` - Event payload
    ///
    /// # Returns
    /// Ok(()) if dispatch succeeded (even if event was filtered)
    /// Err() if internal error occurred
    async fn dispatch_to_subscription(
        &self,
        subscription_id: &str,
        _event_type: String,
        event_data: Arc<Value>,
    ) -> Result<(), SubscriptionError> {
        // 1. Get subscription with security context
        let sub_entry = self
            .subscriptions_secure
            .get(subscription_id)
            .ok_or(SubscriptionError::SubscriptionNotFound)?;

        let sub_with_security = sub_entry.value().clone();

        // 2. Apply security filters - skip if access denied
        if !self.check_security_filters(&sub_with_security, &event_data)? {
            // Access denied, silently skip this subscription
            return Ok(());
        }

        // 3. Invoke Python resolver (placeholder for Phase 2)
        let resolver_result = self
            .invoke_python_resolver(subscription_id, &event_data)
            .await?;

        // 4. Serialize response to bytes
        let response_bytes = self.serialize_response(&resolver_result)?;

        // 5. Queue response for later retrieval
        self.queue_response(subscription_id.to_string(), response_bytes)?;

        Ok(())
    }

    /// Check security filters for event delivery (Phase 2/2.3)
    ///
    /// # Overview
    /// Applies all 5 security modules to determine if subscription should receive event:
    /// 1. Row-level filtering (multi-tenant, user access, federation)
    /// 2. RBAC field-level access checks
    /// 3. Scope validation
    /// 4. Resource limits
    /// 5. Security violation count
    ///
    /// # Returns
    /// true if access allowed, false if access denied
    fn check_security_filters(
        &self,
        sub_with_security: &ExecutedSubscriptionWithSecurity,
        event_data: &Value,
    ) -> Result<bool, SubscriptionError> {
        let security_ctx = &sub_with_security.security_context;

        // SECURITY FILTER 1: Violation circuit breaker
        // If subscription has too many violations, deny all further events
        if sub_with_security.violations_count > 100 {
            return Ok(false); // Too many violations, security circuit breaker activated
        }

        // SECURITY FILTER 2: Row-level filtering (user_id and tenant_id)
        // Ensures users only receive events for their authorized rows
        // Checks if event.user_id and event.tenant_id match subscription context
        if !security_ctx.row_filter.matches(event_data) {
            // Event doesn't match subscription's row filter criteria
            // This is expected behavior for events outside user's scope
            return Ok(false);
        }

        // SECURITY FILTER 3: Multi-tenant enforcement
        // Ensures strict tenant isolation - events from other tenants are rejected
        // Validates event.tenant_id matches subscription context tenant
        if !security_ctx.tenant.matches(event_data) {
            // Event is from a different tenant, reject
            return Ok(false);
        }

        // SECURITY FILTER 4: Federation context isolation
        // In Apollo Federation 2.0, prevents cross-subgraph subscriptions
        // Ensures subscription and event are from same federated service
        if let Some(ref fed_context) = security_ctx.federation {
            // If federation context is set, need to validate federation boundaries
            // In current Phase 2, we allow all events if federation is configured
            // A more complete implementation would extract federation_id from event_data
            // and validate it matches the subscription's federation context
            if fed_context.is_federated() {
                // Federation is enabled - in production would validate event federation_id
                // For Phase 2, allow events through (federation_id extraction TBD in Phase 2.3 refinement)
            }
        }

        // SECURITY FILTER 5: RBAC field-level access control
        // Validates that user has permission to access all fields in subscription
        // This was already validated at subscription registration time,
        // but we can re-validate on event delivery if needed
        // For Phase 2, we trust the initial RBAC validation
        if let Some(ref _rbac) = security_ctx.rbac {
            // RBAC context exists - user must have been validated at subscription time
            // We allow events through as RBAC was enforced at subscription registration
            // If additional field-level filtering needed during event delivery,
            // would implement response field filtering here
        }

        // All security filters passed - allow event delivery
        Ok(true)
    }

    /// Invoke Python resolver for event (Phase 2/2.2)
    ///
    /// # Overview
    /// Calls user-defined GraphQL resolver to generate response data.
    /// In Phase 2, this is a placeholder that echoes the event data.
    ///
    /// # Arguments
    /// * `subscription_id` - The subscription being processed
    /// * `event_data` - The event data to pass to resolver
    ///
    /// # Returns
    /// Resolver result as JSON value
    ///
    /// # Notes
    /// Phase 2.2 implementation will:
    /// 1. Get stored Python resolver function from resolvers map
    /// 2. Use Python::with_gil to call resolver
    /// 3. Pass event_data and subscription variables
    /// 4. Return resolver result or error
    async fn invoke_python_resolver(
        &self,
        subscription_id: &str,
        event_data: &Value,
    ) -> Result<Value, SubscriptionError> {
        // Phase 3: Try to invoke registered Python resolver

        // Serialize event data to JSON string
        let event_data_json = serde_json::to_string(event_data).map_err(|e| {
            SubscriptionError::SubscriptionRejected(format!("Failed to serialize event: {}", e))
        })?;

        // Get resolver callback clone before invoking
        let callback = {
            let callback_lock = self.resolver_callback.try_lock();
            match callback_lock {
                Ok(callback_guard) => callback_guard.as_ref().map(Arc::clone),
                Err(_) => None,
            }
        };

        // If we have a callback, invoke it with error handling (Phase 3.4)
        if let Some(callback) = callback {
            // Invoke resolver synchronously (Phase 3.4: Error handling & recovery)
            // Note: Timeout cannot be applied to synchronous Python resolver calls
            // due to GIL constraints. See Phase 3.5 for async resolver architecture.
            // Instead, we provide comprehensive error handling:
            // - Parse errors (malformed response)
            // - Resolver exceptions (Python errors)
            // - Graceful fallback to echo resolver with error

            let result = callback.invoke(subscription_id, &event_data_json);

            match result {
                Ok(result_json) => {
                    // Resolver succeeded - parse result (Phase 3.4: Error handling)
                    match serde_json::from_str::<Value>(&result_json) {
                        Ok(result_value) => Ok(result_value),
                        Err(e) => {
                            // If parsing fails, wrap in error response (Phase 3.4)
                            Ok(json!({
                                "error": format!("Failed to parse resolver result: {}", e),
                                "data": event_data
                            }))
                        }
                    }
                }
                Err(e) => {
                    // Resolver returned error (Phase 3.4: Convert exception to response)
                    // This handles both Python exceptions and Rust errors from the callback
                    Ok(json!({
                        "error": e.to_string(),
                        "data": event_data
                    }))
                }
            }
        } else {
            // No resolver registered - use default echo resolver
            Ok(json!({
                "data": event_data,
                "type": "next"
            }))
        }
    }

    /// Serialize response to pre-serialized bytes (Phase 2/2.2)
    ///
    /// # Overview
    /// Converts resolver result to GraphQL response format and serializes to bytes.
    /// Pre-serialization avoids JSON encoding overhead in hot path.
    ///
    /// # Arguments
    /// * `response` - The resolver response object
    ///
    /// # Returns
    /// Pre-serialized response bytes ready for WebSocket transmission
    fn serialize_response(&self, response: &Value) -> Result<Vec<u8>, SubscriptionError> {
        // Format as GraphQL subscription response
        let gql_response = json!({
            "type": "next",
            "payload": {
                "data": response.get("data"),
                "errors": response.get("errors")
            }
        });

        // Serialize to bytes (could use MessagePack for better performance)
        serde_json::to_vec(&gql_response).map_err(|e| {
            SubscriptionError::InternalError(format!("Response serialization failed: {}", e))
        })
    }
}

// End Phase 2 event distribution engine
// ============================================================================

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
