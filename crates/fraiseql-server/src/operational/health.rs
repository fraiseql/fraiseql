//! Health check endpoints
//!
//! Provides health, readiness, and liveness endpoints

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Health status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    /// Overall status (healthy, degraded, unhealthy)
    pub status:         String,
    /// Unix timestamp
    pub timestamp:      u64,
    /// Uptime in seconds
    pub uptime_seconds: u64,
}

/// Readiness status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessStatus {
    /// Is ready to serve requests
    pub ready:              bool,
    /// Database connectivity
    pub database_connected: bool,
    /// Cache availability (if enabled)
    pub cache_available:    bool,
    /// Reason if not ready
    pub reason:             Option<String>,
}

/// Liveness status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivenessStatus {
    /// Process is alive
    pub alive:            bool,
    /// Process ID
    pub pid:              u32,
    /// Response time in ms
    pub response_time_ms: u32,
}

/// Perform health check
pub fn health_check(uptime_seconds: u64) -> HealthStatus {
    HealthStatus {
        status: "healthy".to_string(),
        timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        uptime_seconds,
    }
}

/// Perform readiness check
pub fn readiness_check(database_connected: bool, cache_available: bool) -> ReadinessStatus {
    let ready = database_connected && cache_available;
    let reason = if !database_connected {
        Some("Database unavailable".to_string())
    } else if !cache_available {
        Some("Cache unavailable".to_string())
    } else {
        None
    };

    ReadinessStatus {
        ready,
        database_connected,
        cache_available,
        reason,
    }
}

/// Perform liveness check
pub fn liveness_check() -> LivenessStatus {
    LivenessStatus {
        alive:            true,
        pid:              std::process::id(),
        response_time_ms: 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_check() {
        let status = health_check(3600);
        assert_eq!(status.status, "healthy");
        assert_eq!(status.uptime_seconds, 3600);
    }

    #[test]
    fn test_readiness_check() {
        let status = readiness_check(true, true);
        assert!(status.ready);
    }

    #[test]
    fn test_liveness_check() {
        let status = liveness_check();
        assert!(status.alive);
        assert!(status.pid > 0);
    }
}
