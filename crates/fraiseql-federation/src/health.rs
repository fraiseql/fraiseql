//! Subgraph health check aggregation for federation gateway mode.
//!
//! Tracks the health status of all registered subgraphs, providing
//! an aggregate health endpoint for monitoring and load balancing.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Health status of a single subgraph.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SubgraphHealthStatus {
    /// Subgraph is healthy and responding
    Healthy,

    /// Subgraph is degraded (slow responses or partial failures)
    Degraded,

    /// Subgraph is unhealthy (not responding or erroring)
    Unhealthy,

    /// Health status is unknown (not yet checked)
    Unknown,
}

impl std::fmt::Display for SubgraphHealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Healthy => write!(f, "healthy"),
            Self::Degraded => write!(f, "degraded"),
            Self::Unhealthy => write!(f, "unhealthy"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Health report for a single subgraph.
#[derive(Debug, Clone)]
pub struct SubgraphHealthReport {
    /// Subgraph name
    pub name: String,

    /// Current health status
    pub status: SubgraphHealthStatus,

    /// Subgraph URL
    pub url: String,

    /// Last successful check time
    pub last_check: Option<Instant>,

    /// Last response latency
    pub last_latency: Option<Duration>,

    /// Consecutive failure count
    pub consecutive_failures: u32,
}

/// Aggregate health status for all subgraphs in the federation.
#[derive(Debug, Clone)]
pub struct FederationHealthReport {
    /// Overall federation health (worst of all subgraphs)
    pub overall_status: SubgraphHealthStatus,

    /// Per-subgraph health reports
    pub subgraphs: Vec<SubgraphHealthReport>,

    /// Number of healthy subgraphs
    pub healthy_count: usize,

    /// Number of degraded subgraphs
    pub degraded_count: usize,

    /// Number of unhealthy subgraphs
    pub unhealthy_count: usize,
}

/// Aggregates health status across all registered subgraphs.
pub struct SubgraphHealthAggregator {
    subgraphs: Mutex<HashMap<String, SubgraphHealthReport>>,
}

impl SubgraphHealthAggregator {
    /// Create a new health aggregator.
    pub fn new() -> Self {
        Self {
            subgraphs: Mutex::new(HashMap::new()),
        }
    }

    /// Register a subgraph for health tracking.
    pub fn register(&self, name: impl Into<String>, url: impl Into<String>) {
        let name = name.into();
        let url = url.into();
        let mut subgraphs = self.subgraphs.lock().unwrap_or_else(|e| e.into_inner());
        subgraphs.insert(
            name.clone(),
            SubgraphHealthReport {
                name,
                status: SubgraphHealthStatus::Unknown,
                url,
                last_check: None,
                last_latency: None,
                consecutive_failures: 0,
            },
        );
    }

    /// Report a successful health check for a subgraph.
    pub fn report_healthy(&self, name: &str, latency: Duration) {
        let mut subgraphs = self.subgraphs.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(report) = subgraphs.get_mut(name) {
            report.status = if latency > Duration::from_secs(5) {
                SubgraphHealthStatus::Degraded
            } else {
                SubgraphHealthStatus::Healthy
            };
            report.last_check = Some(Instant::now());
            report.last_latency = Some(latency);
            report.consecutive_failures = 0;
        }
    }

    /// Report a failed health check for a subgraph.
    pub fn report_unhealthy(&self, name: &str) {
        let mut subgraphs = self.subgraphs.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(report) = subgraphs.get_mut(name) {
            report.consecutive_failures += 1;
            report.status = SubgraphHealthStatus::Unhealthy;
            report.last_check = Some(Instant::now());
            report.last_latency = None;
        }
    }

    /// Get the aggregate health report.
    pub fn aggregate(&self) -> FederationHealthReport {
        let subgraphs = self.subgraphs.lock().unwrap_or_else(|e| e.into_inner());

        let reports: Vec<SubgraphHealthReport> = subgraphs.values().cloned().collect();

        let healthy_count = reports
            .iter()
            .filter(|r| r.status == SubgraphHealthStatus::Healthy)
            .count();
        let degraded_count = reports
            .iter()
            .filter(|r| r.status == SubgraphHealthStatus::Degraded)
            .count();
        let unhealthy_count = reports
            .iter()
            .filter(|r| r.status == SubgraphHealthStatus::Unhealthy)
            .count();

        let overall_status = if unhealthy_count > 0 {
            SubgraphHealthStatus::Unhealthy
        } else if degraded_count > 0 {
            SubgraphHealthStatus::Degraded
        } else if healthy_count > 0 {
            SubgraphHealthStatus::Healthy
        } else {
            SubgraphHealthStatus::Unknown
        };

        FederationHealthReport {
            overall_status,
            subgraphs: reports,
            healthy_count,
            degraded_count,
            unhealthy_count,
        }
    }

    /// Number of registered subgraphs.
    pub fn subgraph_count(&self) -> usize {
        let subgraphs = self.subgraphs.lock().unwrap_or_else(|e| e.into_inner());
        subgraphs.len()
    }
}

impl Default for SubgraphHealthAggregator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    #[test]
    fn test_register_subgraph() {
        let agg = SubgraphHealthAggregator::new();
        agg.register("users", "http://users:4000/graphql");
        agg.register("orders", "http://orders:4001/graphql");

        assert_eq!(agg.subgraph_count(), 2);
    }

    #[test]
    fn test_healthy_report() {
        let agg = SubgraphHealthAggregator::new();
        agg.register("users", "http://users:4000/graphql");
        agg.report_healthy("users", Duration::from_millis(10));

        let report = agg.aggregate();
        assert_eq!(report.overall_status, SubgraphHealthStatus::Healthy);
        assert_eq!(report.healthy_count, 1);
        assert_eq!(report.unhealthy_count, 0);
    }

    #[test]
    fn test_degraded_report() {
        let agg = SubgraphHealthAggregator::new();
        agg.register("slow_service", "http://slow:4000/graphql");
        agg.report_healthy("slow_service", Duration::from_secs(10));

        let report = agg.aggregate();
        assert_eq!(report.overall_status, SubgraphHealthStatus::Degraded);
        assert_eq!(report.degraded_count, 1);
    }

    #[test]
    fn test_unhealthy_report() {
        let agg = SubgraphHealthAggregator::new();
        agg.register("users", "http://users:4000/graphql");
        agg.report_unhealthy("users");

        let report = agg.aggregate();
        assert_eq!(report.overall_status, SubgraphHealthStatus::Unhealthy);
        assert_eq!(report.unhealthy_count, 1);

        let user_report = &report.subgraphs[0];
        assert_eq!(user_report.consecutive_failures, 1);
    }

    #[test]
    fn test_mixed_health_worst_wins() {
        let agg = SubgraphHealthAggregator::new();
        agg.register("users", "http://users:4000/graphql");
        agg.register("orders", "http://orders:4001/graphql");

        agg.report_healthy("users", Duration::from_millis(10));
        agg.report_unhealthy("orders");

        let report = agg.aggregate();
        assert_eq!(
            report.overall_status,
            SubgraphHealthStatus::Unhealthy,
            "worst status should win"
        );
        assert_eq!(report.healthy_count, 1);
        assert_eq!(report.unhealthy_count, 1);
    }

    #[test]
    fn test_recovery_resets_failures() {
        let agg = SubgraphHealthAggregator::new();
        agg.register("users", "http://users:4000/graphql");

        agg.report_unhealthy("users");
        agg.report_unhealthy("users");

        let report = agg.aggregate();
        assert_eq!(report.subgraphs[0].consecutive_failures, 2);

        agg.report_healthy("users", Duration::from_millis(5));

        let report = agg.aggregate();
        assert_eq!(report.subgraphs[0].consecutive_failures, 0);
        assert_eq!(report.overall_status, SubgraphHealthStatus::Healthy);
    }

    #[test]
    fn test_unknown_initial_status() {
        let agg = SubgraphHealthAggregator::new();
        agg.register("users", "http://users:4000/graphql");

        let report = agg.aggregate();
        assert_eq!(report.overall_status, SubgraphHealthStatus::Unknown);
        assert_eq!(report.subgraphs[0].status, SubgraphHealthStatus::Unknown);
    }

    #[test]
    fn test_empty_aggregator() {
        let agg = SubgraphHealthAggregator::new();
        let report = agg.aggregate();
        assert_eq!(report.overall_status, SubgraphHealthStatus::Unknown);
        assert!(report.subgraphs.is_empty());
    }

    #[test]
    fn test_health_status_display() {
        assert_eq!(SubgraphHealthStatus::Healthy.to_string(), "healthy");
        assert_eq!(SubgraphHealthStatus::Degraded.to_string(), "degraded");
        assert_eq!(SubgraphHealthStatus::Unhealthy.to_string(), "unhealthy");
        assert_eq!(SubgraphHealthStatus::Unknown.to_string(), "unknown");
    }
}
