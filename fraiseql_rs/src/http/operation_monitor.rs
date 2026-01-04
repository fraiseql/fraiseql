//! GraphQL operation monitoring and slow operation detection (Phase 19, Commit 4.5)
//!
//! This module provides the core monitoring functionality for GraphQL operations, including:
//! - Operation metrics collection and storage
//! - Slow operation detection with configurable thresholds
//! - Statistics aggregation and percentile calculation
//! - Thread-safe metrics storage

use crate::http::operation_metrics::{
    GraphQLOperationType, OperationMetrics, OperationStatistics,
};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Configuration for GraphQL operation monitoring
#[derive(Debug, Clone)]
pub struct OperationMonitorConfig {
    /// Threshold for marking queries as slow (milliseconds)
    pub slow_query_threshold_ms: f64,

    /// Threshold for marking mutations as slow (milliseconds)
    pub slow_mutation_threshold_ms: f64,

    /// Threshold for marking subscriptions as slow (milliseconds)
    pub slow_subscription_threshold_ms: f64,

    /// Maximum number of recent operations to keep in memory
    pub max_recent_operations: usize,

    /// Sampling rate (0.0-1.0, where 1.0 = record all operations)
    pub sampling_rate: f64,

    /// Enable automatic slow operation detection and alerting
    pub enable_slow_operation_alerts: bool,
}

impl OperationMonitorConfig {
    /// Create a new monitoring configuration with default values
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set slow query threshold
    #[must_use]
    pub fn with_query_threshold(mut self, threshold_ms: f64) -> Self {
        self.slow_query_threshold_ms = threshold_ms;
        self
    }

    /// Set slow mutation threshold
    #[must_use]
    pub fn with_mutation_threshold(mut self, threshold_ms: f64) -> Self {
        self.slow_mutation_threshold_ms = threshold_ms;
        self
    }

    /// Set slow subscription threshold
    #[must_use]
    pub fn with_subscription_threshold(mut self, threshold_ms: f64) -> Self {
        self.slow_subscription_threshold_ms = threshold_ms;
        self
    }

    /// Set maximum recent operations capacity
    #[must_use]
    pub fn with_max_recent_operations(mut self, max: usize) -> Self {
        self.max_recent_operations = max;
        self
    }

    /// Set sampling rate
    #[must_use]
    pub fn with_sampling_rate(mut self, rate: f64) -> Self {
        self.sampling_rate = rate.clamp(0.0, 1.0);
        self
    }
}

impl Default for OperationMonitorConfig {
    fn default() -> Self {
        Self {
            slow_query_threshold_ms: 100.0,
            slow_mutation_threshold_ms: 500.0,
            slow_subscription_threshold_ms: 1000.0,
            max_recent_operations: 10_000,
            sampling_rate: 1.0,
            enable_slow_operation_alerts: true,
        }
    }
}

/// Thread-safe storage for operation metrics
#[derive(Debug)]
struct MetricsStorage {
    /// Recent operations (FIFO queue)
    recent_operations: VecDeque<OperationMetrics>,

    /// Slow operations (kept separately for quick access)
    slow_operations: VecDeque<OperationMetrics>,

    /// Total operations ever recorded
    total_recorded: u64,

    /// Total slow operations ever recorded
    total_slow: u64,
}

impl MetricsStorage {
    /// Create a new empty metrics storage
    fn new() -> Self {
        Self {
            recent_operations: VecDeque::new(),
            slow_operations: VecDeque::new(),
            total_recorded: 0,
            total_slow: 0,
        }
    }

    /// Add an operation to storage
    fn add_operation(&mut self, metrics: OperationMetrics, max_capacity: usize) {
        self.total_recorded += 1;

        if metrics.is_slow {
            self.total_slow += 1;
            // Keep slow operations in separate queue
            if self.slow_operations.len() >= max_capacity / 2 {
                self.slow_operations.pop_front();
            }
            self.slow_operations.push_back(metrics.clone());
        }

        // Keep recent operations in main queue
        if self.recent_operations.len() >= max_capacity {
            self.recent_operations.pop_front();
        }
        self.recent_operations.push_back(metrics);
    }

    /// Get all recent operations
    fn get_recent_operations(&self) -> Vec<OperationMetrics> {
        self.recent_operations.iter().cloned().collect()
    }

    /// Get slow operations
    fn get_slow_operations(&self) -> Vec<OperationMetrics> {
        self.slow_operations.iter().cloned().collect()
    }

    /// Get slow operations by type
    fn get_slow_operations_by_type(&self, op_type: GraphQLOperationType) -> Vec<OperationMetrics> {
        self.slow_operations
            .iter()
            .filter(|m| m.operation_type == op_type)
            .cloned()
            .collect()
    }

