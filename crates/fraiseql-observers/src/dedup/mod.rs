//! Event deduplication system for preventing duplicate processing.
//!
//! This module provides Redis-based time-window deduplication to prevent
//! the same event from being processed multiple times within a configurable window.
//!
//! # Problem Solved
//!
//! Without deduplication:
//! - Event fires twice due to trigger + manual retry
//! - Same event processed by multiple listeners
//! - Duplicate emails sent, duplicate charges created
//!
//! With deduplication:
//! - First occurrence processed
//! - Duplicate within time window silently skipped
//! - No duplicate side effects
//!
//! # Architecture
//!
//! ```text
//! Event arrives
//!     ↓
//! Hash = SHA256(entity_type + entity_id + event_type)
//!     ↓
//! Redis key: "dedup:{hash}" with TTL (default 5 min)
//!     ↓
//! If exists → Skip (already processed)
//! If missing → Process and set key with TTL
//! ```
//!
//! # Time Window
//!
//! - Default: 5 minutes (300 seconds)
//! - Configurable per deduplication store
//! - TTL automatically expires old dedup keys
//! - Zero manual cleanup needed

#[cfg(feature = "dedup")]
pub mod redis;

use crate::error::Result;
use serde::{Deserialize, Serialize};

/// Deduplication store abstraction.
///
/// Provides persistent storage for deduplication keys with TTL support.
/// Implementations determine whether an event should be processed or skipped.
///
/// # Trait Objects
///
/// This trait is object-safe and can be used as `Arc<dyn DeduplicationStore>`.
#[async_trait::async_trait]
pub trait DeduplicationStore: Send + Sync + Clone {
    /// Check if event was recently processed (is duplicate).
    ///
    /// Returns `true` if the event was seen recently (should skip processing).
    /// Returns `false` if the event is new (should process).
    ///
    /// # Arguments
    ///
    /// * `event_key` - Unique event identifier (typically `event_type` + `entity_id`)
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Redis connection fails
    /// - Key operations fail
    async fn is_duplicate(&self, event_key: &str) -> Result<bool>;

    /// Mark event as processed (for deduplication).
    ///
    /// Stores the event key with TTL so future occurrences within the window
    /// are considered duplicates. This is called after processing.
    ///
    /// # Arguments
    ///
    /// * `event_key` - Unique event identifier
    ///
    /// # Errors
    ///
    /// Returns error if Redis operation fails.
    async fn mark_processed(&self, event_key: &str) -> Result<()>;

    /// Get the deduplication time window in seconds.
    fn window_seconds(&self) -> u64;

    /// Set the deduplication time window in seconds.
    fn set_window_seconds(&mut self, seconds: u64);

    /// Remove a deduplication key (for manual intervention/reset).
    ///
    /// # Arguments
    ///
    /// * `event_key` - Unique event identifier
    ///
    /// # Errors
    ///
    /// Returns error if Redis operation fails.
    async fn remove(&self, event_key: &str) -> Result<()>;
}

/// Deduplication statistics for monitoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeduplicationStats {
    /// Total events checked
    pub total_checked: u64,
    /// Events marked as duplicates
    pub duplicates_skipped: u64,
    /// New events processed
    pub new_events: u64,
    /// Deduplication hit rate (0.0 - 1.0)
    pub hit_rate: f64,
}

impl DeduplicationStats {
    /// Create new deduplication statistics.
    #[must_use] 
    pub const fn new() -> Self {
        Self {
            total_checked: 0,
            duplicates_skipped: 0,
            new_events: 0,
            hit_rate: 0.0,
        }
    }

    /// Update statistics after processing.
    ///
    /// # Arguments
    ///
    /// * `is_duplicate` - Whether the event was a duplicate
    pub fn record(&mut self, is_duplicate: bool) {
        self.total_checked += 1;
        if is_duplicate {
            self.duplicates_skipped += 1;
        } else {
            self.new_events += 1;
        }

        if self.total_checked > 0 {
            self.hit_rate = self.duplicates_skipped as f64 / self.total_checked as f64;
        }
    }

    /// Reset statistics.
    pub fn reset(&mut self) {
        self.total_checked = 0;
        self.duplicates_skipped = 0;
        self.new_events = 0;
        self.hit_rate = 0.0;
    }
}

impl Default for DeduplicationStats {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dedup_stats_new() {
        let stats = DeduplicationStats::new();
        assert_eq!(stats.total_checked, 0);
        assert_eq!(stats.duplicates_skipped, 0);
        assert_eq!(stats.new_events, 0);
        assert_eq!(stats.hit_rate, 0.0);
    }

    #[test]
    fn test_dedup_stats_record_new_event() {
        let mut stats = DeduplicationStats::new();
        stats.record(false);

        assert_eq!(stats.total_checked, 1);
        assert_eq!(stats.duplicates_skipped, 0);
        assert_eq!(stats.new_events, 1);
        assert_eq!(stats.hit_rate, 0.0);
    }

    #[test]
    fn test_dedup_stats_record_duplicate() {
        let mut stats = DeduplicationStats::new();
        stats.record(false);
        stats.record(true);

        assert_eq!(stats.total_checked, 2);
        assert_eq!(stats.duplicates_skipped, 1);
        assert_eq!(stats.new_events, 1);
        assert!((stats.hit_rate - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_dedup_stats_hit_rate() {
        let mut stats = DeduplicationStats::new();
        for _ in 0..8 {
            stats.record(true); // duplicates
        }
        for _ in 0..2 {
            stats.record(false); // new events
        }

        assert_eq!(stats.total_checked, 10);
        assert_eq!(stats.duplicates_skipped, 8);
        assert_eq!(stats.new_events, 2);
        assert!((stats.hit_rate - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn test_dedup_stats_reset() {
        let mut stats = DeduplicationStats::new();
        stats.record(true);
        stats.record(false);

        stats.reset();

        assert_eq!(stats.total_checked, 0);
        assert_eq!(stats.duplicates_skipped, 0);
        assert_eq!(stats.new_events, 0);
        assert_eq!(stats.hit_rate, 0.0);
    }
}
