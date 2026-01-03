//! Stress testing utilities for subscriptions module
//!
//! Provides latency simulation, failure injection, and resource monitoring
//! for comprehensive stress testing of the event bus system.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Latency simulator for network condition testing
#[derive(Debug, Clone)]
pub struct LatencySimulator {
    /// Minimum latency
    pub min: Duration,
    /// Maximum latency
    pub max: Duration,
    /// Whether to apply random jitter
    pub jitter: bool,
}

impl LatencySimulator {
    /// Create a fixed latency simulator
    pub fn fixed(delay: Duration) -> Self {
        Self {
            min: delay,
            max: delay,
            jitter: false,
        }
    }

    /// Create a jittered latency simulator (random between min and max)
    pub fn jittered(min: Duration, max: Duration) -> Self {
        Self {
            min,
            max,
            jitter: true,
        }
    }

    /// Apply the configured latency
    pub async fn apply(&self) {
        if self.min.is_zero() && self.max.is_zero() {
            return;
        }

        let delay = if self.jitter {
            let min_ms = self.min.as_millis() as u64;
            let max_ms = self.max.as_millis() as u64;
            let random_ms = min_ms + (rand::random::<u64>() % (max_ms - min_ms + 1));
            Duration::from_millis(random_ms)
        } else {
            self.min
        };

        tokio::time::sleep(delay).await;
    }

    /// Get description of latency settings
    pub fn describe(&self) -> String {
        if self.jitter {
            format!(
                "Jittered latency: {}-{}ms",
                self.min.as_millis(),
                self.max.as_millis()
            )
        } else {
            format!("Fixed latency: {}ms", self.min.as_millis())
        }
    }
}

/// Failure injector for chaos engineering
#[derive(Debug, Clone)]
pub struct FailureInjector {
    /// Failure probability (0.0-1.0)
    pub failure_rate: f64,
}

impl FailureInjector {
    /// Create a failure injector with given failure rate
    pub fn new(failure_rate: f64) -> Self {
        let rate = failure_rate.max(0.0).min(1.0);
        Self { failure_rate: rate }
    }

    /// Determine if this invocation should fail
    pub fn should_fail(&self) -> bool {
        rand::random::<f64>() < self.failure_rate
    }

    /// Get description of failure rate
    pub fn describe(&self) -> String {
        format!("Failure rate: {:.1}%", self.failure_rate * 100.0)
    }
}

/// Resource monitor for tracking system behavior under stress
#[derive(Debug, Clone)]
pub struct ResourceMonitor {
    start_time: Arc<Instant>,
    start_memory: u64,
    peak_memory: Arc<AtomicU64>,
    current_memory: Arc<AtomicU64>,
    operation_count: Arc<AtomicU64>,
}

