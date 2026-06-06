//! Observer runtime for executing observers in response to database changes.
//!
//! This module integrates the fraiseql-observers crate with the server:
//! 1. Loads observer definitions from `tb_observer`
//! 2. Starts the `ChangeLogListener` to poll `tb_entity_change_log`
//! 3. Routes events through the `ObserverExecutor`
//! 4. Manages lifecycle (startup/shutdown)

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use arc_swap::ArcSwap;
use fraiseql_observers::{
    ActionConfig as ObserverActionConfig, ChangeLogListener, ChangeLogListenerConfig,
    EntityEvent as ObserverEntityEvent, EventMatcher, FailurePolicy, InMemoryTransport,
    ObserverDefinition, ObserverExecutor, RetryConfig as ObserverRetryConfig,
    config::{TransportConfig, TransportKind},
    transport::{EventFilter, EventTransport},
};
use futures::StreamExt;
use sqlx::PgPool;
use tokio::{
    sync::{RwLock, mpsc, oneshot},
    task::JoinHandle,
};
use tracing::{debug, error, info, warn};

use crate::{
    ServerError,
    observers::{Observer, ObserverRepository},
    subscriptions::event_bridge::EntityEvent as BridgeEntityEvent,
};

#[cfg(test)]
mod tests;

/// Which event-source path [`ObserverRuntime::start`] takes for a given transport.
///
/// This is the listener-selection *seam* for #350: the PostgreSQL transport
/// drives the existing `ChangeLogListener` (LISTEN/NOTIFY) loop, while NATS and
/// the in-memory transport are driven generically through an
/// [`EventTransport`] stream. Keeping the decision in a pure function makes the
/// selection unit-testable without a database or broker, and guarantees a
/// non-Postgres selection never silently falls through to the PG listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListenerSelection {
    /// PostgreSQL LISTEN/NOTIFY via `ChangeLogListener` (the default path).
    PostgresChangeLog,
    /// An `EventTransport` stream (NATS `JetStream`, or in-memory).
    TransportStream,
}

/// Map a transport kind to the event-source path `start()` will use.
pub(crate) const fn listener_selection(kind: TransportKind) -> ListenerSelection {
    match kind {
        TransportKind::Postgres => ListenerSelection::PostgresChangeLog,
        // NATS, in-memory, and any future fraiseql-observers transport variant
        // run through the generic stream path rather than silently using the PG
        // listener. `init_observer_runtime` has already refused/downgraded any
        // transport this binary cannot run, so reaching `start()` with an
        // unknown kind is benign (and still never lands on the PG listener).
        _ => ListenerSelection::TransportStream,
    }
}

/// Build the transport-layer [`NatsConfig`](fraiseql_observers::transport::NatsConfig)
/// from the operator-facing `[observers.runtime.transport.nats]` settings.
///
/// Note: NATS rejects stream names containing `.` or `_`; an offending
/// `stream_name` surfaces as a loud connection/stream-creation error from
/// `NatsTransport::new` rather than a silent fallback.
#[cfg(feature = "observers-nats")]
fn nats_config_from(
    cfg: &fraiseql_observers::config::NatsTransportConfig,
) -> fraiseql_observers::transport::NatsConfig {
    fraiseql_observers::transport::NatsConfig {
        url: cfg.url.clone(),
        stream_name: cfg.stream_name.clone(),
        consumer_name: cfg.consumer_name.clone(),
        subject_prefix: cfg.subject_prefix.clone(),
        ack_wait_secs: cfg.jetstream.ack_wait_secs,
        retention_max_messages: cfg.jetstream.max_msgs,
        retention_max_bytes: cfg.jetstream.max_bytes,
        ..Default::default()
    }
}

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

    /// Maximum entries the in-memory DLQ may hold (`None` = unbounded).
    ///
    /// Mirrors the `fraiseql-observers` library cap: when reached, the newest
    /// failed entry is dropped (drop-newest) with a warning and an overflow
    /// counter bump.
    pub max_dlq_size: Option<usize>,

    /// Event transport selection (PostgreSQL LISTEN/NOTIFY, NATS, in-memory).
    ///
    /// Resolved at boot from `[observers.runtime.transport]` plus
    /// `FRAISEQL_OBSERVER_TRANSPORT`/`FRAISEQL_NATS_*` env overrides, validated,
    /// and gated by `observer_transport_check` (in `server::initialization`).
    /// Consumed by [`ObserverRuntime::start`] to pick the event source.
    pub transport: TransportConfig,
}

