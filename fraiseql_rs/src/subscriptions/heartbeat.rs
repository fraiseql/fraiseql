//! WebSocket heartbeat system
//!
//! Implements ping/pong keep-alive for WebSocket connections with dead connection detection.

use crate::subscriptions::config::WebSocketConfig;
use crate::subscriptions::metrics::SubscriptionMetrics;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Heartbeat state for a connection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeartbeatState {
    /// Healthy - last pong received recently
    Healthy,

    /// Waiting for pong response
    AwaitingPong,

    /// Pong timeout - connection is dead
    Dead,
}

/// Heartbeat manager for a single connection
#[derive(Debug)]
pub struct ConnectionHeartbeat {
    /// Connection ID
    pub connection_id: Uuid,

    /// Current heartbeat state
    pub state: HeartbeatState,

    /// Time of last ping sent
    pub last_ping_sent: Option<Instant>,

    /// Time of last pong received
    pub last_pong_received: Instant,

    /// WebSocket configuration (timeouts, intervals)
    config: Arc<WebSocketConfig>,

    /// Metrics collector
    metrics: Option<Arc<SubscriptionMetrics>>,
}

impl ConnectionHeartbeat {
    /// Create new heartbeat manager
    #[must_use]
    pub fn new(
        connection_id: Uuid,
        config: Arc<WebSocketConfig>,
        metrics: Option<Arc<SubscriptionMetrics>>,
    ) -> Self {
        let now = Instant::now();
        Self {
            connection_id,
            state: HeartbeatState::Healthy,
            last_ping_sent: None,
            last_pong_received: now,
            config,
            metrics,
        }
    }

    /// Check if it's time to send a ping
    #[must_use]
    pub fn should_ping(&self) -> bool {
        self.last_ping_sent
            .is_none_or(|last_ping| last_ping.elapsed() >= self.config.ping_interval)
    }

    /// Record a ping sent
    pub fn ping_sent(&mut self) {
        self.last_ping_sent = Some(Instant::now());
        self.state = HeartbeatState::AwaitingPong;
    }

    /// Record a pong received
    pub fn pong_received(&mut self) {
        self.last_pong_received = Instant::now();
        self.state = HeartbeatState::Healthy;
    }

    /// Check if pong timeout has been exceeded
    #[must_use]
    pub fn is_pong_timeout(&self) -> bool {
        if self.state != HeartbeatState::AwaitingPong {
            return false;
        }

        self.last_ping_sent
            .is_some_and(|last_ping| last_ping.elapsed() > self.config.pong_timeout)
    }

    /// Detect and record dead connection
    pub fn check_dead(&mut self) -> bool {
        if self.is_pong_timeout() {
            self.state = HeartbeatState::Dead;
            if let Some(metrics) = &self.metrics {
                metrics.record_error("pong_timeout");
            }
            return true;
        }
        false
    }

    /// Get time until next ping should be sent
    #[must_use]
    pub fn time_until_next_ping(&self) -> Duration {
        self.last_ping_sent.map_or(Duration::ZERO, |last_ping| {
            let elapsed = last_ping.elapsed();
            if elapsed >= self.config.ping_interval {
                Duration::ZERO
            } else {
                self.config.ping_interval - elapsed
            }
        })
    }

    /// Get time remaining before pong timeout
    #[must_use]
    pub fn time_until_pong_timeout(&self) -> Option<Duration> {
        if self.state != HeartbeatState::AwaitingPong {
            return None;
        }

        self.last_ping_sent.map_or_else(
            || Some(self.config.pong_timeout),
            |last_ping| {
                let elapsed = last_ping.elapsed();
                if elapsed >= self.config.pong_timeout {
                    Some(Duration::ZERO)
                } else {
                    Some(self.config.pong_timeout - elapsed)
                }
            },
        )
    }
}

/// Heartbeat monitor for all connections
#[derive(Debug)]
pub struct HeartbeatMonitor {
    /// WebSocket configuration
    config: Arc<WebSocketConfig>,

    /// Metrics collector
    metrics: Option<Arc<SubscriptionMetrics>>,
}

impl HeartbeatMonitor {
    /// Create new heartbeat monitor
    #[must_use]
    pub const fn new(
        config: Arc<WebSocketConfig>,
        metrics: Option<Arc<SubscriptionMetrics>>,
    ) -> Self {
        Self { config, metrics }
    }

    /// Create heartbeat for a connection
    #[must_use]
    pub fn create_heartbeat(&self, connection_id: Uuid) -> ConnectionHeartbeat {
        ConnectionHeartbeat::new(connection_id, self.config.clone(), self.metrics.clone())
    }

