//! Validation metrics collection for observability.
//!
//! Tracks validation performance, errors, and patterns to enable
//! introspection, learning, and performance analysis.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use serde::{Deserialize, Serialize};

/// Validation metrics collected during request processing.
#[derive(Debug, Clone)]
pub struct ValidationMetricsCollector {
    /// Total validation checks performed
    pub validation_checks_total: Arc<AtomicU64>,

    /// Total validation failures
    pub validation_errors_total: Arc<AtomicU64>,

    /// Async validator executions
    pub async_validation_total: Arc<AtomicU64>,

    /// Async validator failures
    pub async_validation_errors: Arc<AtomicU64>,

    /// Total async validator duration (microseconds)
    pub async_validation_duration_us: Arc<AtomicU64>,

    /// Total validation duration (microseconds)
    pub validation_duration_us: Arc<AtomicU64>,

    /// Per-field validation error counts
    pub field_validation_errors: Arc<parking_lot::RwLock<std::collections::HashMap<String, u64>>>,

    /// Per-rule-type validation error counts
    pub rule_type_errors: Arc<parking_lot::RwLock<std::collections::HashMap<String, u64>>>,
}

/// A single validation metric entry for structured logging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationMetricEntry {
    /// Field name being validated
    pub field: String,

    /// Validation rule type (required, pattern, range, etc.)
    pub rule_type: String,

    /// Whether validation passed
    pub valid: bool,

    /// Duration in microseconds
    pub duration_us: u64,

    /// Validator type (async, checksum, pattern, etc.)
    pub validator_type: String,

    /// Failure reason if invalid
    pub failure_reason: Option<String>,
}

impl ValidationMetricsCollector {
    /// Create a new validation metrics collector.
    #[must_use]
    pub fn new() -> Self {
        Self {
            validation_checks_total: Arc::new(AtomicU64::new(0)),
            validation_errors_total: Arc::new(AtomicU64::new(0)),
            async_validation_total: Arc::new(AtomicU64::new(0)),
            async_validation_errors: Arc::new(AtomicU64::new(0)),
            async_validation_duration_us: Arc::new(AtomicU64::new(0)),
            validation_duration_us: Arc::new(AtomicU64::new(0)),
            field_validation_errors: Arc::new(parking_lot::RwLock::new(
                std::collections::HashMap::new(),
            )),
            rule_type_errors: Arc::new(parking_lot::RwLock::new(
                std::collections::HashMap::new(),
            )),
        }
    }

    /// Record a validation check.
    pub fn record_validation(&self, field: &str, rule_type: &str, valid: bool, duration_us: u64) {
        self.validation_checks_total.fetch_add(1, Ordering::Relaxed);
        self.validation_duration_us.fetch_add(duration_us, Ordering::Relaxed);

        if !valid {
            self.validation_errors_total.fetch_add(1, Ordering::Relaxed);

            // Track per-field errors
            {
                let mut errors = self.field_validation_errors.write();
                *errors.entry(field.to_string()).or_insert(0) += 1;
            }

            // Track per-rule-type errors
            {
                let mut errors = self.rule_type_errors.write();
                *errors.entry(rule_type.to_string()).or_insert(0) += 1;
            }
        }
    }

    /// Record an async validation execution.
    pub fn record_async_validation(
        &self,
        field: &str,
        rule_type: &str,
        valid: bool,
        duration_us: u64,
    ) {
        self.async_validation_total.fetch_add(1, Ordering::Relaxed);
        self.async_validation_duration_us.fetch_add(duration_us, Ordering::Relaxed);

        if !valid {
            self.async_validation_errors.fetch_add(1, Ordering::Relaxed);

            // Track per-field errors
            {
                let mut errors = self.field_validation_errors.write();
                *errors.entry(field.to_string()).or_insert(0) += 1;
            }

            // Track per-rule-type errors
            {
                let mut errors = self.rule_type_errors.write();
                *errors.entry(rule_type.to_string()).or_insert(0) += 1;
            }
        }
    }