    /// Clear all stored operations
    fn clear(&mut self) {
        self.recent_operations.clear();
        self.slow_operations.clear();
    }
}

/// GraphQL operation monitor
///
/// Tracks operation metrics, detects slow operations, and provides statistics.
/// Thread-safe via Arc<Mutex<>>.
#[derive(Debug)]
pub struct GraphQLOperationMonitor {
    config: OperationMonitorConfig,
    storage: Arc<Mutex<MetricsStorage>>,
}

impl GraphQLOperationMonitor {
    /// Create a new operation monitor with default configuration
    #[must_use]
    pub fn new(config: OperationMonitorConfig) -> Self {
        Self {
            config,
            storage: Arc::new(Mutex::new(MetricsStorage::new())),
        }
    }

    /// Record an operation's metrics
    ///
    /// Automatically detects if operation is slow based on configuration
    /// and type. Applies sampling rate if configured.
    ///
    /// # Arguments
    ///
    /// * `mut metrics` - Operation metrics to record
    ///
    /// # Returns
    ///
    /// `Ok(())` if recorded, `Err` if sampling skipped
    pub fn record(&self, mut metrics: OperationMetrics) -> Result<(), &'static str> {
        // Apply sampling
        if self.config.sampling_rate < 1.0 {
            let should_sample =
                ((metrics.operation_id.len() % 100) as f64) < (self.config.sampling_rate * 100.0);
            if !should_sample {
                return Err("skipped by sampling");
            }
        }

        // Determine slow threshold based on operation type
        let threshold = match metrics.operation_type {
            GraphQLOperationType::Query => self.config.slow_query_threshold_ms,
            GraphQLOperationType::Mutation => self.config.slow_mutation_threshold_ms,
            GraphQLOperationType::Subscription => self.config.slow_subscription_threshold_ms,
            GraphQLOperationType::Unknown => self.config.slow_query_threshold_ms,
        };

        // Set slow threshold and detect if slow
        metrics.set_slow_threshold(threshold);

        // Store the metrics
        if let Ok(mut storage) = self.storage.lock() {
            storage.add_operation(metrics.clone(), self.config.max_recent_operations);
        }

        Ok(())
    }

    /// Get recent operations
    ///
    /// Returns the most recent operations recorded (up to max_recent_operations).
    #[must_use]
    pub fn get_recent_operations(&self, limit: Option<usize>) -> Vec<OperationMetrics> {
        if let Ok(storage) = self.storage.lock() {
            let recent = storage.get_recent_operations();
            if let Some(lim) = limit {
                recent.into_iter().rev().take(lim).collect()
            } else {
                recent
            }
        } else {
            Vec::new()
        }
    }

    /// Get slow operations
    ///
    /// Returns operations that exceeded the slow threshold.
    ///
    /// # Arguments
    ///
    /// * `operation_type` - Filter by operation type (None = all types)
    /// * `limit` - Maximum number of slow operations to return
    #[must_use]
    pub fn get_slow_operations(
        &self,
        operation_type: Option<GraphQLOperationType>,
        limit: Option<usize>,
    ) -> Vec<OperationMetrics> {
        if let Ok(storage) = self.storage.lock() {
            let slow_ops = operation_type
                .map(|op_type| storage.get_slow_operations_by_type(op_type))
                .unwrap_or_else(|| storage.get_slow_operations());

            if let Some(lim) = limit {
                slow_ops.into_iter().rev().take(lim).collect()
            } else {
                slow_ops
            }
        } else {
            Vec::new()
        }
    }

    /// Get operation statistics
    ///
    /// Calculates aggregate statistics from all recent operations.
    #[must_use]
    pub fn get_statistics(&self) -> OperationStatistics {
        if let Ok(storage) = self.storage.lock() {
            let recent = storage.get_recent_operations();
            OperationStatistics::from_metrics(&recent)
        } else {
            OperationStatistics::new()
        }
    }

    /// Get statistics for a specific operation type
    #[must_use]
    pub fn get_statistics_by_type(&self, op_type: GraphQLOperationType) -> OperationStatistics {
        if let Ok(storage) = self.storage.lock() {
            let recent: Vec<OperationMetrics> = storage
                .get_recent_operations()
                .into_iter()
                .filter(|m| m.operation_type == op_type)
                .collect();
            OperationStatistics::from_metrics(&recent)
        } else {
            OperationStatistics::new()
        }
    }

    /// Count of slow operations of a specific type
    #[must_use]
    pub fn count_slow_by_type(&self, op_type: GraphQLOperationType) -> usize {
        self.get_slow_operations(Some(op_type), None).len()
    }

    /// Total count of all operations recorded
    #[must_use]
    pub fn total_operations_recorded(&self) -> u64 {
        if let Ok(storage) = self.storage.lock() {
            storage.total_recorded
        } else {
            0
        }
    }

    /// Total count of all slow operations recorded
    #[must_use]
    pub fn total_slow_operations_recorded(&self) -> u64 {
        if let Ok(storage) = self.storage.lock() {
            storage.total_slow
        } else {
            0
        }
    }

    /// Clear all stored metrics
    pub fn clear(&self) {
        if let Ok(mut storage) = self.storage.lock() {
            storage.clear();
        }
    }

    /// Get the slow operation threshold for a given operation type
    #[must_use]
    pub fn get_slow_threshold(&self, op_type: GraphQLOperationType) -> f64 {
        match op_type {
            GraphQLOperationType::Query | GraphQLOperationType::Unknown => {
                self.config.slow_query_threshold_ms
            }
            GraphQLOperationType::Mutation => self.config.slow_mutation_threshold_ms,
            GraphQLOperationType::Subscription => self.config.slow_subscription_threshold_ms,
        }
    }

    /// Get the current configuration
    #[must_use]
    pub fn config(&self) -> &OperationMonitorConfig {
        &self.config
    }
}

