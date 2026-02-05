//! Subgraph health checking for federation queries.
//!
//! This module provides:
//! - Periodic health checks to all configured subgraphs
//! - Liveness probes (fast, frequent)
//! - Availability tracking
//! - Error rate monitoring

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Subgraph health status snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubgraphHealthStatus {
    /// Subgraph name
    pub name: String,

    /// Is subgraph available
    pub available: bool,

    /// Request latency in milliseconds
    pub latency_ms: f64,

    /// Last check timestamp
    pub last_check: String,

    /// Error count in last 60 seconds
    pub error_count_last_60s: u32,

    /// Error rate percentage (0-100)
    pub error_rate_percent: f64,
}

/// Rolling error window - tracks errors in a 60-second window with 10-second buckets.
#[derive(Debug)]
pub struct RollingErrorWindow {
    // Time-bucketed storage: [0]=last 10s, [1]=prev 10s, etc. (6 buckets = 60s)
    buckets: Mutex<VecDeque<ErrorBucket>>,
}

#[derive(Debug, Clone)]
struct ErrorBucket {
    timestamp: Instant,
    errors:    u32,
    total:     u32,
}

impl RollingErrorWindow {
    /// Create new rolling error window.
    pub fn new() -> Self {
        Self {
            buckets: Mutex::new(VecDeque::with_capacity(6)),
        }
    }

    /// Record a success.
    pub fn record_success(&self) {
        let mut buckets = self.buckets.lock().unwrap();
        if let Some(bucket) = buckets.back_mut() {
            bucket.total += 1;
        } else {
            buckets.push_back(ErrorBucket {
                timestamp: Instant::now(),
                errors:    0,
                total:     1,
            });
        }
    }

    /// Record an error.
    pub fn record_error(&self) {
        let mut buckets = self.buckets.lock().unwrap();
        if let Some(bucket) = buckets.back_mut() {
            bucket.errors += 1;
            bucket.total += 1;
        } else {
            buckets.push_back(ErrorBucket {
                timestamp: Instant::now(),
                errors:    1,
                total:     1,
            });
        }
    }

    /// Get error count in last 60 seconds.
    pub fn error_count(&self) -> u32 {
        let buckets = self.buckets.lock().unwrap();
        let now = Instant::now();
        buckets
            .iter()
            .filter(|b| now.duration_since(b.timestamp) < Duration::from_secs(60))
            .map(|b| b.errors)
            .sum()
    }

    /// Get error rate percentage over last 300 seconds (5 minutes).
    pub fn error_rate_percent(&self) -> f64 {
        let buckets = self.buckets.lock().unwrap();
        let now = Instant::now();
        let recent: Vec<_> = buckets
            .iter()
            .filter(|b| now.duration_since(b.timestamp) < Duration::from_secs(300))
            .collect();

        if recent.is_empty() {
            return 0.0;
        }

        let total_errors: u32 = recent.iter().map(|b| b.errors).sum();
        let total_checks: u32 = recent.iter().map(|b| b.total).sum();

        if total_checks == 0 {
            0.0
        } else {
            (total_errors as f64 / total_checks as f64) * 100.0
        }
    }

    /// Cleanup old buckets (older than 5 minutes).
    fn cleanup(&self) {
        let mut buckets = self.buckets.lock().unwrap();
        let now = Instant::now();
        while let Some(front) = buckets.front() {
            if now.duration_since(front.timestamp) > Duration::from_secs(300) {
                buckets.pop_front();
            } else {
                break;
            }
        }
    }
}

impl Default for RollingErrorWindow {
    fn default() -> Self {
        Self::new()
    }
}

/// Subgraph health checker.
pub struct SubgraphHealthChecker {
    subgraphs:     Vec<SubgraphConfig>,
    http_client:   reqwest::Client,
    error_windows: Arc<Mutex<std::collections::HashMap<String, RollingErrorWindow>>>,
    status_cache:  Arc<Mutex<Vec<SubgraphHealthStatus>>>,
}

/// Configuration for a single subgraph.
#[derive(Debug, Clone)]
pub struct SubgraphConfig {
    /// Subgraph name
    pub name: String,

    /// GraphQL endpoint URL
    pub endpoint: String,
}