    /// Get current per-field error counts.
    pub fn get_field_errors(&self) -> std::collections::HashMap<String, u64> {
        self.field_validation_errors.read().clone()
    }

    /// Get current per-rule-type error counts.
    pub fn get_rule_type_errors(&self) -> std::collections::HashMap<String, u64> {
        self.rule_type_errors.read().clone()
    }

    /// Clear all metrics.
    pub fn reset(&self) {
        self.validation_checks_total.store(0, Ordering::Relaxed);
        self.validation_errors_total.store(0, Ordering::Relaxed);
        self.async_validation_total.store(0, Ordering::Relaxed);
        self.async_validation_errors.store(0, Ordering::Relaxed);
        self.async_validation_duration_us.store(0, Ordering::Relaxed);
        self.validation_duration_us.store(0, Ordering::Relaxed);
        self.field_validation_errors.write().clear();
        self.rule_type_errors.write().clear();
    }

    /// Get a snapshot of the current metrics as Prometheus format.
    pub fn snapshot_prometheus(&self) -> PrometheusValidationMetrics {
        PrometheusValidationMetrics::from(self)
    }
}

impl Default for ValidationMetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Prometheus metrics format for validation metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrometheusValidationMetrics {
    /// Total validation checks performed
    pub validation_checks_total: u64,

    /// Total validation failures
    pub validation_errors_total: u64,

    /// Async validator executions
    pub async_validation_total: u64,

    /// Async validator failures
    pub async_validation_errors: u64,

    /// Average validation duration in microseconds
    pub validation_avg_duration_us: f64,

    /// Average async validation duration in microseconds
    pub async_validation_avg_duration_us: f64,
}

impl PrometheusValidationMetrics {
    /// Generate Prometheus text format output.
    #[must_use]
    pub fn to_prometheus_format(&self) -> String {
        format!(
            r"# HELP fraiseql_validation_checks_total Total validation checks performed
# TYPE fraiseql_validation_checks_total counter
fraiseql_validation_checks_total {}

# HELP fraiseql_validation_errors_total Total validation errors
# TYPE fraiseql_validation_errors_total counter
fraiseql_validation_errors_total {}

# HELP fraiseql_async_validation_total Total async validation checks
# TYPE fraiseql_async_validation_total counter
fraiseql_async_validation_total {}

# HELP fraiseql_async_validation_errors_total Total async validation errors
# TYPE fraiseql_async_validation_errors_total counter
fraiseql_async_validation_errors_total {}

# HELP fraiseql_validation_avg_duration_us Average validation duration in microseconds
# TYPE fraiseql_validation_avg_duration_us gauge
fraiseql_validation_avg_duration_us {:.2}

# HELP fraiseql_async_validation_avg_duration_us Average async validation duration in microseconds
# TYPE fraiseql_async_validation_avg_duration_us gauge
fraiseql_async_validation_avg_duration_us {:.2}
",
            self.validation_checks_total,
            self.validation_errors_total,
            self.async_validation_total,
            self.async_validation_errors,
            self.validation_avg_duration_us,
            self.async_validation_avg_duration_us,
        )
    }
}

