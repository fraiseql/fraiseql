//! Main observer executor engine with retry logic and orchestration.
//!
//! This module implements the core execution engine that:
//! 1. Receives events from the listener
//! 2. Matches events to observers using the matcher
//! 3. Evaluates conditions for each observer
//! 4. Executes actions with retry logic
//! 5. Handles failures via Dead Letter Queue

mod dispatch;
mod summary;

use std::{sync::Arc, time::Duration};

pub(crate) use dispatch::ActionDispatcher;
use dispatch::DefaultActionDispatcher;
pub use summary::ExecutionSummary;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

#[cfg(feature = "caching")]
use crate::cache::{CacheBackendDyn, CachedActionResult};
#[cfg(feature = "metrics")]
use crate::metrics::MetricsRegistry;
use crate::{
    actions::{EmailAction, SlackAction, WebhookAction},
    actions_additional::{CacheAction, PushAction, SearchAction, SmsAction},
    config::{ActionConfig, BackoffStrategy, FailurePolicy, RetryConfig},
    error::{ObserverError, Result},
    event::EntityEvent,
    matcher::EventMatcher,
    traits::{ActionResult, DeadLetterQueue},
};

/// Main observer executor engine
pub struct ObserverExecutor {
    /// Event-to-observer matcher
    matcher:          Arc<EventMatcher>,
    /// Condition parser and evaluator
    condition_parser: Arc<crate::condition::ConditionParser>,
    /// Action dispatcher (production or mock)
    dispatcher:       Arc<dyn ActionDispatcher>,
    /// Dead letter queue for failed actions
    dlq:              Arc<dyn DeadLetterQueue>,
    /// Optional cache backend for action result caching
    #[cfg(feature = "caching")]
    cache_backend:    Option<Arc<dyn CacheBackendDyn>>,
    /// Prometheus metrics registry
    #[cfg(feature = "metrics")]
    metrics:          MetricsRegistry,
}

impl ObserverExecutor {
    /// Create a new executor
    pub fn new(matcher: EventMatcher, dlq: Arc<dyn DeadLetterQueue>) -> Self {
        Self::with_cache(matcher, dlq, None)
    }

    /// Create a new executor with optional cache backend
    #[cfg(feature = "caching")]
    pub fn with_cache(
        matcher: EventMatcher,
        dlq: Arc<dyn DeadLetterQueue>,
        cache_backend: Option<Arc<dyn CacheBackendDyn>>,
    ) -> Self {
        let dispatcher = Arc::new(DefaultActionDispatcher {
            webhook_action: Arc::new(WebhookAction::new()),
            slack_action:   Arc::new(SlackAction::new()),
            email_action:   Arc::new(EmailAction::new()),
            sms_action:     Arc::new(SmsAction::new()),
            push_action:    Arc::new(PushAction::new()),
            search_action:  Arc::new(SearchAction::new()),
            cache_action:   Arc::new(CacheAction::new()),
        });
        Self {
            matcher: Arc::new(matcher),
            condition_parser: Arc::new(crate::condition::ConditionParser::new()),
            dispatcher,
            dlq,
            cache_backend,
            #[cfg(feature = "metrics")]
            metrics: MetricsRegistry::global().unwrap_or_default(),
        }
    }

    /// Create a new executor with optional cache backend (no-op when caching feature disabled)
    #[cfg(not(feature = "caching"))]
    pub fn with_cache(
        matcher: EventMatcher,
        dlq: Arc<dyn DeadLetterQueue>,
        _cache_backend: Option<Arc<dyn std::fmt::Debug>>,
    ) -> Self {
        let dispatcher = Arc::new(DefaultActionDispatcher {
            webhook_action: Arc::new(WebhookAction::new()),
            slack_action:   Arc::new(SlackAction::new()),
            email_action:   Arc::new(EmailAction::new()),
            sms_action:     Arc::new(SmsAction::new()),
            push_action:    Arc::new(PushAction::new()),
            search_action:  Arc::new(SearchAction::new()),
            cache_action:   Arc::new(CacheAction::new()),
        });
        Self {
            matcher: Arc::new(matcher),
            condition_parser: Arc::new(crate::condition::ConditionParser::new()),
            dispatcher,
            dlq,
            #[cfg(feature = "metrics")]
            metrics: MetricsRegistry::global().unwrap_or_default(),
        }
    }

    /// Create an executor with a custom action dispatcher (for testing).
    ///
    /// This constructor is `pub(crate)` so unit tests in this crate can inject a
    /// `MockActionDispatcher` without exposing the internal seam publicly.
    #[cfg(test)]
    pub(crate) fn with_dispatcher(
        matcher: EventMatcher,
        dlq: Arc<dyn DeadLetterQueue>,
        dispatcher: Arc<dyn ActionDispatcher>,
    ) -> Self {
        Self {
            matcher: Arc::new(matcher),
            condition_parser: Arc::new(crate::condition::ConditionParser::new()),
            dispatcher,
            dlq,
            #[cfg(feature = "caching")]
            cache_backend: None,
            #[cfg(feature = "metrics")]
            metrics: MetricsRegistry::global().unwrap_or_default(),
        }
    }

    /// Get a shared reference to the dead letter queue.
    ///
    /// Used by wrappers (e.g. `DedupedObserverExecutor`) to route violation
    /// events to the same DLQ without requiring a separate DLQ reference.
    pub fn dlq(&self) -> Arc<dyn DeadLetterQueue> {
        Arc::clone(&self.dlq)
    }

    /// Process an event through all matching observers
    ///
    /// This is the main entry point. For each matching observer:
    /// 1. Evaluate condition (if present)
    /// 2. Execute actions with retry logic
    /// 3. Handle failures via DLQ
    pub async fn process_event(&self, event: &EntityEvent) -> Result<ExecutionSummary> {
        // Record metrics
        #[cfg(feature = "metrics")]
        self.metrics.event_processed();

        let mut summary = ExecutionSummary::new();
        let matching_observers = self.matcher.find_matches(event);

        debug!(
            "Processing event {} (entity_type: {}, event_type: {:?})",
            event.id, event.entity_type, event.event_type
        );
        debug!("Found {} matching observers for this event", matching_observers.len());

        for observer in matching_observers {
            // Skip if condition is not met
            if let Some(condition) = &observer.condition {
                match self.condition_parser.parse_and_evaluate(condition, event) {
                    Ok(true) => {
                        debug!("Condition passed for observer");
                    },
                    Ok(false) => {
                        debug!("Condition failed, skipping observer");
                        summary.conditions_skipped += 1;
                        continue;
                    },
                    Err(e) => {
                        error!("Condition evaluation error: {}", e);
                        summary.errors.push(e.to_string());
                        continue;
                    },
                }
            }

            // Execute actions for this observer
            for action in &observer.actions {
                self.execute_action_with_retry(
                    action,
                    event,
                    &observer.retry,
                    &observer.on_failure,
                    &mut summary,
                )
                .await;
            }
        }

        Ok(summary)
    }