impl SubgraphHealthChecker {
    /// Create new health checker with subgraph configurations.
    pub fn new(subgraphs: Vec<SubgraphConfig>) -> Self {
        let mut error_windows = std::collections::HashMap::new();
        for config in &subgraphs {
            error_windows.insert(config.name.clone(), RollingErrorWindow::new());
        }

        Self {
            subgraphs,
            http_client: reqwest::Client::new(),
            error_windows: Arc::new(Mutex::new(error_windows)),
            status_cache: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Check health of a single subgraph.
    async fn check_subgraph(&self, config: &SubgraphConfig) -> SubgraphHealthStatus {
        let start = Instant::now();

        // Simple liveness check: { __typename }
        let query = serde_json::json!({
            "query": "{ __typename }"
        });

        let result = self
            .http_client
            .post(&config.endpoint)
            .header("Content-Type", "application/json")
            .json(&query)
            .timeout(Duration::from_secs(2))
            .send()
            .await;

        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

        let available = matches!(result, Ok(ref resp) if resp.status() == 200);

        // Record result and get updated stats
        let (error_count, error_rate) = {
            let windows = self.error_windows.lock().unwrap();
            let window = windows.get(&config.name).expect("Window should exist for subgraph");

            if available {
                window.record_success();
            } else {
                if let Ok(resp) = &result {
                    warn!(
                        subgraph = %config.name,
                        status = %resp.status(),
                        latency_ms = latency_ms,
                        "Subgraph returned non-200 status"
                    );
                } else if let Err(e) = &result {
                    warn!(
                        subgraph = %config.name,
                        error = %e,
                        latency_ms = latency_ms,
                        "Subgraph health check failed"
                    );
                }
                window.record_error();
            }

            (window.error_count(), window.error_rate_percent())
        };

        SubgraphHealthStatus {
            name: config.name.clone(),
            available,
            latency_ms,
            last_check: Utc::now().to_rfc3339(),
            error_count_last_60s: error_count,
            error_rate_percent: error_rate,
        }
    }

    /// Run background health checks (every 30 seconds).
    pub async fn run_background_checks(self: Arc<Self>) {
        info!("Starting federation health check background task");

        loop {
            // Check all subgraphs
            let mut statuses = Vec::new();
            for config in &self.subgraphs {
                let status = self.check_subgraph(config).await;
                debug!(
                    subgraph = %status.name,
                    available = status.available,
                    latency_ms = status.latency_ms,
                    "Subgraph health check result"
                );
                statuses.push(status);
            }

            // Update cache
            {
                let mut cache = self.status_cache.lock().unwrap();
                *cache = statuses;
            }

            // Cleanup old error buckets
            {
                let windows = self.error_windows.lock().unwrap();
                for window in windows.values() {
                    window.cleanup();
                }
            }

            // Sleep for 30 seconds
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    }

    /// Get cached health statuses.
    pub fn get_cached_statuses(&self) -> Vec<SubgraphHealthStatus> {
        self.status_cache.lock().unwrap().iter().cloned().collect()
    }

    /// Get overall federation health status.
    pub fn get_overall_status(&self) -> String {
        let statuses = self.get_cached_statuses();

        if statuses.is_empty() {
            return "unknown".to_string();
        }

        if statuses.iter().all(|s| s.available) {
            "healthy".to_string()
        } else if statuses.iter().any(|s| s.available) {
            "degraded".to_string()
        } else {
            "unhealthy".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rolling_error_window_creation() {
        let window = RollingErrorWindow::new();
        assert_eq!(window.error_count(), 0);
        assert_eq!(window.error_rate_percent(), 0.0);
    }

    #[test]
    fn test_rolling_error_window_success() {
        let window = RollingErrorWindow::new();
        window.record_success();
        window.record_success();

        assert_eq!(window.error_count(), 0);
        assert_eq!(window.error_rate_percent(), 0.0);
    }

    #[test]
    fn test_rolling_error_window_mixed() {
        let window = RollingErrorWindow::new();
        window.record_success();
        window.record_success();
        window.record_error();

        assert_eq!(window.error_count(), 1);
        assert!((window.error_rate_percent() - 33.33).abs() < 0.1);
    }

    #[test]
    fn test_health_status_serialization() {
        let status = SubgraphHealthStatus {
            name:                 "test-subgraph".to_string(),
            available:            true,
            latency_ms:           25.5,
            last_check:           Utc::now().to_rfc3339(),
            error_count_last_60s: 0,
            error_rate_percent:   0.0,
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("test-subgraph"));
        assert!(json.contains("true"));
    }
}
