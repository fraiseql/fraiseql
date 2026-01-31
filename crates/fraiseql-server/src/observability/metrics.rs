//! Metrics collection and export
//!
//! Provides metrics for observability (counters, histograms, gauges)

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A metrics counter
#[derive(Clone, Debug)]
pub struct MetricCounter {
    /// Counter name
    pub name: String,
    /// Counter labels
    pub labels: HashMap<String, String>,
    /// Counter value
    pub value: u64,
}

impl MetricCounter {
    /// Create a new counter
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            labels: HashMap::new(),
            value: 0,
        }
    }

    /// Add a label to the counter
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Increment the counter
    pub fn increment(&mut self) {
        self.value += 1;
    }

    /// Increment by n
    pub fn increment_by(&mut self, n: u64) {
        self.value += n;
    }
}

/// A metrics histogram
#[derive(Clone, Debug)]
pub struct MetricHistogram {
    /// Histogram name
    pub name: String,
    /// Histogram buckets
    pub buckets: Vec<u64>,
    /// Observed values
    pub values: Vec<u64>,
}

impl MetricHistogram {
    /// Create a new histogram with standard buckets
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            buckets: vec![1, 5, 10, 25, 50, 100, 250, 500, 1000],
            values: Vec::new(),
        }
    }

    /// Record an observation
    pub fn observe(&mut self, value: u64) {
        self.values.push(value);
    }

    /// Get min value
    pub fn min(&self) -> Option<u64> {
        self.values.iter().copied().min()
    }

    /// Get max value
    pub fn max(&self) -> Option<u64> {
        self.values.iter().copied().max()
    }

    /// Get mean value
    pub fn mean(&self) -> Option<f64> {
        if self.values.is_empty() {
            return None;
        }
        let sum: u64 = self.values.iter().sum();
        Some(sum as f64 / self.values.len() as f64)
    }
}

/// Metrics registry
pub struct MetricsRegistry {
    counters: Arc<Mutex<Vec<MetricCounter>>>,
    histograms: Arc<Mutex<Vec<MetricHistogram>>>,
}

impl MetricsRegistry {
    /// Create a new registry
    pub fn new() -> Self {
        Self {
            counters: Arc::new(Mutex::new(Vec::new())),
            histograms: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Register a counter
    pub fn register_counter(&self, counter: MetricCounter) -> Result<(), String> {
        let mut counters = self.counters.lock().map_err(|e| e.to_string())?;
        counters.push(counter);
        Ok(())
    }

    /// Register a histogram
    pub fn register_histogram(&self, histogram: MetricHistogram) -> Result<(), String> {
        let mut histograms = self.histograms.lock().map_err(|e| e.to_string())?;
        histograms.push(histogram);
        Ok(())
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for MetricsRegistry {
    fn clone(&self) -> Self {
        Self {
            counters: Arc::clone(&self.counters),
            histograms: Arc::clone(&self.histograms),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter() {
        let mut counter = MetricCounter::new("test_counter");
        assert_eq!(counter.value, 0);

        counter.increment();
        assert_eq!(counter.value, 1);

        counter.increment_by(5);
        assert_eq!(counter.value, 6);
    }

    #[test]
    fn test_histogram() {
        let mut histogram = MetricHistogram::new("test_histogram");
        assert_eq!(histogram.buckets.len(), 9);

        histogram.observe(10);
        histogram.observe(50);
        histogram.observe(100);

        assert_eq!(histogram.min(), Some(10));
        assert_eq!(histogram.max(), Some(100));
    }
}
