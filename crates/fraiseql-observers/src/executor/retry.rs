//! Retry loop, backoff computation, transport and listener integration.

use std::{sync::Arc, time::Duration};

use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::{
    config::{ActionConfig, BackoffStrategy, FailurePolicy, RetryConfig},
    error::{ObserverError, Result},
    event::EntityEvent,
};

use super::{ExecutionSummary, ObserverExecutor};

impl ObserverExecutor {
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
                Err(ObserverError::DeserializationError { ref raw, ref reason }) => {
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
}