impl ObserverRuntimeConfig {
    /// Create config with defaults (PostgreSQL transport).
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            poll_interval_ms: 100,
            batch_size: 100,
            channel_capacity: 1000,
            auto_reload: true,
            reload_interval_secs: 60,
            max_dlq_size: None,
            transport: TransportConfig::default(),
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

    /// Set the in-memory DLQ size cap (`None` = unbounded).
    #[must_use]
    pub const fn with_max_dlq_size(mut self, max: Option<usize>) -> Self {
        self.max_dlq_size = max;
        self
    }

    /// Set the event transport configuration (already env-overridden + validated).
    #[must_use]
    pub fn with_transport(mut self, transport: TransportConfig) -> Self {
        self.transport = transport;
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
    config:              ObserverRuntimeConfig,
    repository:          ObserverRepository,
    running:             Arc<AtomicBool>,
    /// Handle to the background processing task
    task_handle:         Option<JoinHandle<()>>,
    /// Channel to send shutdown signal
    shutdown_tx:         Option<mpsc::Sender<()>>,
    /// Statistics
    events_processed:    Arc<std::sync::atomic::AtomicU64>,
    errors:              Arc<std::sync::atomic::AtomicU64>,
    observer_count:      Arc<std::sync::atomic::AtomicUsize>,
    last_checkpoint:     Arc<std::sync::atomic::AtomicI64>,
    /// Hot-swappable components for reload
    matcher:             Arc<RwLock<Option<EventMatcher>>>,
    executor:            Arc<RwLock<Option<Arc<ObserverExecutor>>>>,
    /// `(entity_type, event_type)` → list of observer ids that should be logged
    /// for this combination.  Built locally in [`Self::load_observers`] and
    /// republished by an atomic `ArcSwap` store in `start` / `reload_observers`.
    ///
    /// The whole map is rebuilt off-line and then swapped in a single atomic
    /// pointer write, so concurrent CDC-event lookups always observe either
    /// the fully-populated pre-reload generation or the fully-populated
    /// post-reload generation — never a partial or empty index.  Readers
    /// remain lock-free: `load()` returns a cheap snapshot.
    entity_type_index:   Arc<ArcSwap<HashMap<(String, String), Vec<i64>>>>,
    /// In-memory DLQ shared across reloads and exposed to HTTP handlers.
    dlq:                 Arc<InMemoryDlq>,
    /// Optional sender to forward CDC events to `EventBridge` for GraphQL subscriptions
    event_bridge_sender: Option<mpsc::Sender<BridgeEntityEvent>>,
}

impl ObserverRuntime {
    /// Create a new observer runtime
    #[must_use]
    pub fn new(config: ObserverRuntimeConfig) -> Self {
        let repository = ObserverRepository::new(config.pool.clone());
        let max_dlq_size = config.max_dlq_size;

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
            matcher: Arc::new(RwLock::new(None)),
            executor: Arc::new(RwLock::new(None)),
            entity_type_index: Arc::new(ArcSwap::from_pointee(HashMap::new())),
            dlq: Arc::new(InMemoryDlq::new_with_max(max_dlq_size)),
            event_bridge_sender: None,
        }
    }

    /// Set the `EventBridge` sender so CDC events are forwarded to GraphQL subscriptions.
    pub fn set_event_bridge_sender(&mut self, sender: mpsc::Sender<BridgeEntityEvent>) {
        self.event_bridge_sender = Some(sender);
    }

    /// Load observers from the database and convert to `ObserverDefinitions`.
    /// Returns (definitions, `entity_type_index`) tuple.
    /// `entity_type_index` maps (`entity_type`, `event_type`) -> `observer_id` for logging.
    async fn load_observers(
        &self,
    ) -> Result<
        (HashMap<String, ObserverDefinition>, HashMap<(String, String), Vec<i64>>),
        ServerError,
    > {
        // Load all enabled observers
        let query = crate::observers::ListObserversQuery {
            page:            1,
            page_size:       10000, // Load all
            entity_type:     None,
            event_type:      None,
            enabled:         Some(true),
            include_deleted: false,
        };

        let (observers, _total) = self.repository.list(&query, None).await?;

        let mut definitions = HashMap::new();
        let mut entity_type_index: HashMap<(String, String), Vec<i64>> = HashMap::new();

        for observer in observers {
            match Self::convert_observer(&observer) {
                Ok(definition) => {
                    // Index by (entity_type, event_type) for reverse lookup during logging
                    let entity_type =
                        observer.entity_type.clone().unwrap_or_else(|| "*".to_string());
                    let event_type =
                        observer.event_type.clone().unwrap_or_else(|| "INSERT".to_string());
                    entity_type_index
                        .entry((entity_type, event_type.to_uppercase()))
                        .or_default()
                        .push(observer.pk_observer);

                    definitions.insert(observer.name.clone(), definition);
                },
                Err(e) => {
                    warn!("Failed to convert observer {}: {}", observer.name, e);
                },
            }
        }

        info!("Loaded {} observers from database", definitions.len());
        Ok((definitions, entity_type_index))
    }