    /// Execute a single action with retry logic
    pub(crate) async fn execute_action_with_retry(
        &self,
        action: &ActionConfig,
        event: &EntityEvent,
        retry_config: &RetryConfig,
        failure_policy: &FailurePolicy,
        summary: &mut ExecutionSummary,
    ) {
        let mut attempt = 0;

        loop {
            attempt += 1;
            debug!(
                "Action {} execution attempt {}/{}",
                action.action_type(),
                attempt,
                retry_config.max_attempts
            );

            match self.execute_action_internal(action, event).await {
                Ok(result) => {
                    info!("Action {} succeeded in {}ms", action.action_type(), result.duration_ms);
                    // Record metrics for successful action execution
                    #[cfg(feature = "metrics")]
                    self.metrics.action_executed(&result.action_type, result.duration_ms / 1000.0);

                    summary.successful_actions += 1;
                    summary.total_duration_ms += result.duration_ms;
                    return;
                },
                Err(e) => {
                    let is_transient = e.is_transient();

                    if !is_transient {
                        // Permanent error, don't retry
                        warn!("Permanent error in action {}: {}", action.action_type(), e);
                        self.handle_action_failure(action, event, &e, failure_policy, summary)
                            .await;
                        return;
                    }

                    if attempt >= retry_config.max_attempts {
                        // Retries exhausted
                        error!("Action {} failed after {} attempts", action.action_type(), attempt);
                        self.handle_action_failure(action, event, &e, failure_policy, summary)
                            .await;
                        return;
                    }

                    // Calculate backoff and retry
                    let delay = self.calculate_backoff(attempt, retry_config);
                    warn!(
                        "Action {} attempt {} failed: {}. Retrying in {:?}",
                        action.action_type(),
                        attempt,
                        e,
                        delay
                    );

                    sleep(delay).await;
                },
            }
        }
    }

    /// Execute action and return result
    pub(crate) async fn execute_action_internal(
        &self,
        action: &ActionConfig,
        event: &EntityEvent,
    ) -> Result<ActionResult> {
        // Try cache first (skip for CacheAction itself)
        #[cfg(feature = "caching")]
        if !matches!(action, ActionConfig::Cache { .. }) {
            if let Some(cached) = self.try_cache_get(event, action).await {
                return Ok(cached);
            }
        }

        let result = self.dispatcher.dispatch(action, event).await;

        // Cache successful results before returning
        #[cfg(feature = "caching")]
        if let Ok(ref res) = result {
            if !matches!(action, ActionConfig::Cache { .. }) {
                self.cache_store(event, action, res).await;
            }
        }

        result
    }

    /// Handle action failure based on failure policy
    pub(crate) async fn handle_action_failure(
        &self,
        action: &ActionConfig,
        event: &EntityEvent,
        error: &ObserverError,
        failure_policy: &FailurePolicy,
        summary: &mut ExecutionSummary,
    ) {
        // Record error metrics
        #[cfg(feature = "metrics")]
        {
            let error_type = match error.code() {
                crate::error::ObserverErrorCode::ActionExecutionFailed => "execution_failed",
                crate::error::ObserverErrorCode::ActionPermanentlyFailed => "permanently_failed",
                crate::error::ObserverErrorCode::InvalidActionConfig => "invalid_config",
                crate::error::ObserverErrorCode::TemplateRenderingFailed => {
                    "template_rendering_failed"
                },
                crate::error::ObserverErrorCode::DatabaseError => "database_error",
                crate::error::ObserverErrorCode::CircuitBreakerOpen => "circuit_breaker_open",
                _ => "other_error",
            };
            self.metrics.action_error(action.action_type(), error_type);
        }

        match failure_policy {
            FailurePolicy::Log => {
                error!("Action {} failed for event {}: {}", action.action_type(), event.id, error);
                summary.failed_actions += 1;
            },
            FailurePolicy::Alert => {
                error!(
                    "ALERT: Action {} failed for event {}: {}",
                    action.action_type(),
                    event.id,
                    error
                );
                summary.failed_actions += 1;
            },
            FailurePolicy::Dlq => {
                info!(
                    "Moving failed action {} to DLQ for event {}",
                    action.action_type(),
                    event.id
                );
                if let Err(e) =
                    self.dlq.push(event.clone(), action.clone(), error.to_string()).await
                {
                    error!("Failed to push to DLQ: {}", e);
                    summary.dlq_errors += 1;
                }
                summary.failed_actions += 1;
            },
        }
    }

    /// Calculate backoff delay based on attempt number and strategy
    pub(crate) fn calculate_backoff(&self, attempt: u32, config: &RetryConfig) -> Duration {
        let delay_ms = match config.backoff_strategy {
            BackoffStrategy::Exponential => {
                // 2^(attempt-1) * initial_delay, capped at max_delay
                let exponent = attempt - 1;
                let base_delay = config.initial_delay_ms * (2_u64.pow(exponent));
                base_delay.min(config.max_delay_ms)
            },
            BackoffStrategy::Linear => {
                // attempt * initial_delay, capped at max_delay
                let base_delay = config.initial_delay_ms * u64::from(attempt);
                base_delay.min(config.max_delay_ms)
            },
            BackoffStrategy::Fixed => {
                // Always use initial_delay
                config.initial_delay_ms
            },
        };

        Duration::from_millis(delay_ms)
    }

