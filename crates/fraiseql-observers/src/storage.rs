//! Event storage interface for historical event querying.
//!
//! This module provides traits for querying historical observer events
//! from persistent storage (PostgreSQL, etc.).

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::{error::Result, event::EntityEvent};

/// Interface for querying historical events from storage.
///
/// Implementations should handle date range filtering, entity type filtering,
/// and result limiting efficiently (with appropriate indexes).
#[async_trait]
pub trait EventStorage: Send + Sync {
    /// Query historical events by entity type and optional date range.
    ///
    /// # Arguments
    ///
    /// * `entity_type` - Filter events for this entity type (e.g., "Order", "User")
    /// * `start_date` - Optional start of date range (inclusive)
    /// * `end_date` - Optional end of date range (inclusive)
    /// * `limit` - Maximum number of events to return (None = no limit, but implementations may cap
    ///   this)
    ///
    /// # Returns
    ///
    /// Events matching the filters, sorted by timestamp descending (most recent first)
    ///
    /// # Errors
    ///
    /// Returns error if database query fails or events cannot be deserialized.
    async fn query_events(
        &self,
        entity_type: &str,
        start_date: Option<DateTime<Utc>>,
        end_date: Option<DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<EntityEvent>>;

    /// Count events matching the filter criteria.
    ///
    /// Useful for pagination and understanding result sizes.
    async fn count_events(
        &self,
        entity_type: &str,
        start_date: Option<DateTime<Utc>>,
        end_date: Option<DateTime<Utc>>,
    ) -> Result<usize>;
}

#[cfg(feature = "postgres")]
pub mod postgres {
    //! PostgreSQL implementation of event storage.

    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use sqlx::PgPool;
    use uuid::Uuid;

    use super::EventStorage;
    use crate::{
        error::{ObserverError, Result},
        event::{EntityEvent, EventKind},
    };

    /// PostgreSQL-backed event storage using fraiseql_events table.
    ///
    /// Assumes the following table structure:
    ///
    /// ```sql
    /// CREATE TABLE fraiseql_events (
    ///     id UUID PRIMARY KEY,
    ///     event_type TEXT NOT NULL,
    ///     entity_type TEXT NOT NULL,
    ///     entity_id UUID NOT NULL,
    ///     timestamp TIMESTAMPTZ NOT NULL,
    ///     data JSONB NOT NULL,
    ///     user_id UUID,
    ///     tenant_id UUID,
    ///     created_at TIMESTAMPTZ DEFAULT NOW()
    /// );
    /// CREATE INDEX idx_fraiseql_events_entity_type_timestamp
    ///     ON fraiseql_events(entity_type, timestamp DESC);
    /// ```
    pub struct PostgresEventStorage {
        pool: PgPool,
    }

    impl PostgresEventStorage {
        /// Create a new PostgreSQL event storage backend.
        pub const fn new(pool: PgPool) -> Self {
            Self { pool }
        }
    }

    #[async_trait]
    impl EventStorage for PostgresEventStorage {
        async fn query_events(
            &self,
            entity_type: &str,
            start_date: Option<DateTime<Utc>>,
            end_date: Option<DateTime<Utc>>,
            limit: Option<usize>,
        ) -> Result<Vec<EntityEvent>> {
            // Build query with optional date filters
            let mut query_str = String::from(
                r"
                SELECT id, event_type, entity_type, entity_id, timestamp, data, user_id, tenant_id
                FROM fraiseql_events
                WHERE entity_type = $1
                ",
            );

            let mut param_index = 2;

            if start_date.is_some() {
                #[allow(clippy::format_push_string)]
                query_str.push_str(&format!(" AND timestamp >= ${param_index}"));
                param_index += 1;
            }

            if end_date.is_some() {
                #[allow(clippy::format_push_string)]
                query_str.push_str(&format!(" AND timestamp <= ${param_index}"));
            }

            // Default sort: most recent first
            query_str.push_str(" ORDER BY timestamp DESC");

            if let Some(lim) = limit {
                #[allow(clippy::format_push_string)]
                query_str.push_str(&format!(" LIMIT {lim}"));
            }

            // Execute query
            #[derive(sqlx::FromRow)]
            struct EventRow {
                id:          Uuid,
                event_type:  String,
                entity_type: String,
                entity_id:   Uuid,
                timestamp:   DateTime<Utc>,
                data:        serde_json::Value,
                user_id:     Option<String>,
                tenant_id:   Option<String>,
            }

            let mut query = sqlx::query_as::<_, EventRow>(&query_str);

            query = query.bind(entity_type);

            if let Some(start) = start_date {
                query = query.bind(start);
            }

            if let Some(end) = end_date {
                query = query.bind(end);
            }

            let rows =
                query.fetch_all(&self.pool).await.map_err(|e| ObserverError::StorageError {
                    reason: format!("Failed to query events: {e}"),
                })?;

            // Convert rows to EntityEvents
            let events = rows
                .into_iter()
                .map(|row| {
                    let event_type = match row.event_type.as_str() {
                        "INSERT" => EventKind::Created,
                        "UPDATE" => EventKind::Updated,
                        "DELETE" => EventKind::Deleted,
                        _ => EventKind::Custom,
                    };

                    EntityEvent {
                        id: row.id,
                        event_type,
                        entity_type: row.entity_type,
                        entity_id: row.entity_id,
                        data: row.data,
                        changes: None,
                        user_id: row.user_id,
                        tenant_id: row.tenant_id,
                        timestamp: row.timestamp,
                    }
                })
                .collect();

            Ok(events)
        }

        async fn count_events(
            &self,
            entity_type: &str,
            start_date: Option<DateTime<Utc>>,
            end_date: Option<DateTime<Utc>>,
        ) -> Result<usize> {
            let mut query_str = String::from(
                "SELECT COUNT(*) as count FROM fraiseql_events WHERE entity_type = $1",
            );

            let mut param_index = 2;

            if start_date.is_some() {
                #[allow(clippy::format_push_string)]
                query_str.push_str(&format!(" AND timestamp >= ${param_index}"));
                param_index += 1;
            }

            if end_date.is_some() {
                #[allow(clippy::format_push_string)]
                query_str.push_str(&format!(" AND timestamp <= ${param_index}"));
            }

            let mut query = sqlx::query_scalar::<_, i64>(&query_str).bind(entity_type);

            if let Some(start) = start_date {
                query = query.bind(start);
            }

            if let Some(end) = end_date {
                query = query.bind(end);
            }

            let count =
                query.fetch_one(&self.pool).await.map_err(|e| ObserverError::StorageError {
                    reason: format!("Failed to count events: {e}"),
                })?;

            Ok(count as usize)
        }
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    #[ignore = "Requires PostgreSQL connection"]
    async fn test_postgres_query_events() {
        // This test would require a test database setup
        // Skipping for now - integration tests will cover this
    }
}
