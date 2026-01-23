//! Observer runtime for executing observers in response to database changes.
//!
//! This module integrates the fraiseql-observers crate with the server:
//! 1. Loads observer definitions from tb_observer
//! 2. Starts the ChangeLogListener to poll tb_entity_change_log
//! 3. Routes events through the ObserverExecutor
//! 4. Manages lifecycle (startup/shutdown)

use crate::observers::{Observer, ObserverRepository};
use crate::ServerError;
use fraiseql_observers::{
    ActionConfig as ObserverActionConfig, ChangeLogListener, ChangeLogListenerConfig,
    EventMatcher, FailurePolicy, ObserverDefinition, ObserverExecutor,
    RetryConfig as ObserverRetryConfig,
};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Configuration for the observer runtime
#[derive(Debug, Clone)]
pub struct ObserverRuntimeConfig {
    /// PostgreSQL connection pool
    pub pool: PgPool,

    /// How often to poll for new change log entries (milliseconds)
    pub poll_interval_ms: u64,

    /// Maximum events to fetch per batch
    pub batch_size: usize,

    /// Channel capacity for event backpressure
    pub channel_capacity: usize,

    /// Whether to automatically reload observers on changes
    pub auto_reload: bool,

    /// Interval to check for observer changes (seconds)
    pub reload_interval_secs: u64,
}

impl ObserverRuntimeConfig {
    /// Create config with defaults
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            poll_interval_ms: 100,
            batch_size: 100,
            channel_capacity: 1000,
            auto_reload: true,
            reload_interval_secs: 60,
        }
    }

    /// Set poll interval
    #[must_use]
    pub const fn with_poll_interval(mut self, ms: u64) -> Self {
        self.poll_interval_ms = ms;
        self
    }

    /// Set batch size
    #[must_use]
    pub const fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    /// Set channel capacity
    #[must_use]
    pub const fn with_channel_capacity(mut self, capacity: usize) -> Self {
        self.channel_capacity = capacity;
        self
    }
}

/// Runtime health status
#[derive(Debug, Clone)]
pub struct RuntimeHealth {
    /// Whether the runtime is running
    pub running: bool,

    /// Number of loaded observers
    pub observer_count: usize,

    /// Last checkpoint ID processed
    pub last_checkpoint: Option<i64>,

    /// Total events processed
    pub events_processed: u64,

    /// Total errors encountered
    pub errors: u64,
}

/// Observer runtime that manages the execution loop
pub struct ObserverRuntime {
    config: ObserverRuntimeConfig,
    repository: ObserverRepository,
    running: Arc<AtomicBool>,
    /// Handle to the background processing task
    task_handle: Option<JoinHandle<()>>,
    /// Channel to send shutdown signal
    shutdown_tx: Option<mpsc::Sender<()>>,
    /// Statistics
    events_processed: Arc<std::sync::atomic::AtomicU64>,
    errors: Arc<std::sync::atomic::AtomicU64>,
    observer_count: Arc<std::sync::atomic::AtomicUsize>,
    last_checkpoint: Arc<std::sync::atomic::AtomicI64>,
}

impl ObserverRuntime {
    /// Create a new observer runtime
    pub fn new(config: ObserverRuntimeConfig) -> Self {
        let repository = ObserverRepository::new(config.pool.clone());

        Self {
            config,
            repository,
            running: Arc::new(AtomicBool::new(false)),
            task_handle: None,
            shutdown_tx: None,
            events_processed: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            errors: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            observer_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            last_checkpoint: Arc::new(std::sync::atomic::AtomicI64::new(0)),
        }
    }