impl Clone for GraphQLOperationMonitor {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            storage: Arc::clone(&self.storage),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monitor_creation() {
        let config = OperationMonitorConfig::new();
        let monitor = GraphQLOperationMonitor::new(config);

        assert_eq!(monitor.total_operations_recorded(), 0);
    }

    #[test]
    fn test_record_operation() {
        let config = OperationMonitorConfig::new();
        let monitor = GraphQLOperationMonitor::new(config);

        let mut metrics = OperationMetrics::new(
            "op_1".to_string(),
            Some("GetUser".to_string()),
            GraphQLOperationType::Query,
        );
        metrics.duration_ms = 50.0; // Below threshold
        metrics.finish();

        assert!(monitor.record(metrics).is_ok());
        assert_eq!(monitor.total_operations_recorded(), 1);
    }

    #[test]
    fn test_slow_operation_detection() {
        let config = OperationMonitorConfig::new()
            .with_query_threshold(100.0)
            .with_mutation_threshold(500.0);
        let monitor = GraphQLOperationMonitor::new(config);

        // Record a slow query
        let mut slow_query = OperationMetrics::new(
            "op_1".to_string(),
            Some("SlowQuery".to_string()),
            GraphQLOperationType::Query,
        );
        slow_query.duration_ms = 150.0; // Exceeds 100ms threshold
        slow_query.finish();

        assert!(monitor.record(slow_query).is_ok());

        let slow_ops = monitor.get_slow_operations(None, None);
        assert_eq!(slow_ops.len(), 1);
        assert!(slow_ops[0].is_slow);
    }

    #[test]
    fn test_get_slow_operations_by_type() {
        let config = OperationMonitorConfig::new()
            .with_query_threshold(100.0)
            .with_mutation_threshold(500.0);
        let monitor = GraphQLOperationMonitor::new(config);

        // Record slow query
        let mut slow_query = OperationMetrics::new(
            "op_1".to_string(),
            None,
            GraphQLOperationType::Query,
        );
        slow_query.duration_ms = 150.0;
        slow_query.finish();
        monitor.record(slow_query).ok();

        // Record slow mutation
        let mut slow_mutation = OperationMetrics::new(
            "op_2".to_string(),
            None,
            GraphQLOperationType::Mutation,
        );
        slow_mutation.duration_ms = 600.0;
        slow_mutation.finish();
        monitor.record(slow_mutation).ok();

        let slow_queries = monitor.get_slow_operations(Some(GraphQLOperationType::Query), None);
        let slow_mutations =
            monitor.get_slow_operations(Some(GraphQLOperationType::Mutation), None);

        assert_eq!(slow_queries.len(), 1);
        assert_eq!(slow_mutations.len(), 1);
    }

    #[test]
    fn test_recent_operations_limit() {
        let config = OperationMonitorConfig::new().with_max_recent_operations(5);
        let monitor = GraphQLOperationMonitor::new(config);

        // Record 10 operations
        for i in 0..10 {
            let mut metrics =
                OperationMetrics::new(format!("op_{}", i), None, GraphQLOperationType::Query);
            metrics.duration_ms = 10.0;
            metrics.finish();
            monitor.record(metrics).ok();
        }

        let recent = monitor.get_recent_operations(None);
        assert!(recent.len() <= 5); // Should be limited to max
    }