    /// Convert database Observer to `ObserverDefinition`.
    fn convert_observer(observer: &Observer) -> Result<ObserverDefinition, ServerError> {
        // Parse actions from JSONB
        let actions: Vec<ObserverActionConfig> = serde_json::from_value(observer.actions.clone())
            .map_err(|e| {
            ServerError::Validation(format!(
                "Failed to parse actions for observer {}: {}",
                observer.name, e
            ))
        })?;

        // Parse retry config — fall back to default if deserialization fails, but warn so
        // operators know the stored value is invalid and the observer may behave unexpectedly.
        let retry_config: ObserverRetryConfig =
            match serde_json::from_value(observer.retry_config.clone()) {
                Ok(cfg) => cfg,
                Err(e) => {
                    warn!(
                        observer = %observer.name,
                        error = %e,
                        "Observer retry_config could not be deserialized; using defaults. \
                         Check the stored JSON in tb_observer."
                    );
                    ObserverRetryConfig::default()
                },
            };

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
    ///
    /// Selects the event source by the configured transport (#350): the default
    /// PostgreSQL transport drives the `ChangeLogListener` (LISTEN/NOTIFY) loop
    /// below; NATS `JetStream` and the in-memory transport are driven by the
    /// `start_transport_stream` path. A non-Postgres transport never silently
    /// falls through to the PG listener.
    ///
    /// # Errors
    ///
    /// Returns `ServerError` if the runtime is already running, initialization
    /// fails, or the configured transport cannot connect (e.g. an unreachable
    /// NATS broker) — the latter is the #350 dead-broker boot-failure contract.
    pub async fn start(&mut self) -> Result<(), ServerError> {
        if self.running.load(Ordering::SeqCst) {
            return Err(ServerError::ConfigError("Observer runtime already running".to_string()));
        }

        // Listener-selection seam (#350): a non-Postgres transport runs through
        // the generic EventTransport stream path, never the PG listener. Box the
        // delegated future so the (larger, broker-connecting) stream path does not
        // bloat `start()`'s future — and through it `Server::serve` — past the
        // `clippy::large_futures` budget.
        if listener_selection(self.config.transport.transport) == ListenerSelection::TransportStream
        {
            return Box::pin(self.start_transport_stream()).await;
        }

        info!("Starting observer runtime (PostgreSQL LISTEN/NOTIFY transport)...");

        // Load initial observers with entity_type index for logging
        let (observers, entity_type_index) = self.load_observers().await?;
        self.observer_count.store(observers.len(), Ordering::SeqCst);

        // Build event matcher
        let matcher = EventMatcher::build(observers).map_err(|e| {
            ServerError::ConfigError(format!("Failed to build event matcher: {}", e))
        })?;

        // Clone matcher for logging (we need it to find matching observers)
        let matcher_for_logging = matcher.clone();

        // Create executor with the shared in-memory DLQ
        let executor = Arc::new(ObserverExecutor::new(matcher.clone(), self.dlq.clone()));

        // Store in shared references for hot reload
        {
            let mut m = self.matcher.write().await;
            *m = Some(matcher_for_logging.clone());
        }
        {
            let mut ex = self.executor.write().await;
            *ex = Some(executor.clone());
        }
        // Publish the freshly-built index with a single atomic pointer swap.
        // Concurrent lookup paths see either the empty initial map or this
        // fully-populated generation — never a half-built state.
        self.entity_type_index.store(Arc::new(entity_type_index));

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

        // Clone Arc references for hot reload
        let matcher_ref = Arc::clone(&self.matcher);
        let executor_ref = Arc::clone(&self.executor);
        let entity_type_index_ref = Arc::clone(&self.entity_type_index);

        // Clone optional EventBridge sender for forwarding CDC events to subscriptions
        let bridge_sender = self.event_bridge_sender.clone();

        // Extract non-optional initial values for the background task.
        // SAFETY: These were populated immediately above in this function before we
        // reach this point, so the Option is always Some here.  We surface a proper
        // error rather than panicking so callers get a useful diagnostic.
        let initial_matcher = {
            let m = self.matcher.read().await;
            m.clone().ok_or_else(|| {
                ServerError::ConfigError(
                    "matcher not initialised before spawning background task".to_string(),
                )
            })?
        };
        let initial_executor = {
            let ex = self.executor.read().await;
            ex.clone().ok_or_else(|| {
                ServerError::ConfigError(
                    "executor not initialised before spawning background task".to_string(),
                )
            })?
        };

        debug!("About to spawn background task");
        running.store(true, Ordering::SeqCst);

        // Create a oneshot channel so callers can await readiness before
        // inserting events — eliminates the race between `start()` returning
        // and the background task entering its poll loop.
        let (ready_tx, ready_rx) = oneshot::channel::<()>();

        // Spawn background processing task
        debug!("Calling tokio::spawn()");
        let handle = tokio::spawn(async move {
            let mut listener = ChangeLogListener::new(listener_config);
            // Start with the values captured at launch time; hot-reload replaces them.
            let mut current_matcher = initial_matcher;
            let mut current_executor = initial_executor;

            debug!("Observer runtime background task spawned");
            debug!("Poll interval: {:?}", poll_interval);
            info!("Observer runtime started, beginning event processing loop");

            // Signal that the background task is ready to process events.
            let _ = ready_tx.send(());

            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("Observer runtime received shutdown signal");
                        break;
                    }
                    result = listener.next_batch() => {
                        // Refresh matcher/executor from shared slot in case a hot-reload occurred.
                        {
                            let m = matcher_ref.read().await;
                            if let Some(updated) = m.clone() {
                                current_matcher = updated;
                            }
                        }
                        {
                            let ex = executor_ref.read().await;
                            if let Some(updated) = ex.clone() {
                                current_executor = updated;
                            }
                        }

                        match result {
                            Ok(entries) => {
                                if entries.is_empty() {
                                    // No events, wait before polling again
                                    tokio::time::sleep(poll_interval).await;
                                    continue;
                                }

                                debug!("Processing batch of {} change log entries", entries.len());

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

                                    process_entity_event(
                                        &event,
                                        &current_matcher,
                                        &current_executor,
                                        &entity_type_index_ref,
                                        &pool,
                                        bridge_sender.as_ref(),
                                        &events_processed,
                                        &errors,
                                    )
                                    .await;
                                }

                                // Update checkpoint (in-memory and database)
                                if let Some(last_entry) = entries.last() {
                                    last_checkpoint.store(last_entry.id, Ordering::Relaxed);

                                    // Persist checkpoint to database
                                    // Use entity_type as listener_id for now
                                    let listener_id = last_entry.object_type.clone();
                                    let batch_count = i32::try_from(entries.len()).unwrap_or(i32::MAX);

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
                                            info!("Checkpoint saved: listener_id={}, last_id={}", listener_id, last_entry.id);
                                        }
                                        Err(e) => {
                                            error!("Failed to save checkpoint: {}", e);
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

        debug!("tokio::spawn() returned, storing task handle");
        self.task_handle = Some(handle);

        // Wait for the background task to signal readiness before returning.
        // This ensures callers can safely insert events immediately after start().
        ready_rx.await.map_err(|_| {
            ServerError::ConfigError(
                "observer background task exited before signalling readiness".to_string(),
            )
        })?;

        info!("Runtime started successfully");
        Ok(())
    }

    /// Start the runtime on a generic [`EventTransport`] stream (NATS, in-memory).
    ///
    /// Mirrors the PostgreSQL [`start`](Self::start) setup — load observers, build
    /// the matcher + executor, publish the entity-type index, share state for hot
    /// reload — but sources events from [`EventTransport::subscribe`] instead of
    /// the `ChangeLogListener`. Per-event dispatch, logging, and subscription
    /// forwarding go through the shared [`process_entity_event`] seam so behaviour
    /// matches the PG path. Transport-stream consumers do not persist change-log
    /// checkpoints — delivery state is owned by the transport (e.g. a NATS durable
    /// consumer).
    ///
    /// # Errors
    ///
    /// Returns `ServerError` if observers cannot be loaded, the matcher cannot be
    /// built, or the transport cannot connect/subscribe. An unreachable NATS
    /// broker fails here, before the runtime reports itself started — the #350
    /// dead-broker contract (no silent fallback to PostgreSQL).
    async fn start_transport_stream(&mut self) -> Result<(), ServerError> {
        let kind = self.config.transport.transport;
        info!("Starting observer runtime ({kind:?} transport)...");

        // Build the transport. Connecting happens here, so an unreachable broker
        // fails loudly before the runtime reports itself started (#350).
        let transport: Arc<dyn EventTransport> = match kind {
            TransportKind::InMemory => Arc::new(InMemoryTransport::new()),
            #[cfg(feature = "observers-nats")]
            TransportKind::Nats => {
                let nats_config = nats_config_from(&self.config.transport.nats);
                let connected = fraiseql_observers::transport::NatsTransport::new(nats_config)
                    .await
                    .map_err(|e| {
                        ServerError::ConfigError(format!(
                            "observer NATS transport failed to connect to {}: {e}",
                            self.config.transport.nats.url
                        ))
                    })?;
                Arc::new(connected)
            },
            #[cfg(not(feature = "observers-nats"))]
            TransportKind::Nats => {
                return Err(ServerError::ConfigError(
                    "observer transport = \"nats\" but this binary lacks the observers-nats \
                     feature"
                        .to_string(),
                ));
            },
            other => {
                // Postgres is handled by `start()` and never reaches here; an
                // unknown future kind was already refused/downgraded at init.
                return Err(ServerError::ConfigError(format!(
                    "unsupported observer transport for the stream path: {other:?}"
                )));
            },
        };

        // Load initial observers + build matcher/executor (same as the PG path).
        let (observers, entity_type_index) = self.load_observers().await?;
        self.observer_count.store(observers.len(), Ordering::SeqCst);
        let matcher = EventMatcher::build(observers).map_err(|e| {
            ServerError::ConfigError(format!("Failed to build event matcher: {}", e))
        })?;
        let executor = Arc::new(ObserverExecutor::new(matcher.clone(), self.dlq.clone()));
        {
            let mut m = self.matcher.write().await;
            *m = Some(matcher.clone());
        }
        {
            let mut ex = self.executor.write().await;
            *ex = Some(executor.clone());
        }
        self.entity_type_index.store(Arc::new(entity_type_index));

        // Subscribe to the event stream (a second connection-time failure point).
        let mut stream = transport.subscribe(EventFilter::default()).await.map_err(|e| {
            ServerError::ConfigError(format!("failed to subscribe to observer transport: {e}"))
        })?;

        // Shutdown + readiness channels (same protocol as the PG path).
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        self.shutdown_tx = Some(shutdown_tx);
        let (ready_tx, ready_rx) = oneshot::channel::<()>();

        // Clone task-local state for the background loop.
        let running = self.running.clone();
        let events_processed = self.events_processed.clone();
        let errors = self.errors.clone();
        let pool = self.config.pool.clone();
        let matcher_ref = Arc::clone(&self.matcher);
        let executor_ref = Arc::clone(&self.executor);
        let entity_type_index_ref = Arc::clone(&self.entity_type_index);
        let bridge_sender = self.event_bridge_sender.clone();

        let mut current_matcher = matcher;
        let mut current_executor = executor;

        running.store(true, Ordering::SeqCst);

        let handle = tokio::spawn(async move {
            info!("Observer runtime stream loop started, beginning event processing");
            // Signal readiness so callers can publish immediately after start().
            let _ = ready_tx.send(());

            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("Observer runtime received shutdown signal");
                        break;
                    }
                    maybe_event = stream.next() => {
                        // Refresh matcher/executor in case a hot-reload occurred.
                        {
                            let m = matcher_ref.read().await;
                            if let Some(updated) = m.clone() {
                                current_matcher = updated;
                            }
                        }
                        {
                            let ex = executor_ref.read().await;
                            if let Some(updated) = ex.clone() {
                                current_executor = updated;
                            }
                        }

                        match maybe_event {
                            Some(Ok(event)) => {
                                process_entity_event(
                                    &event,
                                    &current_matcher,
                                    &current_executor,
                                    &entity_type_index_ref,
                                    &pool,
                                    bridge_sender.as_ref(),
                                    &events_processed,
                                    &errors,
                                )
                                .await;
                            },
                            Some(Err(e)) => {
                                errors.fetch_add(1, Ordering::Relaxed);
                                error!("Observer transport stream error: {}", e);
                            },
                            None => {
                                info!("Observer transport stream ended; stopping loop");
                                break;
                            },
                        }
                    }
                }

                if !running.load(Ordering::SeqCst) {
                    break;
                }
            }

            info!("Observer runtime stopped");
        });

        self.task_handle = Some(handle);

        ready_rx.await.map_err(|_| {
            ServerError::ConfigError(
                "observer background task exited before signalling readiness".to_string(),
            )
        })?;

        info!("Observer runtime started ({kind:?} transport)");
        Ok(())
    }

    /// Stop the observer runtime gracefully
    ///
    /// # Errors
    ///
    /// Returns `ServerError` if the shutdown signal fails to send.
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

    /// Whether a [`start`](Self::start) failure on this transport should be fatal
    /// at boot.
    ///
    /// The default PostgreSQL transport keeps the resilient "log and continue
    /// without observers" behaviour on a start failure. A non-Postgres transport
    /// (NATS) is an explicit operator choice for a broker-backed event source; if
    /// it cannot start — e.g. an unreachable broker — the server must not
    /// silently come up *without* it in production (#350). The boot path turns a
    /// start error into a boot failure when this is true and we are in production.
    #[must_use]
    pub(crate) fn transport_requires_broker(&self) -> bool {
        self.config.transport.transport != TransportKind::Postgres
    }

    /// Get a reference to the in-memory DLQ for use by HTTP handlers.
    #[must_use]
    pub(crate) const fn dlq(&self) -> &Arc<InMemoryDlq> {
        &self.dlq
    }

    /// Get a reference to the executor `RwLock` for use by DLQ retry handlers.
    #[must_use]
    pub(crate) const fn executor_ref(&self) -> &Arc<RwLock<Option<Arc<ObserverExecutor>>>> {
        &self.executor
    }

    /// Get runtime health status
    #[must_use]
    pub fn health(&self) -> RuntimeHealth {
        RuntimeHealth {
            running:          self.running.load(Ordering::SeqCst),
            observer_count:   self.observer_count.load(Ordering::SeqCst),
            last_checkpoint:  Some(self.last_checkpoint.load(Ordering::SeqCst)),
            events_processed: self.events_processed.load(Ordering::SeqCst),
            errors:           self.errors.load(Ordering::SeqCst),
        }
    }

    /// Reload observers from the database
    ///
    /// # Errors
    ///
    /// Returns `ServerError::Database` if loading observers fails.
    pub async fn reload_observers(&self) -> Result<usize, ServerError> {
        debug!("Reloading observers from database");

        // Load observers from database
        let (observers, new_entity_type_index) = self.load_observers().await?;
        let count = observers.len();

        // Build new matcher
        let new_matcher = EventMatcher::build(observers)
            .map_err(|e| ServerError::ConfigError(format!("Failed to build matcher: {}", e)))?;

        // Build new executor sharing the existing DLQ
        let new_executor = Arc::new(ObserverExecutor::new(new_matcher.clone(), self.dlq.clone()));

        // Atomic swap - write locks block readers briefly
        debug!("Swapping matcher, executor, and entity_type_index atomically");

        {
            let mut m = self.matcher.write().await;
            *m = Some(new_matcher);
        }

        {
            let mut ex = self.executor.write().await;
            *ex = Some(new_executor);
        }

        // Republish the new index with a single atomic pointer swap.  See
        // the field doc on `entity_type_index`: readers always observe a
        // fully-populated generation (pre-reload or post-reload), never a
        // partial index.
        self.entity_type_index.store(Arc::new(new_entity_type_index));

        // Update count
        self.observer_count.store(count, Ordering::SeqCst);

        info!("Reloaded {} observers successfully", count);
        Ok(count)
    }
}

/// Process a single ready [`EntityEvent`](ObserverEntityEvent): match observers,
/// execute actions, write the per-observer execution log, and forward the event
/// to the GraphQL subscription bridge.
///
/// This is the shared seam between the PostgreSQL `ChangeLogListener` loop and
/// the generic [`EventTransport`] stream loop (#350): both paths converge here so
/// observer dispatch, logging, and subscription forwarding behave identically
/// regardless of how the event was sourced. The PostgreSQL loop additionally
/// persists a change-log checkpoint per batch (a PG-specific concern that stays
/// in that loop); transport-stream consumers manage their own delivery state.
#[allow(clippy::too_many_arguments)]
// Reason: a shared event-processing seam threading the runtime's task-local state
// (counters, shared index, pool, bridge sender) into both event-source loops.
async fn process_entity_event(
    event: &ObserverEntityEvent,
    matcher: &EventMatcher,
    executor: &Arc<ObserverExecutor>,
    entity_type_index: &ArcSwap<HashMap<(String, String), Vec<i64>>>,
    pool: &PgPool,
    bridge_sender: Option<&mpsc::Sender<BridgeEntityEvent>>,
    events_processed: &std::sync::atomic::AtomicU64,
    errors: &std::sync::atomic::AtomicU64,
) {
    // Find matching observers using the current (possibly reloaded) matcher.
    let matching_observers = matcher.find_matches(event);

    // Process event using the current (possibly reloaded) executor.
    let process_result = executor.process_event(event).await;

    match process_result {
        Ok(summary) => {
            events_processed.fetch_add(1, Ordering::Relaxed);
            debug!(
                "Event {} processed: {} actions succeeded, {} skipped",
                event.id, summary.successful_actions, summary.conditions_skipped
            );

            // Write execution logs for each matched observer.
            // Look up observer IDs by (entity_type, event_type) from shared reference.
            let event_type_str = event.event_type.as_str().to_uppercase();
            let observer_ids = entity_type_index
                .load()
                .get(&(event.entity_type.clone(), event_type_str.clone()))
                .cloned();
            if let Some(observer_ids) = observer_ids {
                let status = if summary.successful_actions > 0 {
                    "success"
                } else {
                    "error"
                };
                let duration_ms = if matching_observers.is_empty() {
                    0
                } else {
                    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                    // Reason: observer count is small; average duration fits in i32
                    {
                        (summary.total_duration_ms / matching_observers.len() as f64) as i32
                    }
                };

                // Write a log entry for each matched observer.
                for observer_id in observer_ids {
                    let _ = sqlx::query(
                        // entity_id is bound as text (Uuid::to_string) and cast
                        // to the column's uuid type — sqlx will not implicitly
                        // coerce text to uuid on INSERT.
                        "INSERT INTO tb_observer_log
                         (fk_observer, event_id, entity_type, entity_id, event_type, status, duration_ms, attempt_number, max_attempts)
                         VALUES ($1, $2, $3, $4::uuid, $5, $6, $7, 1, 3)",
                    )
                    .bind(observer_id)
                    .bind(event.id)
                    .bind(&event.entity_type)
                    .bind(event.entity_id.to_string())
                    .bind(event.event_type.as_str())
                    .bind(status)
                    .bind(duration_ms)
                    .execute(pool)
                    .await;
                }
            }

            // Forward processed event to EventBridge for GraphQL subscription delivery.
            if let Some(sender) = bridge_sender {
                let mut bridge_event = BridgeEntityEvent::new(
                    &event.entity_type,
                    event.entity_id.to_string(),
                    event.event_type.as_str(),
                    event.data.clone(),
                );
                // Propagate tenant_id for multi-tenant filtering.
                if let Some(ref tid) = event.tenant_id {
                    bridge_event = bridge_event.with_tenant_id(tid);
                }
                if let Err(e) = sender.try_send(bridge_event) {
                    warn!("Failed to forward event {} to EventBridge: {}", event.id, e);
                }
            }
        },
        Err(e) => {
            errors.fetch_add(1, Ordering::Relaxed);
            error!("Failed to process event {}: {}", event.id, e);

            // Write error logs for matched observers.
            let event_type_str = event.event_type.as_str().to_uppercase();
            let observer_ids_err = entity_type_index
                .load()
                .get(&(event.entity_type.clone(), event_type_str))
                .cloned();
            if let Some(observer_ids) = observer_ids_err {
                for observer_id in observer_ids {
                    let _ = sqlx::query(
                        // entity_id cast to uuid as in the success path above.
                        "INSERT INTO tb_observer_log
                         (fk_observer, event_id, entity_type, entity_id, event_type, status, error_message, attempt_number, max_attempts)
                         VALUES ($1, $2, $3, $4::uuid, $5, 'error', $6, 1, 3)",
                    )
                    .bind(observer_id)
                    .bind(event.id)
                    .bind(&event.entity_type)
                    .bind(event.entity_id.to_string())
                    .bind(event.event_type.as_str())
                    .bind(e.to_string())
                    .execute(pool)
                    .await;
                }
            }
        },
    }
}

/// Simple in-memory Dead Letter Queue for development.
///
/// Honors an optional size cap (`max_size`) mirroring the `fraiseql-observers`
/// library policy so `max_dlq_size` means the same thing in the binary and the
/// embedder: when the cap is reached, [`push`](InMemoryDlq::push) **drops the
/// newest** failed entry, emits a `warn!` with the same fields as the library,
/// and bumps an overflow counter — the durable, loud signal (a lone warn line
/// scrolls past). The length check and the insert happen under the same items
/// mutex, so there is no separate-atomic TOCTOU.
pub(crate) struct InMemoryDlq {
    items:          std::sync::Mutex<Vec<fraiseql_observers::DlqItem>>,
    /// Maximum retained entries; `None` = unbounded (back-compat default).
    max_size:       Option<usize>,
    /// Count of entries dropped because the DLQ was at capacity (drop-newest).
    overflow_count: std::sync::atomic::AtomicUsize,
}

impl InMemoryDlq {
    /// Create a DLQ with an optional retention cap (`None` = unbounded).
    const fn new_with_max(max_size: Option<usize>) -> Self {
        Self {
            items: std::sync::Mutex::new(Vec::new()),
            max_size,
            overflow_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Returns the number of items currently in the DLQ.
    pub(crate) fn count(&self) -> usize {
        self.items.lock().expect("items mutex poisoned").len()
    }

    /// Number of entries dropped because the DLQ was at capacity (drop-newest).
    ///
    /// The durable counterpart to the per-drop `warn!` — the loud overflow
    /// signal that survives log rotation.
    pub(crate) fn overflow_count(&self) -> usize {
        self.overflow_count.load(Ordering::Relaxed)
    }

    /// Returns all DLQ items as a cloned snapshot.
    pub(crate) fn list_all(&self) -> Vec<fraiseql_observers::DlqItem> {
        self.items.lock().expect("items mutex poisoned").clone()
    }

    /// Returns a single DLQ item by ID, if it exists.
    pub(crate) fn get(&self, id: uuid::Uuid) -> Option<fraiseql_observers::DlqItem> {
        self.items
            .lock()
            .expect("items mutex poisoned")
            .iter()
            .find(|item| item.id == id)
            .cloned()
    }

    /// Atomically remove and return a DLQ item by ID.
    ///
    /// The find and the remove happen under a single lock, so two concurrent
    /// retries on the same id cannot both observe it: exactly one caller gets
    /// `Some`, the rest get `None`. This makes DLQ retry **at-most-once per
    /// claim** (#344), closing the previous `get` → `process` → `remove` race
    /// that re-dispatched the action twice (turning at-least-once delivery into
    /// at-least-twice).
    pub(crate) fn try_claim(&self, id: uuid::Uuid) -> Option<fraiseql_observers::DlqItem> {
        let mut items = self.items.lock().expect("items mutex poisoned");
        let pos = items.iter().position(|item| item.id == id)?;
        Some(items.remove(pos))
    }

    /// Re-insert a previously-claimed item whose retry failed, **bypassing the
    /// size cap**.
    ///
    /// A claimed item was already counted against the cap before [`try_claim`]
    /// removed it; routing it back through [`push`](InMemoryDlq::push) would let
    /// drop-newest silently discard it if the DLQ refilled to `max_size` during
    /// the claim — re-introducing the exact silent loss the cap exists to
    /// prevent (#343/#344). The retry handlers use this to restore an item after
    /// a failed redispatch.
    pub(crate) fn reinsert(&self, item: fraiseql_observers::DlqItem) {
        self.items.lock().expect("items mutex poisoned").push(item);
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
        let mut items = self.items.lock().expect("items mutex poisoned");

        // Drop-newest when at capacity, mirroring the library policy
        // (`fraiseql-observers` executor): same `warn!` fields + an overflow
        // counter so the cap means the same thing in the binary and embedder.
        if let Some(max) = self.max_size {
            if items.len() >= max {
                warn!(
                    max_dlq_size = max,
                    action_type = action.action_type(),
                    event_id = %event.id,
                    "DLQ full; dropping failed action entry"
                );
                self.overflow_count.fetch_add(1, Ordering::Relaxed);
                // Drop the newest entry: it is intentionally not stored.
                return Ok(id);
            }
        }

        items.push(fraiseql_observers::DlqItem {
            id,
            event,
            action,
            error_message: error,
            attempts: 0,
        });
        Ok(id)
    }

    async fn get_pending(
        &self,
        limit: i64,
    ) -> fraiseql_observers::Result<Vec<fraiseql_observers::DlqItem>> {
        let items = self.items.lock().expect("items mutex poisoned");
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        // Reason: limit is a user-supplied i64 clamped to a small positive range; negative values
        // wrap to 0 safely
        let limit_usize = limit as usize;
        Ok(items.iter().take(limit_usize).cloned().collect())
    }

    async fn mark_success(&self, id: uuid::Uuid) -> fraiseql_observers::Result<()> {
        let mut items = self.items.lock().expect("items mutex poisoned");
        items.retain(|i| i.id != id);
        Ok(())
    }

    async fn mark_retry_failed(
        &self,
        id: uuid::Uuid,
        error: &str,
    ) -> fraiseql_observers::Result<()> {
        // Keep the item for a future retry / operator inspection: record the
        // failure on the existing fields instead of silently destroying it.
        // Items leave the DLQ only via `mark_success` or an explicit `remove`
        // (operator DELETE). This preserves the audit trail (#343) and is the
        // lifecycle the atomic claim model (#344) relies on.
        let mut items = self.items.lock().expect("items mutex poisoned");
        if let Some(item) = items.iter_mut().find(|i| i.id == id) {
            item.attempts = item.attempts.saturating_add(1);
            item.error_message = error.to_string();
        }
        Ok(())
    }
}
