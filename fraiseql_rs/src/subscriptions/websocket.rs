//! WebSocket connection handler
//!
//! Manages WebSocket connections and implements the graphql-ws protocol lifecycle.

use crate::subscriptions::config::WebSocketConfig;
use crate::subscriptions::connection_manager::{ConnectionManager, ConnectionMetadata};
use crate::subscriptions::metrics::SubscriptionMetrics;
use crate::subscriptions::protocol::GraphQLMessage;
use crate::subscriptions::SubscriptionError;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// WebSocket connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Initial state, waiting for `ConnectionInit`
    Waiting,

    /// Connection initialized and ready for subscriptions
    Connected,

    /// Connection is closing
    Closing,

    /// Connection closed
    Closed,
}

/// WebSocket connection handler
pub struct WebSocketConnection {
    /// Connection ID
    pub connection_id: Uuid,

    /// Current connection state
    pub state: ConnectionState,

    /// Connection metadata from manager
    pub metadata: ConnectionMetadata,

    /// WebSocket configuration
    config: Arc<WebSocketConfig>,

    /// Connection manager reference
    manager: Arc<ConnectionManager>,

    /// Time connection was created
    created_at: Instant,

    /// Time of last activity
    last_activity: Instant,

    /// Connection initialization timeout
    init_timeout: Instant,
}

impl WebSocketConnection {
    /// Create new WebSocket connection
    #[must_use] 
    pub fn new(
        metadata: ConnectionMetadata,
        config: Arc<WebSocketConfig>,
        manager: Arc<ConnectionManager>,
    ) -> Self {
        let now = Instant::now();
        let init_timeout = now + config.init_timeout;

        Self {
            connection_id: metadata.id,
            state: ConnectionState::Waiting,
            metadata,
            config,
            manager,
            created_at: now,
            last_activity: now,
            init_timeout,
        }
    }

    /// Check if initialization timed out
    #[must_use] 
    pub fn init_timed_out(&self) -> bool {
        self.state == ConnectionState::Waiting && Instant::now() > self.init_timeout
    }