    #[test]
    fn test_statistics_calculation() {
        let config = OperationMonitorConfig::new();
        let monitor = GraphQLOperationMonitor::new(config);

        // Record operations
        for i in 0..5 {
            let mut metrics =
                OperationMetrics::new(format!("op_{}", i), None, GraphQLOperationType::Query);
            metrics.duration_ms = (i as f64 + 1.0) * 10.0; // 10, 20, 30, 40, 50
            metrics.set_response_size(1024);
            metrics.set_field_count(5);
            metrics.finish();
            monitor.record(metrics).ok();
        }

        let stats = monitor.get_statistics();
        assert_eq!(stats.total_operations, 5);
        assert!(stats.avg_duration_ms > 0.0);
        assert_eq!(stats.total_response_bytes, 5120); // 5 * 1024
    }

    #[test]
    fn test_sampling_rate() {
        let config = OperationMonitorConfig::new().with_sampling_rate(0.5); // 50% sampling
        let monitor = GraphQLOperationMonitor::new(config);

        // Try to record many operations
        let mut recorded = 0;
        for i in 0..100 {
            let mut metrics =
                OperationMetrics::new(format!("op_{}", i), None, GraphQLOperationType::Query);
            metrics.duration_ms = 10.0;
            metrics.finish();
            if monitor.record(metrics).is_ok() {
                recorded += 1;
            }
        }

        // Should have recorded approximately 50% (with some variance)
        assert!(recorded > 30 && recorded < 70);
    }

    #[test]
    fn test_config_builder() {
        let config = OperationMonitorConfig::new()
            .with_query_threshold(200.0)
            .with_mutation_threshold(1000.0)
            .with_subscription_threshold(2000.0)
            .with_max_recent_operations(5000)
            .with_sampling_rate(0.8);

        assert_eq!(config.slow_query_threshold_ms, 200.0);
        assert_eq!(config.slow_mutation_threshold_ms, 1000.0);
        assert_eq!(config.slow_subscription_threshold_ms, 2000.0);
        assert_eq!(config.max_recent_operations, 5000);
        assert_eq!(config.sampling_rate, 0.8);
    }

    #[test]
    fn test_statistics_by_type() {
        let config = OperationMonitorConfig::new();
        let monitor = GraphQLOperationMonitor::new(config);

        // Record different operation types
        for i in 0..3 {
            let mut metrics =
                OperationMetrics::new(format!("query_{}", i), None, GraphQLOperationType::Query);
            metrics.duration_ms = 20.0;
            metrics.finish();
            monitor.record(metrics).ok();
        }

        for i in 0..2 {
            let mut metrics = OperationMetrics::new(
                format!("mutation_{}", i),
                None,
                GraphQLOperationType::Mutation,
            );
            metrics.duration_ms = 50.0;
            metrics.finish();
            monitor.record(metrics).ok();
        }

        let query_stats = monitor.get_statistics_by_type(GraphQLOperationType::Query);
        let mutation_stats = monitor.get_statistics_by_type(GraphQLOperationType::Mutation);

        assert_eq!(query_stats.total_operations, 3);
        assert_eq!(mutation_stats.total_operations, 2);
    }

    #[test]
    fn test_clear_operations() {
        let config = OperationMonitorConfig::new();
        let monitor = GraphQLOperationMonitor::new(config);

        let mut metrics = OperationMetrics::new("op_1".to_string(), None, GraphQLOperationType::Query);
        metrics.duration_ms = 10.0;
        metrics.finish();
        monitor.record(metrics).ok();

        assert_eq!(monitor.total_operations_recorded(), 1);

        monitor.clear();

        let recent = monitor.get_recent_operations(None);
        assert_eq!(recent.len(), 0);
    }

    #[test]
    fn test_get_slow_threshold() {
        let config = OperationMonitorConfig::new()
            .with_query_threshold(100.0)
            .with_mutation_threshold(500.0);
        let monitor = GraphQLOperationMonitor::new(config);

        assert_eq!(
            monitor.get_slow_threshold(GraphQLOperationType::Query),
            100.0
        );
        assert_eq!(
            monitor.get_slow_threshold(GraphQLOperationType::Mutation),
            500.0
        );
    }

    #[test]
    fn test_monitor_clone() {
        let config = OperationMonitorConfig::new();
        let monitor1 = GraphQLOperationMonitor::new(config);

        let mut metrics = OperationMetrics::new("op_1".to_string(), None, GraphQLOperationType::Query);
        metrics.duration_ms = 10.0;
        metrics.finish();
        monitor1.record(metrics).ok();

        // Clone the monitor
        let monitor2 = monitor1.clone();

        // Both should share the same storage
        assert_eq!(monitor1.total_operations_recorded(), 1);
        assert_eq!(monitor2.total_operations_recorded(), 1);

        // Record through clone
        let mut metrics2 = OperationMetrics::new("op_2".to_string(), None, GraphQLOperationType::Query);
        metrics2.duration_ms = 20.0;
        metrics2.finish();
        monitor2.record(metrics2).ok();

        // Original should see the new operation too
        assert_eq!(monitor1.total_operations_recorded(), 2);
    }
}