    /// Get ping interval from configuration
    #[must_use]
    pub fn ping_interval(&self) -> Duration {
        self.config.ping_interval
    }

    /// Get pong timeout from configuration
    #[must_use]
    pub fn pong_timeout(&self) -> Duration {
        self.config.pong_timeout
    }

    /// Get graceful shutdown timeout from configuration
    #[must_use]
    pub fn shutdown_grace_period(&self) -> Duration {
        self.config.shutdown_grace
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> Arc<WebSocketConfig> {
        Arc::new(WebSocketConfig {
            init_timeout: Duration::from_secs(5),
            ping_interval: Duration::from_secs(30),
            pong_timeout: Duration::from_secs(10),
            shutdown_grace: Duration::from_secs(5),
            max_message_size: 64 * 1024,
            message_buffer_capacity: 1000,
        })
    }

    #[test]
    fn test_heartbeat_creation() {
        let config = create_test_config();
        let connection_id = Uuid::new_v4();
        let heartbeat = ConnectionHeartbeat::new(connection_id, config, None);

        assert_eq!(heartbeat.connection_id, connection_id);
        assert_eq!(heartbeat.state, HeartbeatState::Healthy);
        assert_eq!(heartbeat.last_ping_sent, None);
    }

    #[test]
    fn test_should_ping_on_creation() {
        let config = create_test_config();
        let connection_id = Uuid::new_v4();
        let heartbeat = ConnectionHeartbeat::new(connection_id, config, None);

        assert!(heartbeat.should_ping());
    }

    #[test]
    fn test_ping_sent_state_transition() {
        let config = create_test_config();
        let connection_id = Uuid::new_v4();
        let mut heartbeat = ConnectionHeartbeat::new(connection_id, config, None);

        heartbeat.ping_sent();
        assert_eq!(heartbeat.state, HeartbeatState::AwaitingPong);
        assert!(heartbeat.last_ping_sent.is_some());
    }

    #[test]
    fn test_pong_received_state_transition() {
        let config = create_test_config();
        let connection_id = Uuid::new_v4();
        let mut heartbeat = ConnectionHeartbeat::new(connection_id, config, None);

        heartbeat.ping_sent();
        assert_eq!(heartbeat.state, HeartbeatState::AwaitingPong);

        heartbeat.pong_received();
        assert_eq!(heartbeat.state, HeartbeatState::Healthy);
    }

    #[test]
    fn test_pong_timeout_detection() {
        let config = Arc::new(WebSocketConfig {
            pong_timeout: Duration::from_millis(100),
            ..Default::default()
        });
        let connection_id = Uuid::new_v4();
        let mut heartbeat = ConnectionHeartbeat::new(connection_id, config, None);

        heartbeat.ping_sent();
        assert!(!heartbeat.is_pong_timeout());

        // Wait for timeout
        std::thread::sleep(Duration::from_millis(150));
        assert!(heartbeat.is_pong_timeout());
    }

    #[test]
    fn test_check_dead_detects_timeout() {
        let config = Arc::new(WebSocketConfig {
            pong_timeout: Duration::from_millis(100),
            ..Default::default()
        });
        let connection_id = Uuid::new_v4();
        let mut heartbeat = ConnectionHeartbeat::new(connection_id, config, None);

        heartbeat.ping_sent();
        assert!(!heartbeat.check_dead());

        // Wait for timeout
        std::thread::sleep(Duration::from_millis(150));
        assert!(heartbeat.check_dead());
        assert_eq!(heartbeat.state, HeartbeatState::Dead);
    }

    #[test]
    fn test_heartbeat_monitor_creation() {
        let config = create_test_config();
        let monitor = HeartbeatMonitor::new(config, None);

        assert_eq!(monitor.ping_interval(), Duration::from_secs(30));
        assert_eq!(monitor.pong_timeout(), Duration::from_secs(10));
        assert_eq!(monitor.shutdown_grace_period(), Duration::from_secs(5));
    }

    #[test]
    fn test_heartbeat_monitor_create_heartbeat() {
        let config = create_test_config();
        let monitor = HeartbeatMonitor::new(config, None);
        let connection_id = Uuid::new_v4();

        let heartbeat = monitor.create_heartbeat(connection_id);
        assert_eq!(heartbeat.connection_id, connection_id);
        assert_eq!(heartbeat.state, HeartbeatState::Healthy);
    }
}
