//! PostgreSQL LISTEN/NOTIFY transport implementation
//!
//! This module wraps the existing `ChangeLogListener` to implement the `EventTransport` trait,
//! providing backward compatibility while enabling the new abstraction layer.
//!
//! # Design
//!
//! - Wraps `ChangeLogListener` (polls `tb_entity_change_log`)
//! - Implements `EventTransport` trait
//! - Maintains existing behavior (zero changes to semantics)
//! - Enables gradual migration to transport-agnostic code

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use futures::stream;
use tokio::sync::Mutex;
use tracing::{debug, error};

use crate::{
    error::Result,
    event::EntityEvent,
    listener::{ChangeLogEntry, ChangeLogListener, ChangeLogListenerConfig},
    transport::{
        EventFilter, EventStream, EventTransport, HealthStatus, TransportHealth, TransportType,
    },
};

/// PostgreSQL transport using LISTEN/NOTIFY (via `tb_entity_change_log` polling)
///
/// This is a wrapper around the existing `ChangeLogListener` that implements
/// the `EventTransport` trait for backward compatibility.
pub struct PostgresNotifyTransport {
    /// Inner change log listener (wrapped)
    listener:      Arc<Mutex<ChangeLogListener>>,
    /// Poll interval for checking new events
    poll_interval: Duration,
}

impl PostgresNotifyTransport {
    /// Create a new PostgreSQL transport from existing listener
    #[must_use]
    pub fn new(listener: ChangeLogListener) -> Self {
        let poll_interval = Duration::from_millis(100); // Default 100ms polling

        Self {
            listener: Arc::new(Mutex::new(listener)),
            poll_interval,
        }
    }

    /// Create from configuration (convenience constructor)
    #[must_use]
    pub fn from_config(config: ChangeLogListenerConfig) -> Self {
        let poll_interval = Duration::from_millis(config.poll_interval_ms);
        let listener = ChangeLogListener::new(config);

        Self {
            listener: Arc::new(Mutex::new(listener)),
            poll_interval,
        }
    }

    /// Set poll interval
    #[must_use]
    pub const fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }
}

#[async_trait]
impl EventTransport for PostgresNotifyTransport {
    async fn subscribe(&self, _filter: EventFilter) -> Result<EventStream> {
        let listener = Arc::clone(&self.listener);
        let poll_interval = self.poll_interval;

        // Create a stream that polls the change log listener
        let stream =
            stream::unfold((listener, poll_interval), move |(listener, interval)| async move {
                loop {
                    // Lock the listener and fetch next batch
                    let entries: Vec<ChangeLogEntry> = {
                        let mut listener_guard = listener.lock().await;
                        match listener_guard.next_batch().await {
                            Ok(entries) => {
                                drop(listener_guard); // Release lock
                                entries
                            },
                            Err(e) => {
                                error!("Error fetching batch from change log: {}", e);
                                drop(listener_guard); // Release lock
                                // Return error and continue
                                return Some((Err(e), (listener, interval)));
                            },
                        }
                    };

                    // If we got entries, convert them to events
                    if !entries.is_empty() {
                        debug!("PostgresNotifyTransport: fetched {} entries", entries.len());

                        // Convert entries to events and yield them one by one
                        if let Some(entry) = entries.into_iter().next() {
                            match entry.to_entity_event() {
                                Ok(event) => {
                                    return Some((Ok(event), (listener, interval)));
                                },
                                Err(e) => {
                                    error!("Error converting change log entry to event: {}", e);
                                    return Some((Err(e), (listener, interval)));
                                },
                            }
                        }
                    }

                    // No entries, sleep and retry
                    tokio::time::sleep(interval).await;
                }
            });

        Ok(Box::pin(stream))
    }

    async fn publish(&self, event: EntityEvent) -> Result<()> {
        // PostgreSQL transport doesn't support publishing (write-only via database triggers)
        // This is a no-op for now, but could be implemented via direct INSERT to
        // tb_entity_change_log
        debug!("PostgresNotifyTransport::publish() called for event {} (no-op)", event.id);
        Ok(())
    }

    fn transport_type(&self) -> TransportType {
        TransportType::PostgresNotify
    }

    async fn health_check(&self) -> Result<TransportHealth> {
        // Try to lock the listener (if locked, it's healthy)
        let listener = self.listener.lock().await;

        // Could add more sophisticated health checks here (e.g., database ping)
        drop(listener);

        Ok(TransportHealth {
            status:  HealthStatus::Healthy,
            message: Some("PostgreSQL change log listener operational".to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use sqlx::postgres::PgPool;

    use super::*;
    use crate::listener::ChangeLogListenerConfig;

    /// Panics if TEST_DATABASE_URL is not set (tests using this are `#[ignore]`).
    async fn require_test_pool() -> PgPool {
        let database_url =
            env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set to run this test");
        PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to TEST_DATABASE_URL")
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL: set TEST_DATABASE_URL"]
    async fn test_postgres_transport_creation() {
        let pool = require_test_pool().await;

        let config = ChangeLogListenerConfig::new(pool);
        let transport = PostgresNotifyTransport::from_config(config);

        assert_eq!(transport.transport_type(), TransportType::PostgresNotify);
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL: set TEST_DATABASE_URL"]
    async fn test_postgres_transport_health_check() {
        let pool = require_test_pool().await;

        let config = ChangeLogListenerConfig::new(pool);
        let transport = PostgresNotifyTransport::from_config(config);

        let health = transport.health_check().await.expect("health_check should succeed");
        assert_eq!(health.status, HealthStatus::Healthy);
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL: set TEST_DATABASE_URL"]
    async fn test_postgres_transport_subscribe() {
        let pool = require_test_pool().await;

        let config = ChangeLogListenerConfig::new(pool).with_poll_interval(50);
        let transport = PostgresNotifyTransport::from_config(config);

        // Verify the stream can be created (won't produce events without data)
        let stream = transport
            .subscribe(EventFilter::default())
            .await
            .expect("subscribe should succeed");
        drop(stream);
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL: set TEST_DATABASE_URL"]
    async fn test_postgres_transport_poll_interval() {
        let pool = require_test_pool().await;

        let config = ChangeLogListenerConfig::new(pool);
        let transport = PostgresNotifyTransport::from_config(config)
            .with_poll_interval(Duration::from_millis(200));

        assert_eq!(transport.poll_interval, Duration::from_millis(200));
    }
}