    /// Load observers from the database and convert to ObserverDefinitions
    /// Returns (definitions, entity_type_index) tuple
    /// entity_type_index maps (entity_type, event_type) -> observer_id for logging
    async fn load_observers(&self) -> Result<(HashMap<String, ObserverDefinition>, HashMap<(String, String), Vec<i64>>), ServerError> {
        // Load all enabled observers
        let query = crate::observers::ListObserversQuery {
            page: 1,
            page_size: 10000, // Load all
            entity_type: None,
            event_type: None,
            enabled: Some(true),
            include_deleted: false,
        };

        let (observers, _total) = self.repository.list(&query, None).await?;

        let mut definitions = HashMap::new();
        let mut entity_type_index: HashMap<(String, String), Vec<i64>> = HashMap::new();

        for observer in observers {
            match Self::convert_observer(&observer) {
                Ok(definition) => {
                    // Index by (entity_type, event_type) for reverse lookup during logging
                    let entity_type = observer.entity_type.clone().unwrap_or_else(|| "*".to_string());
                    let event_type = observer.event_type.clone().unwrap_or_else(|| "INSERT".to_string());
                    entity_type_index
                        .entry((entity_type, event_type.to_uppercase()))
                        .or_default()
                        .push(observer.pk_observer);

                    definitions.insert(observer.name.clone(), definition);
                }
                Err(e) => {
                    warn!("Failed to convert observer {}: {}", observer.name, e);
                }
            }
        }

        info!("Loaded {} observers from database", definitions.len());
        Ok((definitions, entity_type_index))
    }

    /// Convert database Observer to ObserverDefinition
    fn convert_observer(observer: &Observer) -> Result<ObserverDefinition, ServerError> {
        // Parse actions from JSONB
        let actions: Vec<ObserverActionConfig> = serde_json::from_value(observer.actions.clone())
            .map_err(|e| {
                ServerError::Validation(format!(
                    "Failed to parse actions for observer {}: {}",
                    observer.name, e
                ))
            })?;

        // Parse retry config
        let retry_config: ObserverRetryConfig =
            serde_json::from_value(observer.retry_config.clone()).unwrap_or_default();

        Ok(ObserverDefinition {
            event_type: observer.event_type.clone().unwrap_or_else(|| "INSERT".to_string()),
            entity: observer.entity_type.clone().unwrap_or_else(|| "*".to_string()),
            condition: observer.condition_expression.clone(),
            actions,
            retry: retry_config,
            on_failure: FailurePolicy::default(),
        })
    }