impl From<&ValidationMetricsCollector> for PrometheusValidationMetrics {
    fn from(collector: &ValidationMetricsCollector) -> Self {
        let validation_checks = collector.validation_checks_total.load(Ordering::Relaxed);
        let validation_duration = collector.validation_duration_us.load(Ordering::Relaxed);
        let async_checks = collector.async_validation_total.load(Ordering::Relaxed);
        let async_duration = collector.async_validation_duration_us.load(Ordering::Relaxed);

        Self {
            validation_checks_total: validation_checks,
            validation_errors_total: collector.validation_errors_total.load(Ordering::Relaxed),
            async_validation_total: async_checks,
            async_validation_errors: collector.async_validation_errors.load(Ordering::Relaxed),
            validation_avg_duration_us: if validation_checks > 0 {
                validation_duration as f64 / validation_checks as f64
            } else {
                0.0
            },
            async_validation_avg_duration_us: if async_checks > 0 {
                async_duration as f64 / async_checks as f64
            } else {
                0.0
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_metrics_creation() {
        let collector = ValidationMetricsCollector::new();
        assert_eq!(collector.validation_checks_total.load(Ordering::Relaxed), 0);
        assert_eq!(collector.validation_errors_total.load(Ordering::Relaxed), 0);
        assert_eq!(collector.async_validation_total.load(Ordering::Relaxed), 0);
        assert_eq!(collector.async_validation_errors.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_record_validation_success() {
        let collector = ValidationMetricsCollector::new();
        collector.record_validation("email", "pattern", true, 100);

        assert_eq!(collector.validation_checks_total.load(Ordering::Relaxed), 1);
        assert_eq!(collector.validation_errors_total.load(Ordering::Relaxed), 0);
        assert_eq!(collector.validation_duration_us.load(Ordering::Relaxed), 100);
    }

    #[test]
    fn test_record_validation_failure() {
        let collector = ValidationMetricsCollector::new();
        collector.record_validation("email", "pattern", false, 150);

        assert_eq!(collector.validation_checks_total.load(Ordering::Relaxed), 1);
        assert_eq!(collector.validation_errors_total.load(Ordering::Relaxed), 1);
        assert_eq!(collector.validation_duration_us.load(Ordering::Relaxed), 150);
    }

    #[test]
    fn test_per_field_error_tracking() {
        let collector = ValidationMetricsCollector::new();
        collector.record_validation("email", "pattern", false, 100);
        collector.record_validation("email", "length", false, 100);
        collector.record_validation("name", "required", false, 50);

        let field_errors = collector.get_field_errors();
        assert_eq!(field_errors.get("email"), Some(&2));
        assert_eq!(field_errors.get("name"), Some(&1));
    }

    #[test]
    fn test_per_rule_type_error_tracking() {
        let collector = ValidationMetricsCollector::new();
        collector.record_validation("email", "pattern", false, 100);
        collector.record_validation("age", "pattern", false, 100);
        collector.record_validation("name", "required", false, 50);

        let rule_errors = collector.get_rule_type_errors();
        assert_eq!(rule_errors.get("pattern"), Some(&2));
        assert_eq!(rule_errors.get("required"), Some(&1));
    }

    #[test]
    fn test_record_async_validation_success() {
        let collector = ValidationMetricsCollector::new();
        collector.record_async_validation("email", "async", true, 500);

        assert_eq!(collector.async_validation_total.load(Ordering::Relaxed), 1);
        assert_eq!(collector.async_validation_errors.load(Ordering::Relaxed), 0);
        assert_eq!(collector.async_validation_duration_us.load(Ordering::Relaxed), 500);
    }

    #[test]
    fn test_record_async_validation_failure() {
        let collector = ValidationMetricsCollector::new();
        collector.record_async_validation("email", "async", false, 600);

        assert_eq!(collector.async_validation_total.load(Ordering::Relaxed), 1);
        assert_eq!(collector.async_validation_errors.load(Ordering::Relaxed), 1);
        assert_eq!(collector.async_validation_duration_us.load(Ordering::Relaxed), 600);
    }

    #[test]
    fn test_async_validation_in_field_errors() {
        let collector = ValidationMetricsCollector::new();
        collector.record_async_validation("email", "async_email", false, 500);
        collector.record_async_validation("email", "async_domain", false, 500);

        let field_errors = collector.get_field_errors();
        assert_eq!(field_errors.get("email"), Some(&2));
    }

    #[test]
    fn test_multiple_fields_and_rules() {
        let collector = ValidationMetricsCollector::new();

        // Multiple fields
        collector.record_validation("email", "pattern", false, 100);
        collector.record_validation("phone", "pattern", false, 150);
        collector.record_validation("age", "range", false, 50);

        // Multiple rule types
        collector.record_validation("password", "length", false, 75);
        collector.record_validation("password", "pattern", false, 75);
        collector.record_validation("country", "enum", false, 25);

        let field_errors = collector.get_field_errors();
        assert_eq!(field_errors.len(), 5);

        let rule_errors = collector.get_rule_type_errors();
        assert_eq!(rule_errors.len(), 4);
        assert_eq!(rule_errors.get("pattern"), Some(&3));
    }

    #[test]
    fn test_validation_duration_accumulation() {
        let collector = ValidationMetricsCollector::new();
        collector.record_validation("email", "pattern", true, 100);
        collector.record_validation("email", "pattern", false, 150);
        collector.record_validation("name", "required", true, 50);

        let total_duration = collector.validation_duration_us.load(Ordering::Relaxed);
        assert_eq!(total_duration, 300); // 100 + 150 + 50
    }

    #[test]
    fn test_async_validation_duration_accumulation() {
        let collector = ValidationMetricsCollector::new();
        collector.record_async_validation("email", "async", true, 500);
        collector.record_async_validation("email", "async", false, 600);

        let total_duration = collector.async_validation_duration_us.load(Ordering::Relaxed);
        assert_eq!(total_duration, 1100); // 500 + 600
    }

    #[test]
    fn test_reset_clears_all_metrics() {
        let collector = ValidationMetricsCollector::new();
        collector.record_validation("email", "pattern", false, 100);
        collector.record_async_validation("email", "async", false, 500);

        assert!(collector.validation_errors_total.load(Ordering::Relaxed) > 0);
        assert!(collector.async_validation_errors.load(Ordering::Relaxed) > 0);

        collector.reset();

        assert_eq!(collector.validation_checks_total.load(Ordering::Relaxed), 0);
        assert_eq!(collector.validation_errors_total.load(Ordering::Relaxed), 0);
        assert_eq!(collector.async_validation_total.load(Ordering::Relaxed), 0);
        assert_eq!(collector.async_validation_errors.load(Ordering::Relaxed), 0);
        assert_eq!(collector.get_field_errors().len(), 0);
        assert_eq!(collector.get_rule_type_errors().len(), 0);
    }

    #[test]
    fn test_thread_safety_field_errors() {
        let collector = Arc::new(ValidationMetricsCollector::new());
        let mut handles = vec![];

        for i in 0..10 {
            let collector_clone = collector.clone();
            let handle = std::thread::spawn(move || {
                let field = format!("field_{}", i % 5);
                collector_clone.record_validation(&field, "pattern", false, 100);
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let field_errors = collector.get_field_errors();
        assert_eq!(field_errors.len(), 5);
        // Each field should have 2 errors (10 total / 5 fields)
        for i in 0..5 {
            let field = format!("field_{}", i);
            assert_eq!(field_errors.get(&field), Some(&2));
        }
    }

    #[test]
    fn test_concurrent_validation_counting() {
        let collector = Arc::new(ValidationMetricsCollector::new());
        let mut handles = vec![];

        for _ in 0..100 {
            let collector_clone = collector.clone();
            let handle = std::thread::spawn(move || {
                collector_clone.record_validation("email", "pattern", false, 10);
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(collector.validation_checks_total.load(Ordering::Relaxed), 100);
        assert_eq!(collector.validation_errors_total.load(Ordering::Relaxed), 100);
    }

    #[test]
    fn test_validation_metric_entry_serialization() {
        let entry = ValidationMetricEntry {
            field: "email".to_string(),
            rule_type: "pattern".to_string(),
            valid: false,
            duration_us: 150,
            validator_type: "regex".to_string(),
            failure_reason: Some("Invalid email format".to_string()),
        };

        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("email"));
        assert!(json.contains("pattern"));
        assert!(!json.contains("\"valid\":true"));

        let deserialized: ValidationMetricEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.field, entry.field);
        assert_eq!(deserialized.rule_type, entry.rule_type);
        assert!(!deserialized.valid);
    }

    #[test]
    fn test_default_constructor() {
        let collector = ValidationMetricsCollector::default();
        assert_eq!(collector.validation_checks_total.load(Ordering::Relaxed), 0);
        assert_eq!(collector.validation_errors_total.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_prometheus_validation_metrics_conversion() {
        let collector = ValidationMetricsCollector::new();
        collector.validation_checks_total.store(100, Ordering::Relaxed);
        collector.validation_errors_total.store(10, Ordering::Relaxed);
        collector.async_validation_total.store(50, Ordering::Relaxed);
        collector.async_validation_errors.store(5, Ordering::Relaxed);

        let metrics = PrometheusValidationMetrics::from(&collector);

        assert_eq!(metrics.validation_checks_total, 100);
        assert_eq!(metrics.validation_errors_total, 10);
        assert_eq!(metrics.async_validation_total, 50);
        assert_eq!(metrics.async_validation_errors, 5);
    }

    #[test]
    fn test_prometheus_validation_metrics_output_format() {
        let collector = ValidationMetricsCollector::new();
        collector.validation_checks_total.store(100, Ordering::Relaxed);
        collector.validation_errors_total.store(10, Ordering::Relaxed);

        let metrics = PrometheusValidationMetrics::from(&collector);
        let output = metrics.to_prometheus_format();

        assert!(output.contains("fraiseql_validation_checks_total 100"));
        assert!(output.contains("fraiseql_validation_errors_total 10"));
        assert!(output.contains("# HELP"));
        assert!(output.contains("# TYPE"));
    }

    #[test]
    fn test_prometheus_validation_metrics_average_calculation() {
        let collector = ValidationMetricsCollector::new();
        collector.validation_checks_total.store(10, Ordering::Relaxed);
        collector.validation_duration_us.store(1000, Ordering::Relaxed); // 1000 us total

        let metrics = PrometheusValidationMetrics::from(&collector);
        assert!((metrics.validation_avg_duration_us - 100.0).abs() < 0.01); // 100 us average
    }

    #[test]
    fn test_prometheus_validation_metrics_async_average() {
        let collector = ValidationMetricsCollector::new();
        collector.async_validation_total.store(5, Ordering::Relaxed);
        collector.async_validation_duration_us.store(2500, Ordering::Relaxed); // 2500 us total

        let metrics = PrometheusValidationMetrics::from(&collector);
        assert!((metrics.async_validation_avg_duration_us - 500.0).abs() < 0.01); // 500 us average
    }

    #[test]
    fn test_prometheus_validation_metrics_zero_checks() {
        let collector = ValidationMetricsCollector::new();
        let metrics = PrometheusValidationMetrics::from(&collector);

        assert_eq!(metrics.validation_avg_duration_us, 0.0);
        assert_eq!(metrics.async_validation_avg_duration_us, 0.0);
    }

    #[test]
    fn test_prometheus_validation_metrics_serialization() {
        let metrics = PrometheusValidationMetrics {
            validation_checks_total: 100,
            validation_errors_total: 10,
            async_validation_total: 50,
            async_validation_errors: 5,
            validation_avg_duration_us: 100.5,
            async_validation_avg_duration_us: 250.75,
        };

        let json = serde_json::to_string(&metrics).unwrap();
        assert!(json.contains("100"));
        assert!(json.contains("10"));

        let deserialized: PrometheusValidationMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.validation_checks_total, 100);
        assert_eq!(deserialized.validation_errors_total, 10);
    }

    #[test]
    fn test_snapshot_prometheus() {
        let collector = ValidationMetricsCollector::new();
        collector.validation_checks_total.store(100, Ordering::Relaxed);
        collector.validation_errors_total.store(10, Ordering::Relaxed);
        collector.async_validation_total.store(50, Ordering::Relaxed);

        let snapshot = collector.snapshot_prometheus();

        assert_eq!(snapshot.validation_checks_total, 100);
        assert_eq!(snapshot.validation_errors_total, 10);
        assert_eq!(snapshot.async_validation_total, 50);
    }
}