    /// Update last activity time
    pub fn update_activity(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Get connection uptime
    #[must_use] 
    pub fn uptime(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// Get time since last activity
    #[must_use] 
    pub fn idle_time(&self) -> Duration {
        self.last_activity.elapsed()
    }

    /// Transition to Connected state
    pub fn on_connection_init(&mut self) -> Result<(), SubscriptionError> {
        if self.state != ConnectionState::Waiting {
            return Err(SubscriptionError::InvalidMessage(
                "Connection already initialized".to_string(),
            ));
        }

        self.state = ConnectionState::Connected;
        self.update_activity();
        Ok(())
    }

    /// Handle incoming message
    pub fn handle_message(&mut self, message: GraphQLMessage) -> Result<(), SubscriptionError> {
        self.update_activity();

        match message {
            GraphQLMessage::ConnectionInit { .. } => {
                self.on_connection_init()?;
            }

            GraphQLMessage::Subscribe { id, payload: _ } => {
                if self.state != ConnectionState::Connected {
                    return Err(SubscriptionError::InvalidMessage(
                        "Must initialize connection first".to_string(),
                    ));
                }

                // Register subscription with connection manager
                self.manager.register_subscription(self.connection_id, id)?;
            }

            GraphQLMessage::Ping { .. } => {
                // Ping handled at protocol level
            }

            GraphQLMessage::Pong { .. } => {
                // Pong handled at protocol level
            }

            _ => {
                return Err(SubscriptionError::InvalidMessage(format!(
                    "Unexpected message type: {}",
                    message.type_name()
                )))
            }
        }

        Ok(())
    }

    /// Check if connection is alive
    #[must_use] 
    pub fn is_alive(&self) -> bool {
        self.state != ConnectionState::Closed
    }

    /// Start graceful shutdown
    pub fn start_shutdown(&mut self) {
        self.state = ConnectionState::Closing;
        self.update_activity();
    }

    /// Complete shutdown
    pub fn finish_shutdown(&mut self) -> Result<(), SubscriptionError> {
        self.state = ConnectionState::Closed;
        self.manager.unregister_connection(self.connection_id)?;
        Ok(())
    }

    /// Get connection info as JSON
    #[must_use] 
    pub fn as_json(&self) -> Value {
        serde_json::json!({
            "connection_id": self.connection_id.to_string(),
            "state": format!("{:?}", self.state),
            "user_id": self.metadata.user_id,
            "tenant_id": self.metadata.tenant_id,
            "uptime_secs": self.uptime().as_secs(),
            "idle_secs": self.idle_time().as_secs(),
        })
    }
}

/// WebSocket server manager
pub struct WebSocketServer {
    /// Active connections
    connections: Arc<std::sync::Mutex<std::collections::HashMap<Uuid, WebSocketConnection>>>,

    /// Connection manager
    connection_manager: Arc<ConnectionManager>,

    /// WebSocket configuration
    config: Arc<WebSocketConfig>,

    /// Optional metrics collector
    metrics: Option<Arc<SubscriptionMetrics>>,
}

impl WebSocketServer {
    /// Create new WebSocket server
    #[must_use] 
    pub fn new(connection_manager: Arc<ConnectionManager>, config: Arc<WebSocketConfig>) -> Self {
        Self {
            connections: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            connection_manager,
            config,
            metrics: None,
        }
    }

    /// Create WebSocket server with metrics
    #[must_use] 
    pub fn with_metrics(
        connection_manager: Arc<ConnectionManager>,
        config: Arc<WebSocketConfig>,
        metrics: Arc<SubscriptionMetrics>,
    ) -> Self {
        Self {
            connections: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            connection_manager,
            config,
            metrics: Some(metrics),
        }
    }

    /// Register new WebSocket connection
    pub fn register_connection(
        &self,
        user_id: Option<i64>,
        tenant_id: Option<i64>,
    ) -> Result<WebSocketConnection, SubscriptionError> {
        // Register with connection manager first
        let metadata = self
            .connection_manager
            .register_connection(user_id, tenant_id)?;

        let connection = WebSocketConnection::new(
            metadata,
            self.config.clone(),
            self.connection_manager.clone(),
        );

        // Store in local tracking
        self.connections
            .lock()
            .unwrap()
            .insert(connection.connection_id, connection.clone());

        // Record metrics
        if let Some(metrics) = &self.metrics {
            metrics.record_connection_created();
        }

        Ok(connection)
    }

    /// Unregister connection
    pub fn unregister_connection(&self, connection_id: Uuid) -> Result<(), SubscriptionError> {
        // Get connection before removing to record metrics
        let connection = self.connections.lock().unwrap().remove(&connection_id);

        // Unregister from connection manager
        self.connection_manager
            .unregister_connection(connection_id)?;

        // Record metrics
        if let Some(metrics) = &self.metrics {
            metrics.record_connection_closed();
            if let Some(conn) = connection {
                let uptime_secs = conn.uptime().as_secs_f64();
                metrics.record_connection_uptime(uptime_secs);
            }
        }

        Ok(())
    }

    /// Get connection by ID
    #[must_use] 
    pub fn get_connection(&self, connection_id: Uuid) -> Option<WebSocketConnection> {
        self.connections
            .lock()
            .unwrap()
            .get(&connection_id)
            .cloned()
    }

    /// Get all active connections count
    #[must_use] 
    pub fn active_connections_count(&self) -> usize {
        self.connections.lock().unwrap().len()
    }

    /// Get connections info as JSON
    #[must_use] 
    pub fn connections_info(&self) -> Value {
        let connections = self.connections.lock().unwrap();
        let info: Vec<Value> = connections.values().map(WebSocketConnection::as_json).collect();

        serde_json::json!({
            "total": connections.len(),
            "connections": info,
        })
    }

    /// Check for timed-out connections
    #[must_use] 
    pub fn check_timeouts(&self) -> Vec<Uuid> {
        let connections = self.connections.lock().unwrap();
        connections
            .iter()
            .filter(|(_, conn)| conn.init_timed_out())
            .map(|(id, _)| *id)
            .collect()
    }
}

impl Clone for WebSocketConnection {
    fn clone(&self) -> Self {
        Self {
            connection_id: self.connection_id,
            state: self.state,
            metadata: self.metadata.clone(),
            config: self.config.clone(),
            manager: self.manager.clone(),
            created_at: self.created_at,
            last_activity: self.last_activity,
            init_timeout: self.init_timeout,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subscriptions::SubscriptionLimits;

    #[test]
    fn test_websocket_connection_creation() {
        let manager = ConnectionManager::new(SubscriptionLimits::default());
        let conn_meta = manager.register_connection(Some(123), Some(456)).unwrap();

        let config = Arc::new(WebSocketConfig::default());
        let ws_conn = WebSocketConnection::new(conn_meta, config, Arc::new(manager));

        assert_eq!(ws_conn.state, ConnectionState::Waiting);
        assert!(ws_conn.is_alive());
    }

    #[test]
    fn test_connection_init_transition() {
        let manager = ConnectionManager::new(SubscriptionLimits::default());
        let conn_meta = manager.register_connection(Some(123), Some(456)).unwrap();

        let config = Arc::new(WebSocketConfig::default());
        let mut ws_conn = WebSocketConnection::new(conn_meta, config, Arc::new(manager));

        let init_msg = GraphQLMessage::connection_init(None);
        ws_conn.handle_message(init_msg).unwrap();

        assert_eq!(ws_conn.state, ConnectionState::Connected);
    }

    #[test]
    fn test_init_timeout_check() {
        let manager = ConnectionManager::new(SubscriptionLimits::default());
        let conn_meta = manager.register_connection(Some(123), Some(456)).unwrap();

        let config = Arc::new(WebSocketConfig {
            init_timeout: Duration::from_millis(1),
            ..Default::default()
        });
        let ws_conn = WebSocketConnection::new(conn_meta, config, Arc::new(manager));

        std::thread::sleep(Duration::from_millis(10));
        assert!(ws_conn.init_timed_out());
    }

    #[test]
    fn test_websocket_server_register() {
        let manager = Arc::new(ConnectionManager::new(SubscriptionLimits::default()));
        let config = Arc::new(WebSocketConfig::default());
        let server = WebSocketServer::new(manager, config);

        let result = server.register_connection(Some(123), Some(456));
        assert!(result.is_ok());
        assert_eq!(server.active_connections_count(), 1);
    }

    #[test]
    fn test_websocket_server_unregister() {
        let manager = Arc::new(ConnectionManager::new(SubscriptionLimits::default()));
        let config = Arc::new(WebSocketConfig::default());
        let server = WebSocketServer::new(manager, config);

        let conn = server.register_connection(Some(123), Some(456)).unwrap();
        assert_eq!(server.active_connections_count(), 1);

        server.unregister_connection(conn.connection_id).unwrap();
        assert_eq!(server.active_connections_count(), 0);
    }
}
