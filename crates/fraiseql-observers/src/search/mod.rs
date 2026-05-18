//! Event search and indexing system for compliance and debugging.
//!
//! This module provides full-text searchable event audit trail using Elasticsearch,
//! enabling compliance-ready logging and powerful debugging capabilities.
//!
//! # Problem Solved
//!
//! Without search:
//! - No way to find specific events in history
//! - Compliance audits require manual log review
//! - Debugging requires re-running events
//! - No full-text search across event data
//!
//! With Elasticsearch search:
//! - Full-text search across all events
//! - Compliance-ready audit trail
//! - Time-range queries for incident investigation
//! - Entity-scoped queries for user/product tracking
//!
//! # Architecture
//!
//! ```text
//! Event processed
//!     ↓
//! Create IndexedEvent with metadata
//!     ↓
//! Send to Elasticsearch
//!     ↓
//! Index with date-based sharding (daily indices)
//!     ↓
//! Enable full-text search queries
//! ```
//!
//! # Index Structure
//!
//! Each event is indexed with:
//! - `event_type`: Type of event (Created, Updated, Deleted)
//! - `entity_type`: Entity being observed (Order, User, Product)
//! - `entity_id`: UUID of the entity
//! - `tenant_id`: Multi-tenant isolation key
//! - timestamp: Event creation time
//! - `actions_executed`: Array of actions run
//! - `success_count`: Successful actions
//! - `failure_count`: Failed actions
//! - `search_text`: Full-text searchable content
//!
//! # Daily Index Pattern
//!
//! Indices are organized by date for efficient retention:
//! - `events-2026-01-22`: All events from January 22, 2026
//! - `events-2026-01-21`: All events from January 21, 2026
//! - Enables: purging old indices, time-range queries, retention policies

pub mod http;

use serde::{Deserialize, Serialize};

use crate::{error::Result, event::EntityEvent};

/// Indexed event representation for search systems.
///
/// Contains all event data plus search optimization fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedEvent {
    /// Event type (Created, Updated, Deleted)
    pub event_type:       String,
    /// Entity type (Order, User, Product, etc.)
    pub entity_type:      String,
    /// Entity unique identifier
    pub entity_id:        String,
    /// Tenant ID for multi-tenant isolation
    pub tenant_id:        String,
    /// When the event occurred (Unix timestamp)
    pub timestamp:        i64,
    /// All actions executed for this event
    pub actions_executed: Vec<String>,
    /// Count of successful actions
    pub success_count:    usize,
    /// Count of failed actions
    pub failure_count:    usize,
    /// Full event data as JSON string (for full-text search)
    pub event_data:       String,
    /// Optimized search text (action results, error messages)
    pub search_text:      String,
}

impl IndexedEvent {
    /// Create an indexed event from an entity event.
    ///
    /// # Arguments
    ///
    /// * `event` - The entity event to index
    /// * `tenant_id` - Tenant identifier for multi-tenant systems
    /// * `actions` - Array of action names executed
    /// * `success_count` - Number of successful actions
    /// * `failure_count` - Number of failed actions
    #[must_use]
    pub fn from_event(
        event: &EntityEvent,
        tenant_id: String,
        actions: Vec<String>,
        success_count: usize,
        failure_count: usize,
    ) -> Self {
        let event_data = serde_json::to_string(&event.data).unwrap_or_default();
        let search_text = format!(
            "{:?} {} {} {} {}",
            event.event_type,
            event.entity_type,
            event.entity_id,
            actions.join(" "),
            event_data
        );

        Self {
            event_type: format!("{:?}", event.event_type),
            entity_type: event.entity_type.clone(),
            entity_id: event.entity_id.to_string(),
            tenant_id,
            timestamp: event.timestamp.timestamp(),
            actions_executed: actions,
            success_count,
            failure_count,
            event_data,
            search_text,
        }
    }

    /// Get the index name for this event (date-based sharding).
    ///
    /// Returns format: `events-YYYY-MM-DD`
    #[must_use]
    pub fn index_name(&self) -> String {
        let datetime = chrono::DateTime::<chrono::Utc>::from_timestamp(self.timestamp, 0)
            .unwrap_or_else(chrono::Utc::now);
        format!("events-{}", datetime.format("%Y-%m-%d"))
    }
}