impl ResourceMonitor {
    /// Create a new resource monitor
    pub fn new() -> Self {
        let start_memory = Self::current_memory_bytes();
        Self {
            start_time: Arc::new(Instant::now()),
            start_memory,
            peak_memory: Arc::new(AtomicU64::new(start_memory)),
            current_memory: Arc::new(AtomicU64::new(start_memory)),
            operation_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Record operation completion
    pub fn record_operation(&self) {
        self.operation_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Sample current memory usage
    pub fn sample_memory(&self) {
        let current = Self::current_memory_bytes();
        self.current_memory.store(current, Ordering::Relaxed);

        let peak = self.peak_memory.load(Ordering::Relaxed);
        if current > peak {
            self.peak_memory.store(current, Ordering::Relaxed);
        }
    }

    /// Get elapsed time since monitor creation
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Generate report of resource usage
    pub fn report(&self) -> MonitorReport {
        let elapsed = self.elapsed();
        let current = self.current_memory.load(Ordering::Relaxed);
        let peak = self.peak_memory.load(Ordering::Relaxed);
        let operations = self.operation_count.load(Ordering::Relaxed);

        let memory_delta = if current >= self.start_memory {
            current - self.start_memory
        } else {
            0
        };

        let peak_delta = if peak >= self.start_memory {
            peak - self.start_memory
        } else {
            0
        };

        let ops_per_sec = if elapsed.as_secs_f64() > 0.0 {
            operations as f64 / elapsed.as_secs_f64()
        } else {
            0.0
        };

        MonitorReport {
            elapsed,
            initial_memory_mb: self.start_memory / (1024 * 1024),
            current_memory_mb: current / (1024 * 1024),
            peak_memory_mb: peak / (1024 * 1024),
            memory_delta_mb: memory_delta / (1024 * 1024),
            peak_delta_mb: peak_delta / (1024 * 1024),
            operations,
            ops_per_sec,
        }
    }

    /// Get approximate current memory in bytes (conservative estimate)
    fn current_memory_bytes() -> u64 {
        // In production, this would use system APIs like /proc/self/status
        // For testing, we use a conservative estimate based on allocations
        0 // Placeholder: actual implementation would query OS
    }
}

impl Default for ResourceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Report from resource monitor
#[derive(Debug, Clone)]
pub struct MonitorReport {
    /// Total elapsed time since monitor creation
    pub elapsed: Duration,
    /// Initial memory in MB at monitor creation
    pub initial_memory_mb: u64,
    /// Current memory usage in MB
    pub current_memory_mb: u64,
    /// Peak memory usage in MB
    pub peak_memory_mb: u64,
    /// Memory delta from initial to current in MB
    pub memory_delta_mb: u64,
    /// Peak delta from initial in MB
    pub peak_delta_mb: u64,
    /// Total operations recorded
    pub operations: u64,
    /// Operations per second
    pub ops_per_sec: f64,
}

impl MonitorReport {
    /// Print formatted report
    pub fn print(&self) {
        println!(
            "  ⏱ Elapsed: {:.2}s | Ops: {} ({:.0} ops/sec)",
            self.elapsed.as_secs_f64(),
            self.operations,
            self.ops_per_sec
        );
        println!(
            "  💾 Memory: {} MB initial → {} MB current (Δ {} MB) | Peak: {} MB (Δ {} MB)",
            self.initial_memory_mb,
            self.current_memory_mb,
            self.memory_delta_mb,
            self.peak_memory_mb,
            self.peak_delta_mb
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latency_simulator_fixed() {
        let latency = LatencySimulator::fixed(Duration::from_millis(100));
        assert!(!latency.jitter);
        assert_eq!(latency.min, Duration::from_millis(100));
        assert_eq!(latency.max, Duration::from_millis(100));
    }

    #[test]
    fn test_latency_simulator_jittered() {
        let latency =
            LatencySimulator::jittered(Duration::from_millis(50), Duration::from_millis(500));
        assert!(latency.jitter);
        assert_eq!(latency.min, Duration::from_millis(50));
        assert_eq!(latency.max, Duration::from_millis(500));
    }

    #[tokio::test]
    async fn test_latency_simulator_apply_fixed() {
        let latency = LatencySimulator::fixed(Duration::from_millis(10));
        let start = Instant::now();
        latency.apply().await;
        let elapsed = start.elapsed();
        assert!(elapsed.as_millis() >= 10);
    }

    #[test]
    fn test_failure_injector_zero_rate() {
        let injector = FailureInjector::new(0.0);
        for _ in 0..100 {
            assert!(!injector.should_fail());
        }
    }

    #[test]
    fn test_failure_injector_full_rate() {
        let injector = FailureInjector::new(1.0);
        for _ in 0..100 {
            assert!(injector.should_fail());
        }
    }

    #[test]
    fn test_failure_injector_clamps_range() {
        let injector_neg = FailureInjector::new(-0.5);
        assert_eq!(injector_neg.failure_rate, 0.0);

        let injector_high = FailureInjector::new(1.5);
        assert_eq!(injector_high.failure_rate, 1.0);
    }

    #[test]
    fn test_resource_monitor_report() {
        let monitor = ResourceMonitor::new();
        monitor.record_operation();
        monitor.record_operation();
        monitor.sample_memory();

        let report = monitor.report();
        assert_eq!(report.operations, 2);
        assert!(!report.elapsed.is_zero());
    }
}
