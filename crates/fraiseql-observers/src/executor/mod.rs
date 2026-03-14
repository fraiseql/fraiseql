//! Main observer executor engine with retry logic and orchestration.
//!
//! This module implements the core execution engine that:
//! 1. Receives events from the listener
//! 2. Matches events to observers using the matcher
//! 3. Evaluates conditions for each observer
//! 4. Executes actions with retry logic
//! 5. Handles failures via Dead Letter Queue

mod actions;
#[cfg(feature = "caching")]
mod cache;
mod dispatch;
mod retry;
mod summary;
#[cfg(test)]
mod tests;

use std::sync::{Arc, atomic::AtomicUsize};

pub(crate) use dispatch::ActionDispatcher;
use dispatch::DefaultActionDispatcher;
pub use summary::ExecutionSummary;
use tracing::{debug, error};

#[cfg(feature = "caching")]
use crate::cache::CacheBackendDyn;
#[cfg(feature = "metrics")]
use crate::metrics::MetricsRegistry;
use crate::{
    actions::{EmailAction, SlackAction, WebhookAction},
    actions_additional::{CacheAction, PushAction, SearchAction, SmsAction},
    error::Result,
    event::EntityEvent,
    matcher::EventMatcher,
    traits::DeadLetterQueue,
};

/// Main observer executor engine
pub struct ObserverExecutor {
    /// Event-to-observer matcher
    pub(super) matcher:           Arc<EventMatcher>,
    /// Condition parser and evaluator
    pub(super) condition_parser:  Arc<crate::condition::ConditionParser>,
    /// Pre-parsed condition AST cache (condition string → compiled AST).
    ///
    /// Condition strings are deterministic: the same string always produces the
    /// same AST, so we can safely cache the parse result indefinitely.  This
    /// avoids re-lexing and re-parsing the condition on every incoming event,
    /// which was the dominant cost at high event throughput.
    pub(super) condition_cache:   dashmap::DashMap<String, crate::condition::ConditionAst>,
    /// Action dispatcher (production or mock)
    pub(super) dispatcher:        Arc<dyn ActionDispatcher>,
    /// Dead letter queue for failed actions
    pub(super) dlq:               Arc<dyn DeadLetterQueue>,
    /// Maximum number of entries the DLQ may hold (`None` = unbounded).
    ///
    /// When this limit is reached the newest entry is dropped and a warning is
    /// logged. Tracked via `dlq_push_count` — a monotonically-increasing push
    /// counter that acts as a conservative approximation of DLQ depth (it does
    /// not decrease when items are retried/acked, so it may trigger the cap
    /// earlier than strictly necessary, which is the safe direction).
    pub(super) max_dlq_size:      Option<usize>,
    /// Monotonically-increasing count of pushes sent to the DLQ.
    pub(super) dlq_push_count:    Arc<AtomicUsize>,
    /// Per-action dispatch timeout in milliseconds.
    ///
    /// When set, each call to `execute_action_internal` is wrapped in a
    /// `tokio::time::timeout`.  A slow or hung action is interrupted and
    /// returns a transient `ActionExecutionFailed` error so the retry loop
    /// can back off and retry.  `None` disables the timeout (default).
    pub(super) action_timeout_ms: Option<u64>,
    /// Optional cache backend for action result caching
    #[cfg(feature = "caching")]
    pub(super) cache_backend:     Option<Arc<dyn CacheBackendDyn>>,
    /// Prometheus metrics registry
    #[cfg(feature = "metrics")]
    pub(super) metrics:           MetricsRegistry,
}

impl ObserverExecutor {
    /// Create a new executor.
    pub fn new(matcher: EventMatcher, dlq: Arc<dyn DeadLetterQueue>) -> Self {
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
            condition_cache: dashmap::DashMap::new(),
            dispatcher,
            dlq,
            max_dlq_size: None,
            dlq_push_count: Arc::new(AtomicUsize::new(0)),
            action_timeout_ms: None,
            #[cfg(feature = "caching")]
            cache_backend: None,
            #[cfg(feature = "metrics")]
            metrics: MetricsRegistry::global().unwrap_or_default(),
        }
    }

    /// Create a new executor with a DLQ size cap.
    ///
    /// When the push counter reaches `max_dlq_size` the executor drops the
    /// newest entry and logs a warning instead of forwarding it to the DLQ,
    /// preventing unbounded memory growth under sustained action failures.
    pub fn new_with_dlq_limit(
        matcher: EventMatcher,
        dlq: Arc<dyn DeadLetterQueue>,
        max_dlq_size: usize,
    ) -> Self {
        let mut executor = Self::new(matcher, dlq);
        executor.max_dlq_size = Some(max_dlq_size);
        executor
    }

    /// Create a new executor with an optional cache backend.
    ///
    /// Only available when the `caching` feature is enabled.
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
            condition_cache: dashmap::DashMap::new(),
            dispatcher,
            dlq,
            max_dlq_size: None,
            dlq_push_count: Arc::new(AtomicUsize::new(0)),
            action_timeout_ms: None,
            cache_backend,
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
            condition_cache: dashmap::DashMap::new(),
            dispatcher,
            dlq,
            max_dlq_size: None,
            dlq_push_count: Arc::new(AtomicUsize::new(0)),
            action_timeout_ms: None,
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
            // Skip if condition is not met.
            // The parsed AST is cached in `condition_cache` so we only lex/parse
            // the condition string once per unique condition across all events.
            if let Some(condition) = &observer.condition {
                let evaluate_result = {
                    if let Some(ast) = self.condition_cache.get(condition.as_str()) {
                        self.condition_parser.evaluate(&ast, event)
                    } else {
                        // Parse, cache, then evaluate.
                        match self.condition_parser.parse(condition) {
                            Ok(ast) => {
                                let result = self.condition_parser.evaluate(&ast, event);
                                self.condition_cache.insert(condition.clone(), ast);
                                result
                            },
                            Err(e) => Err(e),
                        }
                    }
                };

                match evaluate_result {
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
                        if summary.errors.len() < summary::MAX_ERROR_STRINGS {
                            summary.errors.push(e.to_string());
                        }
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
}