    /// Start the observer runtime
    pub async fn start(&mut self) -> Result<(), ServerError> {
        if self.running.load(Ordering::SeqCst) {
            return Err(ServerError::ConfigError(
                "Observer runtime already running".to_string(),
            ));
        }

        info!("Starting observer runtime...");

        // Load initial observers with entity_type index for logging
        let (observers, entity_type_index) = self.load_observers().await?;
        self.observer_count
            .store(observers.len(), Ordering::SeqCst);

        // Build event matcher
        let matcher = EventMatcher::build(observers).map_err(|e| {
            ServerError::ConfigError(format!("Failed to build event matcher: {}", e))
        })?;

        // Clone matcher for logging (we need it to find matching observers)
        let matcher_for_logging = matcher.clone();

        // Create executor with in-memory DLQ for now
        let dlq = Arc::new(InMemoryDlq::new());
        let executor = Arc::new(ObserverExecutor::new(matcher, dlq));

        // Create change log listener
        let listener_config = ChangeLogListenerConfig::new(self.config.pool.clone())
            .with_poll_interval(self.config.poll_interval_ms)
            .with_batch_size(self.config.batch_size);

        // Create shutdown channel
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        self.shutdown_tx = Some(shutdown_tx);

        // Clone state for the background task
        let running = self.running.clone();
        let events_processed = self.events_processed.clone();
        let errors = self.errors.clone();
        let last_checkpoint = self.last_checkpoint.clone();
        let poll_interval = Duration::from_millis(self.config.poll_interval_ms);
        let pool = self.config.pool.clone();

        info!("🔧 About to spawn background task...");
        running.store(true, Ordering::SeqCst);

        // Spawn background processing task
        info!("🔧 Calling tokio::spawn()...");
        let handle = tokio::spawn(async move {
            let mut listener = ChangeLogListener::new(listener_config);

            info!("🚀 Observer runtime background task SPAWNED");
            info!("⏰ Poll interval: {:?}", poll_interval);
            info!("Observer runtime started, beginning event processing loop");

            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("Observer runtime received shutdown signal");
                        break;
                    }
                    result = listener.next_batch() => {
                        match result {
                            Ok(entries) => {
                                if entries.is_empty() {
                                    // No events, wait before polling again
                                    tokio::time::sleep(poll_interval).await;
                                    continue;
                                }

                                info!("🔄 Processing batch of {} change log entries", entries.len());

                                for entry in &entries {
                                    // Convert ChangeLogEntry to EntityEvent
                                    let event = match entry.to_entity_event() {
                                        Ok(e) => e,
                                        Err(e) => {
                                            errors.fetch_add(1, Ordering::Relaxed);
                                            warn!("Failed to convert change log entry to event: {}", e);
                                            continue;
                                        }
                                    };

                                    // Find matching observers before processing
                                    let matching_observers = matcher_for_logging.find_matches(&event);

                                    // Process event
                                    match executor.process_event(&event).await {
                                        Ok(summary) => {
                                            events_processed.fetch_add(1, Ordering::Relaxed);
                                            debug!(
                                                "Event {} processed: {} actions succeeded, {} skipped",
                                                event.id,
                                                summary.successful_actions,
                                                summary.conditions_skipped
                                            );

                                            // Write execution logs for each matched observer
                                            // Look up observer IDs by (entity_type, event_type)
                                            let event_type_str = event.event_type.as_str().to_uppercase();
                                            if let Some(observer_ids) = entity_type_index.get(&(event.entity_type.clone(), event_type_str.clone())) {
                                                let status = if summary.successful_actions > 0 { "success" } else { "error" };
                                                let duration_ms = if !matching_observers.is_empty() {
                                                    (summary.total_duration_ms / matching_observers.len() as f64) as i32
                                                } else {
                                                    0
                                                };

                                                // Write a log entry for each matched observer
                                                for observer_id in observer_ids {
                                                    let _ = sqlx::query(
                                                        "INSERT INTO tb_observer_log
                                                         (fk_observer, event_id, entity_type, entity_id, event_type, status, duration_ms, attempt_number, max_attempts)
                                                         VALUES ($1, $2, $3, $4, $5, $6, $7, 1, 3)"
                                                    )
                                                    .bind(observer_id)
                                                    .bind(&event.id)
                                                    .bind(&event.entity_type)
                                                    .bind(&event.entity_id.to_string())
                                                    .bind(&event.event_type.as_str())
                                                    .bind(status)
                                                    .bind(duration_ms)
                                                    .execute(&pool)
                                                    .await;
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            errors.fetch_add(1, Ordering::Relaxed);
                                            error!("Failed to process event {}: {}", event.id, e);

                                            // Write error logs for matched observers
                                            let event_type_str = event.event_type.as_str().to_uppercase();
                                            if let Some(observer_ids) = entity_type_index.get(&(event.entity_type.clone(), event_type_str)) {
                                                for observer_id in observer_ids {
                                                    let _ = sqlx::query(
                                                        "INSERT INTO tb_observer_log
                                                         (fk_observer, event_id, entity_type, entity_id, event_type, status, error_message, attempt_number, max_attempts)
                                                         VALUES ($1, $2, $3, $4, $5, 'error', $6, 1, 3)"
                                                    )
                                                    .bind(observer_id)
                                                    .bind(&event.id)
                                                    .bind(&event.entity_type)
                                                    .bind(&event.entity_id.to_string())
                                                    .bind(&event.event_type.as_str())
                                                    .bind(&e.to_string())
                                                    .execute(&pool)
                                                    .await;
                                                }
                                            }
                                        }
                                    }
                                }

                                // Update checkpoint (in-memory and database)
                                if let Some(last_entry) = entries.last() {
                                    last_checkpoint.store(last_entry.id, Ordering::Relaxed);

                                    // Persist checkpoint to database
                                    // Use entity_type as listener_id for now
                                    let listener_id = last_entry.object_type.clone();
                                    let batch_count = entries.len() as i32;

                                    match sqlx::query(
                                        "INSERT INTO observer_checkpoints
                                         (listener_id, last_processed_id, last_processed_at, batch_size, event_count, updated_at)
                                         VALUES ($1, $2, NOW(), $3, $4, NOW())
                                         ON CONFLICT (listener_id)
                                         DO UPDATE SET
                                            last_processed_id = $2,
                                            last_processed_at = NOW(),
                                            batch_size = $3,
                                            event_count = observer_checkpoints.event_count + $4,
                                            updated_at = NOW()"
                                    )
                                    .bind(&listener_id)
                                    .bind(last_entry.id)
                                    .bind(batch_count)
                                    .bind(batch_count)
                                    .execute(&pool)
                                    .await {
                                        Ok(_) => {
                                            info!("✅ Checkpoint saved: listener_id={}, last_id={}", listener_id, last_entry.id);
                                        }
                                        Err(e) => {
                                            error!("❌ Failed to save checkpoint: {}", e);
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                errors.fetch_add(1, Ordering::Relaxed);
                                error!("Failed to fetch entries from change log: {}", e);
                                // Back off on error
                                tokio::time::sleep(Duration::from_secs(1)).await;
                            }
                        }
                    }
                }

                if !running.load(Ordering::SeqCst) {
                    break;
                }
            }

            info!("Observer runtime stopped");
        });

        info!("🔧 tokio::spawn() returned, storing task handle");
        self.task_handle = Some(handle);

        info!("✅ Runtime.start() completed successfully");
        Ok(())
    }

    /// Stop the observer runtime gracefully
    pub async fn stop(&mut self) -> Result<(), ServerError> {
        if !self.running.load(Ordering::SeqCst) {
            return Ok(());
        }

        info!("Stopping observer runtime...");
        self.running.store(false, Ordering::SeqCst);

        // Send shutdown signal
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }

        // Wait for task to complete
        if let Some(handle) = self.task_handle.take() {
            let _ = tokio::time::timeout(Duration::from_secs(10), handle).await;
        }

        info!("Observer runtime stopped");
        Ok(())
    }

    /// Check if the runtime is running
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get runtime health status
    #[must_use]
    pub fn health(&self) -> RuntimeHealth {
        RuntimeHealth {
            running: self.running.load(Ordering::SeqCst),
            observer_count: self.observer_count.load(Ordering::SeqCst),
            last_checkpoint: Some(self.last_checkpoint.load(Ordering::SeqCst)),
            events_processed: self.events_processed.load(Ordering::SeqCst),
            errors: self.errors.load(Ordering::SeqCst),
        }
    }

    /// Reload observers from the database
    pub async fn reload_observers(&self) -> Result<usize, ServerError> {
        let (observers, _entity_type_index) = self.load_observers().await?;
        let count = observers.len();
        self.observer_count.store(count, Ordering::SeqCst);
        // Note: In a production system, we'd need to swap the matcher atomically
        // For now, this just updates the count
        info!("Reloaded {} observers", count);
        Ok(count)
    }
}

/// Simple in-memory Dead Letter Queue for development
struct InMemoryDlq {
    items: std::sync::Mutex<Vec<fraiseql_observers::DlqItem>>,
}

impl InMemoryDlq {
    fn new() -> Self {
        Self {
            items: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl fraiseql_observers::DeadLetterQueue for InMemoryDlq {
    async fn push(
        &self,
        event: fraiseql_observers::EntityEvent,
        action: fraiseql_observers::ActionConfig,
        error: String,
    ) -> fraiseql_observers::Result<uuid::Uuid> {
        let id = uuid::Uuid::new_v4();
        let item = fraiseql_observers::DlqItem {
            id,
            event,
            action,
            error_message: error,
            attempts: 0,
        };
        let mut items = self.items.lock().unwrap();
        items.push(item);
        Ok(id)
    }

    async fn get_pending(&self, limit: i64) -> fraiseql_observers::Result<Vec<fraiseql_observers::DlqItem>> {
        let items = self.items.lock().unwrap();
        Ok(items.iter().take(limit as usize).cloned().collect())
    }

    async fn mark_success(&self, id: uuid::Uuid) -> fraiseql_observers::Result<()> {
        let mut items = self.items.lock().unwrap();
        items.retain(|i| i.id != id);
        Ok(())
    }

    async fn mark_retry_failed(&self, id: uuid::Uuid, _error: &str) -> fraiseql_observers::Result<()> {
        let mut items = self.items.lock().unwrap();
        items.retain(|i| i.id != id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_config_defaults() {
        // This test would require a PgPool which needs a database connection
        // For now, just verify the struct compiles
    }

    #[test]
    fn test_runtime_health_default() {
        let health = RuntimeHealth {
            running: false,
            observer_count: 0,
            last_checkpoint: None,
            events_processed: 0,
            errors: 0,
        };
        assert!(!health.running);
        assert_eq!(health.observer_count, 0);
    }
}