    /// Run observer executor with pluggable event transport
    ///
    /// This is the new transport-agnostic method that works with any `EventTransport`
    /// implementation (PostgreSQL, NATS, in-memory, etc.).
    ///
    /// # Design
    ///
    /// - Uses `Arc<dyn EventTransport>` for runtime transport selection
    /// - Stream-based API for natural tokio integration
    /// - Transport handles reconnection/backoff internally
    /// - ACK happens after successful `process_event()` (at-least-once semantics)
    ///
    /// # Arguments
    ///
    /// * `transport` - Event transport to subscribe to
    /// * `filter` - Event filter for subscription
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: tokio async runtime.
    /// use std::sync::Arc;
    /// use fraiseql_observers::{ObserverExecutor, EventMatcher};
    /// use fraiseql_observers::transport::{InMemoryTransport, EventFilter};
    /// use fraiseql_observers::testing::mocks::MockDeadLetterQueue;
    ///
    /// # async fn example() -> fraiseql_observers::Result<()> {
    /// let matcher = EventMatcher::new();
    /// let dlq = Arc::new(MockDeadLetterQueue::new());
    /// let executor = ObserverExecutor::new(matcher, dlq);
    ///
    /// let transport = Arc::new(InMemoryTransport::new());
    /// executor.run_with_transport(transport, EventFilter::default()).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn run_with_transport(
        &self,
        transport: Arc<dyn crate::transport::EventTransport>,
        filter: crate::transport::EventFilter,
    ) -> Result<()> {
        use futures::StreamExt;

        info!("Starting observer executor with {:?} transport", transport.transport_type());

        // Subscribe to event stream
        let mut event_stream = transport.subscribe(filter).await?;

        // Process events from stream
        while let Some(event_result) = event_stream.next().await {
            match event_result {
                Ok(event) => {
                    debug!("Received event {} from transport", event.id);

                    // Process the event through all matching observers
                    match self.process_event(&event).await {
                        Ok(summary) => {
                            debug!(
                                "Event {} processed: {} successful, {} failed",
                                event.id, summary.successful_actions, summary.failed_actions
                            );
                        },
                        Err(e) => {
                            error!("Failed to process event {}: {}", event.id, e);
                            // Continue processing other events
                        },
                    }

                    // Note: Transport ACKs message internally after we return from process_event()
                    // This ensures at-least-once delivery semantics
                },
                Err(ObserverError::DeserializationError {
                    ref raw,
                    ref reason,
                }) => {
                    // Unparseable payload: preserve raw bytes in DLQ and bump counter.
                    // The message was already ACKed by the transport to prevent infinite
                    // redelivery of permanently broken payloads.
                    error!(
                        bytes = raw.len(),
                        %reason,
                        "Unparseable event from transport — routing raw bytes to DLQ"
                    );
                    #[cfg(feature = "metrics")]
                    self.metrics.deserialization_failure();
                    if let Err(dlq_err) = self.dlq.push_raw(raw, reason).await {
                        error!("Failed to route unparseable event to DLQ: {}", dlq_err);
                    }
                },
                Err(e) => {
                    error!("Transport error: {}", e);
                    // Transport handles retry/backoff internally
                    // Stream will continue after error recovery
                },
            }
        }

        warn!("Event stream ended (transport disconnected or fatal error)");
        Ok(())
    }

    /// Run listener loop: poll for change log entries and process as events
    ///
    /// This is the integration point between `ChangeLogListener` and `ObserverExecutor`.
    /// It continuously polls the change log for new entries, converts them to `EntityEvents`,
    /// and processes them through the observer pipeline.
    ///
    /// # Arguments
    /// * `listener` - Mutable reference to `ChangeLogListener`
    /// * `max_iterations` - Optional limit on polling iterations (for testing)
    ///
    /// # Behavior
    /// - Polls `listener.next_batch()` at configured interval
    /// - Converts each `ChangeLogEntry` to `EntityEvent`
    /// - Processes event through observers
    /// - Implements exponential backoff on database errors (up to 10 retries)
    /// - Skips malformed entries and continues processing
    /// - Continues indefinitely until listener stops or error occurs
    pub async fn run_listener_loop(
        &self,
        listener: &mut crate::listener::ChangeLogListener,
        max_iterations: Option<usize>,
    ) -> Result<()> {
        let mut iteration = 0;
        let mut consecutive_errors = 0;
        const MAX_CONSECUTIVE_ERRORS: u32 = 10;
        const MAX_BACKOFF_MS: u64 = 30000; // 30 seconds

        loop {
            // Check iteration limit (for testing)
            if let Some(max) = max_iterations {
                if iteration >= max {
                    info!("Listener loop reached max iterations: {}", max);
                    break;
                }
                iteration += 1;
            }

            match listener.next_batch().await {
                Ok(entries) => {
                    // Reset error counter on successful batch fetch
                    consecutive_errors = 0;

                    if entries.is_empty() {
                        debug!("No new entries from change log");
                        // Wait before polling again
                        sleep(Duration::from_millis(100)).await;
                        continue;
                    }

                    debug!("Processing {} change log entries", entries.len());

                    let mut conversion_errors = 0;
                    let mut processing_errors = 0;

                    // Convert and process each entry
                    for entry in entries {
                        match entry.to_entity_event() {
                            Ok(event) => {
                                match self.process_event(&event).await {
                                    Ok(summary) => {
                                        debug!(
                                            "Event {} processed: {} successful, {} failed",
                                            event.id,
                                            summary.successful_actions,
                                            summary.failed_actions
                                        );
                                    },
                                    Err(e) => {
                                        error!("Failed to process event: {}", e);
                                        processing_errors += 1;
                                        // Continue processing other entries despite error
                                    },
                                }
                            },
                            Err(e) => {
                                error!("Failed to convert change log entry to event: {}", e);
                                conversion_errors += 1;
                                // Continue processing other entries despite error
                            },
                        }
                    }

                    // Log batch summary
                    if conversion_errors > 0 || processing_errors > 0 {
                        warn!(
                            "Batch processing: {} conversion errors, {} processing errors",
                            conversion_errors, processing_errors
                        );
                    }
                },
                Err(e) => {
                    consecutive_errors += 1;

                    // Exponential backoff: 1s, 2s, 4s, 8s, ..., capped at 30s
                    let backoff_ms = ((1000_u64) * 2_u64.saturating_pow(consecutive_errors - 1))
                        .min(MAX_BACKOFF_MS);

                    error!(
                        "Error fetching from change log (attempt {}): {}",
                        consecutive_errors, e
                    );

                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                        error!(
                            "Max consecutive errors ({}) reached. Stopping listener loop.",
                            MAX_CONSECUTIVE_ERRORS
                        );
                        return Err(e);
                    }

                    warn!(
                        "Exponential backoff: retrying in {}ms ({} attempts so far)",
                        backoff_ms, consecutive_errors
                    );
                    sleep(Duration::from_millis(backoff_ms)).await;
                },
            }
        }

        Ok(())
    }

    /// Start listener in background task
    ///
    /// Spawns a background task that runs the listener loop,
    /// returning immediately with a task handle for the caller
    /// to manage the background execution.
    ///
    /// # Returns
    /// `JoinHandle` for the background listener task
    #[must_use]
    pub fn spawn_listener(
        self: Arc<Self>,
        mut listener: crate::listener::ChangeLogListener,
    ) -> tokio::task::JoinHandle<Result<()>> {
        tokio::spawn(async move { self.run_listener_loop(&mut listener, None).await })
    }

    /// Generate a cache key for action result caching.
    ///
    /// Format: `action_result:{event.id}:{action_type}:{entity_type}:{entity_id}`
    #[cfg(feature = "caching")]
    fn cache_key(event: &EntityEvent, action: &ActionConfig) -> String {
        use std::{
            collections::hash_map::DefaultHasher,
            hash::{Hash, Hasher},
        };

        // Hash the action config for uniqueness
        let mut hasher = DefaultHasher::new();
        format!("{action:?}").hash(&mut hasher);
        let action_hash = hasher.finish();

        format!(
            "action_result:{}:{}:{}:{}",
            event.id, action_hash, event.entity_type, event.entity_id
        )
    }

    /// Try to get cached action result, return None if cache disabled or miss.
    #[cfg(feature = "caching")]
    async fn try_cache_get(
        &self,
        event: &EntityEvent,
        action: &ActionConfig,
    ) -> Option<ActionResult> {
        if let Some(ref cache) = self.cache_backend {
            let cache_key = Self::cache_key(event, action);
            if let Ok(Some(cached)) = cache.get(&cache_key).await {
                debug!("Cache hit for {} ({}ms latency)", action.action_type(), cached.duration_ms);
                #[cfg(feature = "metrics")]
                self.metrics.cache_hit();

                return Some(ActionResult {
                    action_type: cached.action_type,
                    success:     cached.success,
                    message:     cached.message,
                    duration_ms: cached.duration_ms,
                });
            }
        }
        None
    }

    /// Store action result in cache (no-op if cache disabled).
    #[cfg(feature = "caching")]
    async fn cache_store(&self, event: &EntityEvent, action: &ActionConfig, result: &ActionResult) {
        if let Some(ref cache) = self.cache_backend {
            if result.success {
                let cache_key = Self::cache_key(event, action);
                let cached_result = CachedActionResult::new(
                    result.action_type.clone(),
                    result.success,
                    result.message.clone(),
                    result.duration_ms,
                );

                if let Err(e) = cache.set(&cache_key, &cached_result).await {
                    warn!("Failed to cache action result: {}", e);
                }
            }
        }
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{event::EventKind, testing::mocks::MockDeadLetterQueue};

    fn create_test_matcher() -> EventMatcher {
        EventMatcher::new()
    }

    fn create_test_executor() -> ObserverExecutor {
        let matcher = create_test_matcher();
        let dlq = Arc::new(MockDeadLetterQueue::new());
        ObserverExecutor::new(matcher, dlq)
    }

    #[test]
    fn test_executor_creation() {
        let executor = create_test_executor();
        let _ = executor;
    }

    #[test]
    fn test_backoff_exponential() {
        let executor = create_test_executor();
        let config = RetryConfig {
            max_attempts:     5,
            initial_delay_ms: 100,
            max_delay_ms:     5000,
            backoff_strategy: BackoffStrategy::Exponential,
        };

        assert_eq!(executor.calculate_backoff(1, &config).as_millis(), 100);
        assert_eq!(executor.calculate_backoff(2, &config).as_millis(), 200);
        assert_eq!(executor.calculate_backoff(3, &config).as_millis(), 400);
        assert_eq!(executor.calculate_backoff(4, &config).as_millis(), 800);
        assert_eq!(executor.calculate_backoff(5, &config).as_millis(), 1600);
    }

    #[test]
    fn test_backoff_linear() {
        let executor = create_test_executor();
        let config = RetryConfig {
            max_attempts:     5,
            initial_delay_ms: 100,
            max_delay_ms:     5000,
            backoff_strategy: BackoffStrategy::Linear,
        };

        assert_eq!(executor.calculate_backoff(1, &config).as_millis(), 100);
        assert_eq!(executor.calculate_backoff(2, &config).as_millis(), 200);
        assert_eq!(executor.calculate_backoff(3, &config).as_millis(), 300);
        assert_eq!(executor.calculate_backoff(4, &config).as_millis(), 400);
        assert_eq!(executor.calculate_backoff(5, &config).as_millis(), 500);
    }

    #[test]
    fn test_backoff_fixed() {
        let executor = create_test_executor();
        let config = RetryConfig {
            max_attempts:     5,
            initial_delay_ms: 100,
            max_delay_ms:     5000,
            backoff_strategy: BackoffStrategy::Fixed,
        };

        assert_eq!(executor.calculate_backoff(1, &config).as_millis(), 100);
        assert_eq!(executor.calculate_backoff(2, &config).as_millis(), 100);
        assert_eq!(executor.calculate_backoff(3, &config).as_millis(), 100);
        assert_eq!(executor.calculate_backoff(4, &config).as_millis(), 100);
        assert_eq!(executor.calculate_backoff(5, &config).as_millis(), 100);
    }

    #[test]
    fn test_backoff_exponential_cap() {
        let executor = create_test_executor();
        let config = RetryConfig {
            max_attempts:     10,
            initial_delay_ms: 100,
            max_delay_ms:     1000,
            backoff_strategy: BackoffStrategy::Exponential,
        };

        // Should be capped at 1000
        assert_eq!(executor.calculate_backoff(10, &config).as_millis(), 1000);
    }

    #[test]
    fn test_execution_summary_success() {
        let summary = ExecutionSummary {
            successful_actions: 5,
            failed_actions:     0,
            conditions_skipped: 0,
            total_duration_ms:  50.0,
            dlq_errors:         0,
            errors:             vec![],
            duplicate_skipped:  false,
            tenant_rejected:    false,
            cache_hits:         0,
            cache_misses:       0,
        };

        assert!(summary.is_success());
        assert_eq!(summary.total_actions(), 5);
    }

    #[test]
    fn test_execution_summary_failure() {
        let summary = ExecutionSummary {
            successful_actions: 3,
            failed_actions:     1,
            conditions_skipped: 1,
            total_duration_ms:  75.0,
            dlq_errors:         0,
            errors:             vec![],
            duplicate_skipped:  false,
            tenant_rejected:    false,
            cache_hits:         0,
            cache_misses:       0,
        };

        assert!(!summary.is_success());
        assert_eq!(summary.total_actions(), 4);
    }

    #[tokio::test]
    async fn test_process_event_no_matching_observers() {
        let executor = create_test_executor();
        let event = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            uuid::Uuid::new_v4(),
            json!({"total": 100}),
        );

        let summary = executor.process_event(&event).await.unwrap();

        assert!(summary.is_success());
        assert_eq!(summary.successful_actions, 0);
        assert_eq!(summary.failed_actions, 0);
    }

    #[test]
    fn test_retry_config_defaults() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_delay_ms, 100);
        assert_eq!(config.max_delay_ms, 30000);
    }

    // Listener integration tests ()

    #[tokio::test]
    async fn test_run_listener_loop_empty_batch() {
        use sqlx::postgres::PgPool;

        use crate::listener::{ChangeLogListener, ChangeLogListenerConfig};

        let executor = create_test_executor();
        let pool = PgPool::connect_lazy("postgres://localhost/dummy").unwrap();
        let config = ChangeLogListenerConfig::new(pool);
        let mut listener = ChangeLogListener::new(config);

        // Run for 1 iteration - should handle empty batch gracefully
        let result = executor.run_listener_loop(&mut listener, Some(1)).await;

        // Should succeed despite no entries
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_checkpoint_tracking() {
        use sqlx::postgres::PgPool;

        use crate::listener::{ChangeLogListener, ChangeLogListenerConfig};

        let pool = PgPool::connect_lazy("postgres://localhost/dummy").unwrap();
        let config = ChangeLogListenerConfig::new(pool);
        let mut listener = ChangeLogListener::new(config);

        // Initial checkpoint should be 0
        assert_eq!(listener.checkpoint(), 0);

        // Update checkpoint
        listener.set_checkpoint(100);
        assert_eq!(listener.checkpoint(), 100);

        // Checkpoint persists
        assert_eq!(listener.checkpoint(), 100);
    }

    #[tokio::test]
    async fn test_listener_config_builder() {
        use sqlx::postgres::PgPool;

        use crate::listener::ChangeLogListenerConfig;

        let pool = PgPool::connect_lazy("postgres://localhost/dummy").unwrap();
        let config = ChangeLogListenerConfig::new(pool)
            .with_poll_interval(250)
            .with_batch_size(200)
            .with_resume_from(500);

        assert_eq!(config.poll_interval_ms, 250);
        assert_eq!(config.batch_size, 200);
        assert_eq!(config.resume_from_id, Some(500));
    }

    // Error handling and resilience tests ()

    #[tokio::test]
    async fn test_run_listener_loop_with_iteration_limit() {
        use sqlx::postgres::PgPool;

        use crate::listener::{ChangeLogListener, ChangeLogListenerConfig};

        let executor = create_test_executor();
        let pool = PgPool::connect_lazy("postgres://localhost/dummy").unwrap();
        let config = ChangeLogListenerConfig::new(pool);
        let mut listener = ChangeLogListener::new(config);

        // Should complete successfully with iteration limit
        let result = executor.run_listener_loop(&mut listener, Some(3)).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_exponential_backoff_calculation() {
        let executor = create_test_executor();
        let config = RetryConfig {
            max_attempts:     5,
            initial_delay_ms: 100,
            max_delay_ms:     5000,
            backoff_strategy: BackoffStrategy::Exponential,
        };

        // Exponential backoff should double each time
        let delay1 = executor.calculate_backoff(1, &config);
        let delay2 = executor.calculate_backoff(2, &config);
        let delay3 = executor.calculate_backoff(3, &config);

        // 2^0 * 100 = 100
        assert_eq!(delay1.as_millis(), 100);

        // 2^1 * 100 = 200
        assert_eq!(delay2.as_millis(), 200);

        // 2^2 * 100 = 400
        assert_eq!(delay3.as_millis(), 400);
    }

    #[test]
    fn test_exponential_backoff_cap() {
        let executor = create_test_executor();
        let config = RetryConfig {
            max_attempts:     10,
            initial_delay_ms: 100,
            max_delay_ms:     1000,
            backoff_strategy: BackoffStrategy::Exponential,
        };

        // Should cap at max_delay_ms
        let delay8 = executor.calculate_backoff(8, &config);
        let delay9 = executor.calculate_backoff(9, &config);

        // Both should be at max (1000)
        assert!(delay8.as_millis() <= 1000);
        assert!(delay9.as_millis() <= 1000);
    }

    #[tokio::test]
    async fn test_run_listener_loop_zero_iterations() {
        use sqlx::postgres::PgPool;

        use crate::listener::{ChangeLogListener, ChangeLogListenerConfig};

        let executor = create_test_executor();
        let pool = PgPool::connect_lazy("postgres://localhost/dummy").unwrap();
        let config = ChangeLogListenerConfig::new(pool);
        let mut listener = ChangeLogListener::new(config);

        // Should handle zero iterations
        let result = executor.run_listener_loop(&mut listener, Some(0)).await;
        assert!(result.is_ok());
    }

    // =========================================================================
    // Helper: build an executor backed by a MockActionDispatcher
    // =========================================================================

    fn make_mock_executor(
        dispatcher: Arc<crate::testing::mocks::MockActionDispatcher>,
        dlq: Arc<crate::testing::mocks::MockDeadLetterQueue>,
    ) -> ObserverExecutor {
        ObserverExecutor::with_dispatcher(EventMatcher::new(), dlq, dispatcher)
    }

    fn make_mock_executor_with_matcher(
        matcher: EventMatcher,
        dispatcher: Arc<crate::testing::mocks::MockActionDispatcher>,
        dlq: Arc<crate::testing::mocks::MockDeadLetterQueue>,
    ) -> ObserverExecutor {
        ObserverExecutor::with_dispatcher(matcher, dlq, dispatcher)
    }

    fn webhook_action() -> ActionConfig {
        ActionConfig::Webhook {
            url:           Some("https://example.com/hook".to_string()),
            url_env:       None,
            headers:       std::collections::HashMap::new(),
            body_template: None,
        }
    }

    fn test_event() -> crate::event::EntityEvent {
        crate::event::EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            uuid::Uuid::new_v4(),
            json!({"id": 42}),
        )
    }

    fn make_retry(max_attempts: u32, initial_delay_ms: u64) -> RetryConfig {
        RetryConfig {
            max_attempts,
            initial_delay_ms,
            max_delay_ms: 5000,
            backoff_strategy: BackoffStrategy::Fixed,
        }
    }

    // =========================================================================
    // execute_action_with_retry — happy path
    // =========================================================================

    #[tokio::test]
    async fn test_retry_happy_path_succeeds_first_attempt() {
        use crate::testing::mocks::MockActionDispatcher;

        let dispatcher = Arc::new(MockActionDispatcher::new());
        dispatcher.expect_ok("webhook", 5.0);
        let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
        let executor = make_mock_executor(Arc::clone(&dispatcher), Arc::clone(&dlq));

        let action = webhook_action();
        let event = test_event();
        let retry = make_retry(3, 0);
        let failure_policy = FailurePolicy::Log;
        let mut summary = ExecutionSummary::new();

        executor
            .execute_action_with_retry(&action, &event, &retry, &failure_policy, &mut summary)
            .await;

        assert_eq!(summary.successful_actions, 1, "expected 1 success");
        assert_eq!(summary.failed_actions, 0);
        assert_eq!(dispatcher.call_count(), 1, "dispatched exactly once");
    }

    #[tokio::test]
    async fn test_retry_total_duration_accumulated_on_success() {
        use crate::testing::mocks::MockActionDispatcher;

        let dispatcher = Arc::new(MockActionDispatcher::new());
        dispatcher.expect_ok("webhook", 42.0);
        let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
        let executor = make_mock_executor(Arc::clone(&dispatcher), dlq);

        let action = webhook_action();
        let event = test_event();
        let retry = make_retry(3, 0);
        let mut summary = ExecutionSummary::new();

        executor
            .execute_action_with_retry(&action, &event, &retry, &FailurePolicy::Log, &mut summary)
            .await;

        assert!((summary.total_duration_ms - 42.0).abs() < f64::EPSILON);
    }

    // =========================================================================
    // execute_action_with_retry — transient errors + retries
    // =========================================================================

    #[tokio::test]
    async fn test_retry_transient_then_success() {
        use std::sync::atomic::{AtomicU32, Ordering};

        // Custom dispatcher that fails the first call, succeeds the second.
        struct TransientThenOkDispatcher {
            attempts: AtomicU32,
        }

        impl ActionDispatcher for TransientThenOkDispatcher {
            fn dispatch<'a>(
                &'a self,
                action: &'a ActionConfig,
                _event: &'a EntityEvent,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<ActionResult>> + Send + 'a>,
            > {
                let attempt = self.attempts.fetch_add(1, Ordering::SeqCst) + 1;
                let action_type = action.action_type().to_string();
                Box::pin(async move {
                    if attempt == 1 {
                        Err(ObserverError::ActionExecutionFailed {
                            reason: "transient".to_string(),
                        })
                    } else {
                        Ok(ActionResult {
                            action_type,
                            success: true,
                            message: "ok".to_string(),
                            duration_ms: 1.0,
                        })
                    }
                })
            }
        }

        let dispatcher = Arc::new(TransientThenOkDispatcher {
            attempts: AtomicU32::new(0),
        });
        let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
        let executor = ObserverExecutor::with_dispatcher(EventMatcher::new(), dlq, dispatcher);

        let action = webhook_action();
        let event = test_event();
        let retry = make_retry(3, 0);
        let mut summary = ExecutionSummary::new();

        executor
            .execute_action_with_retry(&action, &event, &retry, &FailurePolicy::Log, &mut summary)
            .await;

        assert_eq!(summary.successful_actions, 1);
        assert_eq!(summary.failed_actions, 0);
    }

    #[tokio::test]
    async fn test_retry_permanent_error_no_retry() {
        use crate::testing::mocks::MockActionDispatcher;

        let dispatcher = Arc::new(MockActionDispatcher::new());
        // ActionPermanentlyFailed is NOT transient — should not retry
        dispatcher.expect_err(
            "webhook",
            ObserverError::ActionPermanentlyFailed {
                reason: "bad config".to_string(),
            },
        );
        let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
        let executor = make_mock_executor(Arc::clone(&dispatcher), dlq);

        let action = webhook_action();
        let event = test_event();
        let retry = make_retry(5, 0);
        let mut summary = ExecutionSummary::new();

        executor
            .execute_action_with_retry(&action, &event, &retry, &FailurePolicy::Log, &mut summary)
            .await;

        // Called exactly once — no retry for permanent errors
        assert_eq!(dispatcher.call_count(), 1);
        assert_eq!(summary.successful_actions, 0);
        assert_eq!(summary.failed_actions, 1);
    }

    #[tokio::test]
    async fn test_retry_exhausted_after_max_attempts() {
        use crate::testing::mocks::MockActionDispatcher;

        let dispatcher = Arc::new(MockActionDispatcher::new());
        // ActionExecutionFailed IS transient — will retry until exhaustion
        dispatcher.expect_err(
            "webhook",
            ObserverError::ActionExecutionFailed {
                reason: "timeout".to_string(),
            },
        );
        let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
        let executor = make_mock_executor(Arc::clone(&dispatcher), dlq);

        let action = webhook_action();
        let event = test_event();
        let retry = make_retry(3, 0);
        let mut summary = ExecutionSummary::new();

        executor
            .execute_action_with_retry(&action, &event, &retry, &FailurePolicy::Log, &mut summary)
            .await;

        // Should have been called max_attempts times (3) then failed
        assert_eq!(dispatcher.call_count(), 3);
        assert_eq!(summary.failed_actions, 1);
        assert_eq!(summary.successful_actions, 0);
    }

    #[tokio::test]
    async fn test_retry_single_attempt_max_no_retry() {
        use crate::testing::mocks::MockActionDispatcher;

        let dispatcher = Arc::new(MockActionDispatcher::new());
        dispatcher.expect_err(
            "webhook",
            ObserverError::ActionExecutionFailed {
                reason: "timeout".to_string(),
            },
        );
        let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
        let executor = make_mock_executor(Arc::clone(&dispatcher), dlq);

        let action = webhook_action();
        let event = test_event();
        // max_attempts=1 → no retries at all
        let retry = make_retry(1, 0);
        let mut summary = ExecutionSummary::new();

        executor
            .execute_action_with_retry(&action, &event, &retry, &FailurePolicy::Log, &mut summary)
            .await;

        assert_eq!(dispatcher.call_count(), 1);
        assert_eq!(summary.failed_actions, 1);
    }

    #[tokio::test]
    async fn test_retry_two_transient_then_success() {
        use std::sync::atomic::{AtomicU32, Ordering};

        struct FailTwiceThenOk {
            count: AtomicU32,
        }

        impl ActionDispatcher for FailTwiceThenOk {
            fn dispatch<'a>(
                &'a self,
                action: &'a ActionConfig,
                _event: &'a EntityEvent,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<ActionResult>> + Send + 'a>,
            > {
                let n = self.count.fetch_add(1, Ordering::SeqCst) + 1;
                let at = action.action_type().to_string();
                Box::pin(async move {
                    if n <= 2 {
                        Err(ObserverError::ActionExecutionFailed {
                            reason: format!("transient attempt {n}"),
                        })
                    } else {
                        Ok(ActionResult {
                            action_type: at,
                            success:     true,
                            message:     "ok".to_string(),
                            duration_ms: 1.0,
                        })
                    }
                })
            }
        }

        let dispatcher = Arc::new(FailTwiceThenOk {
            count: AtomicU32::new(0),
        });
        let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
        let executor = ObserverExecutor::with_dispatcher(EventMatcher::new(), dlq, dispatcher);

        let mut summary = ExecutionSummary::new();
        executor
            .execute_action_with_retry(
                &webhook_action(),
                &test_event(),
                &make_retry(5, 0),
                &FailurePolicy::Log,
                &mut summary,
            )
            .await;

        assert_eq!(summary.successful_actions, 1);
        assert_eq!(summary.failed_actions, 0);
    }

    // =========================================================================
    // handle_action_failure — FailurePolicy branches
    // =========================================================================

    #[tokio::test]
    async fn test_failure_policy_log_increments_failed_actions() {
        let executor = create_test_executor();
        let action = webhook_action();
        let event = test_event();
        let error = ObserverError::ActionExecutionFailed {
            reason: "SMTP timeout".to_string(),
        };
        let mut summary = ExecutionSummary::new();

        executor
            .handle_action_failure(&action, &event, &error, &FailurePolicy::Log, &mut summary)
            .await;

        assert_eq!(summary.failed_actions, 1);
        assert_eq!(summary.dlq_errors, 0);
    }

    #[tokio::test]
    async fn test_failure_policy_alert_increments_failed_actions() {
        let executor = create_test_executor();
        let action = webhook_action();
        let event = test_event();
        let error = ObserverError::ActionExecutionFailed {
            reason: "network error".to_string(),
        };
        let mut summary = ExecutionSummary::new();

        executor
            .handle_action_failure(&action, &event, &error, &FailurePolicy::Alert, &mut summary)
            .await;

        assert_eq!(summary.failed_actions, 1);
        assert_eq!(summary.dlq_errors, 0);
    }

    #[tokio::test]
    async fn test_failure_policy_dlq_success_no_dlq_error() {
        // MockDeadLetterQueue.push() always succeeds → dlq_errors should remain 0
        let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
        let dispatcher = Arc::new(crate::testing::mocks::MockActionDispatcher::new());
        let executor = make_mock_executor(Arc::clone(&dispatcher), Arc::clone(&dlq));
        let action = webhook_action();
        let event = test_event();
        let error = ObserverError::ActionExecutionFailed {
            reason: "timeout".to_string(),
        };
        let mut summary = ExecutionSummary::new();

        executor
            .handle_action_failure(&action, &event, &error, &FailurePolicy::Dlq, &mut summary)
            .await;

        assert_eq!(summary.failed_actions, 1);
        assert_eq!(summary.dlq_errors, 0);
        // Item was pushed to the DLQ
        assert_eq!(dlq.item_count(), 1);
    }

    #[tokio::test]
    async fn test_failure_policy_dlq_error_counted() {
        use async_trait::async_trait;
        use uuid::Uuid;

        use crate::traits::{DeadLetterQueue, DlqItem};

        /// A DLQ that always returns an error from push().
        struct AlwaysFailDlq;

        #[async_trait]
        impl DeadLetterQueue for AlwaysFailDlq {
            async fn push(
                &self,
                _event: EntityEvent,
                _action: ActionConfig,
                _error: String,
            ) -> Result<Uuid> {
                Err(ObserverError::DlqError {
                    reason: "redis unavailable".to_string(),
                })
            }

            async fn get_pending(&self, _limit: i64) -> Result<Vec<DlqItem>> {
                Ok(vec![])
            }

            async fn mark_success(&self, _id: Uuid) -> Result<()> {
                Ok(())
            }

            async fn mark_retry_failed(&self, _id: Uuid, _error: &str) -> Result<()> {
                Ok(())
            }
        }

        let failing_dlq = Arc::new(AlwaysFailDlq);
        let dispatcher = Arc::new(crate::testing::mocks::MockActionDispatcher::new());
        let executor =
            ObserverExecutor::with_dispatcher(EventMatcher::new(), failing_dlq, dispatcher);
        let action = webhook_action();
        let event = test_event();
        let error = ObserverError::ActionExecutionFailed {
            reason: "timeout".to_string(),
        };
        let mut summary = ExecutionSummary::new();

        executor
            .handle_action_failure(&action, &event, &error, &FailurePolicy::Dlq, &mut summary)
            .await;

        assert_eq!(summary.dlq_errors, 1);
        assert_eq!(summary.failed_actions, 1);
    }

    #[tokio::test]
    async fn test_failure_policy_log_does_not_touch_dlq() {
        let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
        let dispatcher = Arc::new(crate::testing::mocks::MockActionDispatcher::new());
        let executor = make_mock_executor(Arc::clone(&dispatcher), Arc::clone(&dlq));

        let mut summary = ExecutionSummary::new();
        executor
            .handle_action_failure(
                &webhook_action(),
                &test_event(),
                &ObserverError::ActionExecutionFailed {
                    reason: "err".to_string(),
                },
                &FailurePolicy::Log,
                &mut summary,
            )
            .await;

        // DLQ must have received no pushes
        assert_eq!(dlq.item_count(), 0);
    }

    // =========================================================================
    // execute_action_internal — dispatch-level tests
    // =========================================================================

    #[tokio::test]
    async fn test_dispatch_webhook_missing_url_returns_invalid_config() {
        // DefaultActionDispatcher handles the missing-URL case directly.
        let executor = create_test_executor();
        let action = ActionConfig::Webhook {
            url:           None,
            url_env:       None,
            headers:       std::collections::HashMap::new(),
            body_template: None,
        };
        let event = test_event();

        let result = executor.execute_action_internal(&action, &event).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ObserverError::InvalidActionConfig { .. }));
    }

    #[tokio::test]
    async fn test_dispatch_webhook_url_env_var_missing_returns_error_with_var_name() {
        let executor = create_test_executor();
        let action = ActionConfig::Webhook {
            url:           None,
            url_env:       Some("FRAISEQL_TEST_WEBHOOK_URL_DEFINITELY_NOT_SET".to_string()),
            headers:       std::collections::HashMap::new(),
            body_template: None,
        };
        let event = test_event();

        let result = executor.execute_action_internal(&action, &event).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        if let ObserverError::InvalidActionConfig { reason } = err {
            assert!(
                reason.contains("FRAISEQL_TEST_WEBHOOK_URL_DEFINITELY_NOT_SET"),
                "error should mention the missing env var name, got: {reason}"
            );
        } else {
            panic!("expected InvalidActionConfig, got {err:?}");
        }
    }

    #[tokio::test]
    async fn test_dispatch_slack_missing_webhook_url_returns_invalid_config() {
        let executor = create_test_executor();
        let action = ActionConfig::Slack {
            webhook_url:      None,
            webhook_url_env:  None,
            channel:          None,
            message_template: None,
        };
        let event = test_event();

        let result = executor.execute_action_internal(&action, &event).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ObserverError::InvalidActionConfig { .. }));
    }

    #[tokio::test]
    async fn test_dispatch_email_missing_to_returns_invalid_config() {
        let executor = create_test_executor();
        let action = ActionConfig::Email {
            to:               None,
            to_template:      None,
            subject:          Some("Hello".to_string()),
            subject_template: None,
            body_template:    Some("body".to_string()),
            reply_to:         None,
        };
        let event = test_event();

        let result = executor.execute_action_internal(&action, &event).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ObserverError::InvalidActionConfig { .. }));
    }

    #[tokio::test]
    async fn test_dispatch_email_missing_subject_returns_invalid_config() {
        let executor = create_test_executor();
        let action = ActionConfig::Email {
            to:               Some("user@example.com".to_string()),
            to_template:      None,
            subject:          None,
            subject_template: None,
            body_template:    Some("body".to_string()),
            reply_to:         None,
        };
        let event = test_event();

        let result = executor.execute_action_internal(&action, &event).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ObserverError::InvalidActionConfig { .. }));
    }

    #[tokio::test]
    async fn test_dispatch_sms_missing_phone_returns_invalid_config() {
        let executor = create_test_executor();
        let action = ActionConfig::Sms {
            phone:            None,
            phone_template:   None,
            message_template: Some("Hi".to_string()),
        };
        let event = test_event();

        let result = executor.execute_action_internal(&action, &event).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ObserverError::InvalidActionConfig { .. }));
    }

    #[tokio::test]
    async fn test_dispatch_push_missing_device_token_returns_invalid_config() {
        let executor = create_test_executor();
        let action = ActionConfig::Push {
            device_token:   None,
            title_template: Some("title".to_string()),
            body_template:  Some("body".to_string()),
        };
        let event = test_event();

        let result = executor.execute_action_internal(&action, &event).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ObserverError::InvalidActionConfig { .. }));
    }

    #[tokio::test]
    async fn test_dispatch_slack_url_env_var_missing_error() {
        let executor = create_test_executor();
        let action = ActionConfig::Slack {
            webhook_url:      None,
            webhook_url_env:  Some("FRAISEQL_TEST_SLACK_URL_MISSING_VAR".to_string()),
            channel:          None,
            message_template: None,
        };
        let event = test_event();

        let result = executor.execute_action_internal(&action, &event).await;

        assert!(result.is_err());
        if let ObserverError::InvalidActionConfig { reason } = result.unwrap_err() {
            assert!(
                reason.contains("FRAISEQL_TEST_SLACK_URL_MISSING_VAR"),
                "reason should mention var name, got: {reason}"
            );
        } else {
            panic!("expected InvalidActionConfig");
        }
    }

    // =========================================================================
    // process_event integration tests with MockActionDispatcher
    // =========================================================================

    #[tokio::test]
    async fn test_process_event_no_matching_observers_returns_empty_summary() {
        // Empty matcher → no observers match → summary zeroed
        let dispatcher = Arc::new(crate::testing::mocks::MockActionDispatcher::new());
        let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
        let executor = make_mock_executor(Arc::clone(&dispatcher), dlq);

        let event = test_event();
        let summary = executor.process_event(&event).await.unwrap();

        assert_eq!(summary.successful_actions, 0);
        assert_eq!(summary.failed_actions, 0);
        assert_eq!(dispatcher.call_count(), 0);
    }

    #[tokio::test]
    async fn test_process_event_with_mock_dispatcher_success() {
        use crate::config::{FailurePolicy as FP, ObserverDefinition, RetryConfig};

        let dispatcher = Arc::new(crate::testing::mocks::MockActionDispatcher::new());
        dispatcher.expect_ok("webhook", 10.0);

        let observer = ObserverDefinition {
            event_type: "INSERT".to_string(),
            entity:     "Order".to_string(),
            condition:  None,
            actions:    vec![webhook_action()],
            retry:      RetryConfig {
                max_attempts: 1,
                initial_delay_ms: 0,
                ..RetryConfig::default()
            },
            on_failure: FP::Log,
        };
        let mut observers = std::collections::HashMap::new();
        observers.insert("obs".to_string(), observer);
        let matcher = EventMatcher::build(observers).unwrap();

        let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
        let executor = make_mock_executor_with_matcher(matcher, Arc::clone(&dispatcher), dlq);

        let event = test_event();
        let summary = executor.process_event(&event).await.unwrap();

        assert_eq!(summary.successful_actions, 1);
        assert_eq!(summary.failed_actions, 0);
        assert_eq!(dispatcher.call_count(), 1);
    }

    #[tokio::test]
    async fn test_process_event_mock_dispatcher_failure_goes_to_log_policy() {
        use crate::config::{FailurePolicy as FP, ObserverDefinition, RetryConfig};

        let dispatcher = Arc::new(crate::testing::mocks::MockActionDispatcher::new());
        dispatcher.expect_err(
            "webhook",
            ObserverError::ActionPermanentlyFailed {
                reason: "stub failure".to_string(),
            },
        );

        let observer = ObserverDefinition {
            event_type: "INSERT".to_string(),
            entity:     "Order".to_string(),
            condition:  None,
            actions:    vec![webhook_action()],
            retry:      RetryConfig {
                max_attempts: 1,
                initial_delay_ms: 0,
                ..RetryConfig::default()
            },
            on_failure: FP::Log,
        };
        let mut observers = std::collections::HashMap::new();
        observers.insert("obs".to_string(), observer);
        let matcher = EventMatcher::build(observers).unwrap();

        let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
        let executor = make_mock_executor_with_matcher(matcher, Arc::clone(&dispatcher), dlq);

        let summary = executor.process_event(&test_event()).await.unwrap();

        assert_eq!(summary.failed_actions, 1);
        assert_eq!(summary.successful_actions, 0);
    }

    #[tokio::test]
    async fn test_process_event_condition_false_skips_action() {
        use crate::config::{FailurePolicy as FP, ObserverDefinition, RetryConfig};

        let dispatcher = Arc::new(crate::testing::mocks::MockActionDispatcher::new());
        dispatcher.expect_ok("webhook", 1.0);

        // Condition always false: id == 99999 won't match json({"id":42})
        // Using a numeric field that exists so eval_comparison returns Ok(false)
        let observer = ObserverDefinition {
            event_type: "INSERT".to_string(),
            entity:     "Order".to_string(),
            condition:  Some("id == 99999".to_string()),
            actions:    vec![webhook_action()],
            retry:      RetryConfig::default(),
            on_failure: FP::Log,
        };
        let mut observers = std::collections::HashMap::new();
        observers.insert("obs".to_string(), observer);
        let matcher = EventMatcher::build(observers).unwrap();

        let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
        let executor = make_mock_executor_with_matcher(matcher, Arc::clone(&dispatcher), dlq);

        let summary = executor.process_event(&test_event()).await.unwrap();

        // Condition skipped → no dispatch, no failure
        assert_eq!(summary.conditions_skipped, 1);
        assert_eq!(summary.successful_actions, 0);
        assert_eq!(dispatcher.call_count(), 0);
    }

    #[tokio::test]
    async fn test_process_event_multiple_observers_all_succeed() {
        use crate::config::{FailurePolicy as FP, ObserverDefinition, RetryConfig};

        let dispatcher = Arc::new(crate::testing::mocks::MockActionDispatcher::new());
        dispatcher.expect_ok("webhook", 5.0);

        // Build three observers with unique names so the matcher registers all three.
        let mut observers_map = std::collections::HashMap::new();
        for i in 0..3usize {
            let observer = ObserverDefinition {
                event_type: "INSERT".to_string(),
                entity:     "Order".to_string(),
                condition:  None,
                actions:    vec![webhook_action()],
                retry:      RetryConfig {
                    max_attempts: 1,
                    initial_delay_ms: 0,
                    ..RetryConfig::default()
                },
                on_failure: FP::Log,
            };
            observers_map.insert(format!("obs_{i}"), observer);
        }
        let matcher = EventMatcher::build(observers_map).unwrap();

        let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
        let executor = make_mock_executor_with_matcher(matcher, Arc::clone(&dispatcher), dlq);

        let summary = executor.process_event(&test_event()).await.unwrap();

        assert_eq!(summary.successful_actions, 3);
        assert_eq!(dispatcher.call_count(), 3);
    }

    #[tokio::test]
    async fn test_process_event_multiple_actions_in_one_observer() {
        use crate::config::{FailurePolicy as FP, ObserverDefinition, RetryConfig};

        let dispatcher = Arc::new(crate::testing::mocks::MockActionDispatcher::new());
        dispatcher.expect_ok("webhook", 5.0);
        dispatcher.expect_ok("cache", 2.0);

        let observer = ObserverDefinition {
            event_type: "INSERT".to_string(),
            entity:     "Order".to_string(),
            condition:  None,
            actions:    vec![
                webhook_action(),
                ActionConfig::Cache {
                    key_pattern: "orders:*".to_string(),
                    action:      "invalidate".to_string(),
                },
            ],
            retry:      RetryConfig {
                max_attempts: 1,
                initial_delay_ms: 0,
                ..RetryConfig::default()
            },
            on_failure: FP::Log,
        };
        let mut observers = std::collections::HashMap::new();
        observers.insert("obs".to_string(), observer);
        let matcher = EventMatcher::build(observers).unwrap();

        let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
        let executor = make_mock_executor_with_matcher(matcher, Arc::clone(&dispatcher), dlq);

        let summary = executor.process_event(&test_event()).await.unwrap();

        assert_eq!(summary.successful_actions, 2);
        assert_eq!(dispatcher.call_count(), 2);
    }

    #[tokio::test]
    async fn test_process_event_dlq_policy_pushes_on_failure() {
        use crate::config::{FailurePolicy as FP, ObserverDefinition, RetryConfig};

        let dispatcher = Arc::new(crate::testing::mocks::MockActionDispatcher::new());
        dispatcher.expect_err(
            "webhook",
            ObserverError::ActionPermanentlyFailed {
                reason: "permanent".to_string(),
            },
        );

        let observer = ObserverDefinition {
            event_type: "INSERT".to_string(),
            entity:     "Order".to_string(),
            condition:  None,
            actions:    vec![webhook_action()],
            retry:      RetryConfig {
                max_attempts: 1,
                initial_delay_ms: 0,
                ..RetryConfig::default()
            },
            on_failure: FP::Dlq,
        };
        let mut observers = std::collections::HashMap::new();
        observers.insert("obs".to_string(), observer);
        let matcher = EventMatcher::build(observers).unwrap();

        let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
        let executor =
            make_mock_executor_with_matcher(matcher, Arc::clone(&dispatcher), Arc::clone(&dlq));

        let summary = executor.process_event(&test_event()).await.unwrap();

        assert_eq!(summary.failed_actions, 1);
        assert_eq!(summary.dlq_errors, 0);
        assert_eq!(dlq.item_count(), 1);
    }

    #[tokio::test]
    async fn test_failure_policy_log_does_not_increment_dlq_errors() {
        let executor = create_test_executor();
        let mut summary = ExecutionSummary::new();

        executor
            .handle_action_failure(
                &webhook_action(),
                &test_event(),
                &ObserverError::ActionExecutionFailed {
                    reason: "timeout".to_string(),
                },
                &FailurePolicy::Log,
                &mut summary,
            )
            .await;

        assert_eq!(summary.dlq_errors, 0);
        assert_eq!(summary.failed_actions, 1);
    }

    #[tokio::test]
    async fn test_failure_policy_alert_does_not_increment_dlq_errors() {
        let executor = create_test_executor();
        let mut summary = ExecutionSummary::new();

        executor
            .handle_action_failure(
                &webhook_action(),
                &test_event(),
                &ObserverError::ActionExecutionFailed {
                    reason: "alert test".to_string(),
                },
                &FailurePolicy::Alert,
                &mut summary,
            )
            .await;

        assert_eq!(summary.dlq_errors, 0);
        assert_eq!(summary.failed_actions, 1);
    }

    #[tokio::test]
    async fn test_mock_dispatcher_call_log_records_action_type() {
        use crate::testing::mocks::MockActionDispatcher;

        let dispatcher = Arc::new(MockActionDispatcher::new());
        dispatcher.expect_ok("webhook", 5.0);
        dispatcher.expect_ok("cache", 2.0);

        let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
        let executor = make_mock_executor(Arc::clone(&dispatcher), dlq);

        let actions = [
            webhook_action(),
            ActionConfig::Cache {
                key_pattern: "k:*".to_string(),
                action:      "invalidate".to_string(),
            },
        ];

        for action in &actions {
            let mut s = ExecutionSummary::new();
            executor
                .execute_action_with_retry(
                    action,
                    &test_event(),
                    &make_retry(1, 0),
                    &FailurePolicy::Log,
                    &mut s,
                )
                .await;
        }

        let calls = dispatcher.calls();
        assert_eq!(calls, vec!["webhook", "cache"]);
    }

    #[tokio::test]
    async fn test_execution_summary_is_success_with_mock() {
        use crate::testing::mocks::MockActionDispatcher;

        let dispatcher = Arc::new(MockActionDispatcher::new());
        dispatcher.expect_ok("webhook", 1.0);
        let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
        let executor = make_mock_executor(Arc::clone(&dispatcher), dlq);

        let mut summary = ExecutionSummary::new();
        executor
            .execute_action_with_retry(
                &webhook_action(),
                &test_event(),
                &make_retry(1, 0),
                &FailurePolicy::Log,
                &mut summary,
            )
            .await;

        assert!(summary.is_success());
        assert_eq!(summary.total_actions(), 1);
    }

    #[tokio::test]
    async fn test_execution_summary_not_success_on_failure() {
        use crate::testing::mocks::MockActionDispatcher;

        let dispatcher = Arc::new(MockActionDispatcher::new());
        dispatcher.expect_err(
            "webhook",
            ObserverError::ActionPermanentlyFailed {
                reason: "permanent".to_string(),
            },
        );
        let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
        let executor = make_mock_executor(Arc::clone(&dispatcher), dlq);

        let mut summary = ExecutionSummary::new();
        executor
            .execute_action_with_retry(
                &webhook_action(),
                &test_event(),
                &make_retry(1, 0),
                &FailurePolicy::Log,
                &mut summary,
            )
            .await;

        assert!(!summary.is_success());
        assert_eq!(summary.total_actions(), 1);
    }
}