/// Search backend abstraction for event indexing and querying.
///
/// Provides persistent storage and full-text search for events.
/// Implementations handle the actual search engine (Elasticsearch, etc.).
///
/// # Trait Objects
///
/// This trait is object-safe and can be used as `Arc<dyn SearchBackend>`.
#[async_trait::async_trait]
pub trait SearchBackend: Send + Sync + Clone {
    /// Index a single event for searching.
    ///
    /// Stores the event in the search index with appropriate mappings.
    ///
    /// # Arguments
    ///
    /// * `event` - The indexed event to store
    ///
    /// # Errors
    ///
    /// Returns error if indexing fails
    async fn index_event(&self, event: &IndexedEvent) -> Result<()>;

    /// Index multiple events in a batch.
    ///
    /// More efficient than indexing one-by-one for bulk operations.
    ///
    /// # Arguments
    ///
    /// * `events` - Vector of indexed events to store
    ///
    /// # Errors
    ///
    /// Returns error if batch indexing fails
    async fn index_batch(&self, events: &[IndexedEvent]) -> Result<()>;

    /// Search for events by full-text query.
    ///
    /// # Arguments
    ///
    /// * `query` - Full-text search query
    /// * `tenant_id` - Filter by tenant
    /// * `limit` - Maximum results to return
    ///
    /// # Errors
    ///
    /// Returns error if search fails
    async fn search(&self, query: &str, tenant_id: &str, limit: usize)
    -> Result<Vec<IndexedEvent>>;

    /// Search by entity type and ID.
    ///
    /// # Arguments
    ///
    /// * `entity_type` - Type of entity to search for
    /// * `entity_id` - ID of entity to search for
    /// * `tenant_id` - Filter by tenant
    ///
    /// # Errors
    ///
    /// Returns error if search fails
    async fn search_entity(
        &self,
        entity_type: &str,
        entity_id: &str,
        tenant_id: &str,
    ) -> Result<Vec<IndexedEvent>>;

    /// Search by time range.
    ///
    /// # Arguments
    ///
    /// * `start_timestamp` - Start of time range (Unix timestamp)
    /// * `end_timestamp` - End of time range (Unix timestamp)
    /// * `tenant_id` - Filter by tenant
    /// * `limit` - Maximum results
    ///
    /// # Errors
    ///
    /// Returns error if search fails
    async fn search_time_range(
        &self,
        start_timestamp: i64,
        end_timestamp: i64,
        tenant_id: &str,
        limit: usize,
    ) -> Result<Vec<IndexedEvent>>;

    /// Delete events older than specified age (for retention policies).
    ///
    /// # Arguments
    ///
    /// * `days_old` - Delete events older than this many days
    ///
    /// # Errors
    ///
    /// Returns error if deletion fails
    async fn delete_old_events(&self, days_old: u32) -> Result<()>;
}

/// Search statistics for monitoring indexing performance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchStats {
    /// Total events indexed
    pub total_indexed:        u64,
    /// Successful indexing operations
    pub successful_indexes:   u64,
    /// Failed indexing operations
    pub failed_indexes:       u64,
    /// Average indexing latency in milliseconds
    pub avg_index_latency_ms: f64,
}

impl SearchStats {
    /// Create new search statistics.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            total_indexed:        0,
            successful_indexes:   0,
            failed_indexes:       0,
            avg_index_latency_ms: 0.0,
        }
    }

    /// Record an indexing operation.
    #[allow(clippy::cast_precision_loss)] // Reason: f64 precision is acceptable for metrics counters
    pub fn record(&mut self, success: bool, latency_ms: f64) {
        self.total_indexed += 1;

        if success {
            self.successful_indexes += 1;
            self.avg_index_latency_ms = self
                .avg_index_latency_ms
                .mul_add(self.successful_indexes as f64 - 1.0, latency_ms)
                / self.successful_indexes as f64;
        } else {
            self.failed_indexes += 1;
        }
    }

    /// Reset statistics.
    pub const fn reset(&mut self) {
        self.total_indexed = 0;
        self.successful_indexes = 0;
        self.failed_indexes = 0;
        self.avg_index_latency_ms = 0.0;
    }

    /// Get success rate as percentage.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // Reason: f64 precision is acceptable for metrics counters
    pub fn success_rate(&self) -> f64 {
        if self.total_indexed == 0 {
            0.0
        } else {
            (self.successful_indexes as f64 / self.total_indexed as f64) * 100.0
        }
    }
}

impl Default for SearchStats {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
