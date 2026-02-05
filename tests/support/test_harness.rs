//! Test harness for Arrow Flight integration testing
//!
//! Provides utilities for:
//! - Starting test infrastructure (PostgreSQL, ClickHouse, Elasticsearch, NATS, Redis)
//! - Creating test databases and fixtures
//! - Verifying data across multiple backends

use std::sync::Arc;
use tokio::sync::Mutex;

/// Test environment for Arrow Flight integration tests
pub struct TestEnv {
    /// PostgreSQL connection string
    pub postgres_url: String,
    /// NATS server URL
    pub nats_url: String,
    /// Redis connection string
    pub redis_url: String,
    /// ClickHouse HTTP endpoint
    pub clickhouse_url: String,
    /// Elasticsearch endpoint
    pub elasticsearch_url: String,
    /// Test cleanup flag
    cleanup: Arc<Mutex<bool>>,
}

impl TestEnv {
    /// Create a new test environment with default connections
    pub fn new() -> Self {
        Self {
            postgres_url: "postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql"
                .to_string(),
            nats_url: "nats://localhost:4223".to_string(),
            redis_url: "redis://localhost:6380".to_string(),
            clickhouse_url: "http://localhost:8124".to_string(),
            elasticsearch_url: "http://localhost:9201".to_string(),
            cleanup: Arc::new(Mutex::new(true)),
        }
    }

    /// Wait for all services to be ready
    pub async fn wait_for_ready(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut retries = 0;
        const MAX_RETRIES: u32 = 30;

        loop {
            if self.check_services().await.is_ok() {
                return Ok(());
            }

            retries += 1;
            if retries >= MAX_RETRIES {
                return Err("Services failed to become ready after 30 seconds".into());
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }

    async fn check_services(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Check PostgreSQL
        let _pg_conn = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&self.postgres_url)
            .await?;

        // Check Redis
        let _redis_conn = redis::Client::open(self.redis_url.as_str())?
            .get_connection()?;

        // Check ClickHouse
        let _ch_response = reqwest::Client::new()
            .get(&format!("{}/?query=SELECT%201", self.clickhouse_url))
            .send()
            .await?;

        // Check Elasticsearch
        let _es_response = reqwest::Client::new()
            .get(&format!("{}/_cluster/health", self.elasticsearch_url))
            .send()
            .await?;

        Ok(())
    }

    /// Disable cleanup (for manual inspection)
    #[allow(dead_code)]
    pub async fn disable_cleanup(&self) {
        *self.cleanup.lock().await = false;
    }
}

impl Default for TestEnv {
    fn default() -> Self {
        Self::new()
    }
}

/// Performance measurement utilities
pub struct PerfMetrics {
    pub start_time: std::time::Instant,
    pub bytes_processed: std::sync::atomic::AtomicUsize,
}

impl PerfMetrics {
    pub fn new() -> Self {
        Self {
            start_time: std::time::Instant::now(),
            bytes_processed: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub fn elapsed_secs(&self) -> f64 {
        self.start_time.elapsed().as_secs_f64()
    }

    pub fn throughput_mbps(&self) -> f64 {
        let bytes = self.bytes_processed.load(std::sync::atomic::Ordering::Relaxed);
        let mb = bytes as f64 / (1024.0 * 1024.0);
        mb / self.elapsed_secs()
    }

    pub fn throughput_rows_sec(&self, row_size: usize) -> f64 {
        let bytes = self.bytes_processed.load(std::sync::atomic::Ordering::Relaxed);
        let rows = bytes / row_size;
        rows as f64 / self.elapsed_secs()
    }

    pub fn add_bytes(&self, bytes: usize) {
        self.bytes_processed.fetch_add(bytes, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Default for PerfMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory measurement utilities
#[cfg(target_os = "linux")]
pub fn get_rss_bytes() -> Result<usize, Box<dyn std::error::Error>> {
    let status = std::fs::read_to_string("/proc/self/status")?;
    let rss_line = status
        .lines()
        .find(|line| line.starts_with("VmRSS:"))
        .ok_or("VmRSS not found")?;

    let kb = rss_line
        .split_whitespace()
        .nth(1)
        .ok_or("Invalid VmRSS format")?
        .parse::<usize>()?;

    Ok(kb * 1024) // Convert KB to bytes
}

#[cfg(not(target_os = "linux"))]
pub fn get_rss_bytes() -> Result<usize, Box<dyn std::error::Error>> {
    Err("Memory measurement not supported on this platform".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_defaults() {
        let env = TestEnv::new();
        assert!(!env.postgres_url.is_empty());
        assert!(!env.nats_url.is_empty());
        assert!(!env.redis_url.is_empty());
    }

    #[test]
    fn test_perf_metrics() {
        let metrics = PerfMetrics::new();
        metrics.add_bytes(1024 * 1024); // 1 MB
        assert!(metrics.elapsed_secs() >= 0.0);
    }
}
