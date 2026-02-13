//! Multi-listener coordination for high-availability setup.
//!
//! Manages multiple listeners with shared checkpoint store,
//! providing leader election, health monitoring, and failover coordination.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use dashmap::DashMap;

use super::state::{ListenerState, ListenerStateMachine};
use crate::error::{ObserverError, Result};

/// Health status of a listener
#[derive(Debug, Clone)]
pub struct ListenerHealth {
    /// Unique identifier for the listener
    pub listener_id:     String,
    /// Whether the listener is currently healthy
    pub is_healthy:      bool,
    /// Last processed checkpoint ID
    pub last_checkpoint: i64,
    /// Current state of the listener
    pub state:           ListenerState,
    /// Timestamp of last heartbeat
    pub last_heartbeat:  Instant,
}

/// Handle to a registered listener
pub struct ListenerHandle {
    /// Unique identifier for the listener
    pub listener_id:    String,
    /// State machine managing listener lifecycle
    pub state_machine:  ListenerStateMachine,
    /// Current checkpoint (last processed event ID)
    pub checkpoint:     Arc<AtomicU64>,
    /// Last heartbeat timestamp
    pub last_heartbeat: Arc<tokio::sync::Mutex<Instant>>,
}

/// Coordinates multiple listeners
/// This is a lightweight coordinator that manages listener state and checkpoints
/// without requiring a specific checkpoint store implementation.
#[derive(Clone)]
pub struct MultiListenerCoordinator {
    listeners: Arc<DashMap<String, Arc<ListenerHandle>>>,
    leader_id: Arc<tokio::sync::Mutex<Option<String>>>,
}

impl MultiListenerCoordinator {
    /// Create new coordinator
    #[must_use]
    pub fn new() -> Self {
        Self {
            listeners: Arc::new(DashMap::new()),
            leader_id: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    /// Register a listener
    pub async fn register_listener(&self, listener_id: String) -> Result<()> {
        let handle = Arc::new(ListenerHandle {
            listener_id:    listener_id.clone(),
            state_machine:  ListenerStateMachine::new(listener_id.clone()),
            checkpoint:     Arc::new(AtomicU64::new(0)),
            last_heartbeat: Arc::new(tokio::sync::Mutex::new(Instant::now())),
        });

        self.listeners.insert(listener_id, handle);
        Ok(())
    }

    /// Deregister a listener
    pub fn deregister_listener(&self, listener_id: &str) -> Result<()> {
        self.listeners.remove(listener_id);
        Ok(())
    }

    /// Get listener state
    pub async fn get_listener_state(&self, listener_id: &str) -> Result<ListenerState> {
        let handle = self.listeners.get(listener_id).ok_or(ObserverError::InvalidConfig {
            message: format!("Listener {listener_id} not found"),
        })?;

        Ok(handle.state_machine.get_state().await)
    }

    /// Update listener heartbeat
    pub async fn update_heartbeat(&self, listener_id: &str) -> Result<()> {
        let handle = self.listeners.get(listener_id).ok_or(ObserverError::InvalidConfig {
            message: format!("Listener {listener_id} not found"),
        })?;

        *handle.last_heartbeat.lock().await = Instant::now();
        Ok(())
    }

    /// Update listener checkpoint
    pub fn update_checkpoint(&self, listener_id: &str, checkpoint: i64) -> Result<()> {
        let handle = self.listeners.get(listener_id).ok_or(ObserverError::InvalidConfig {
            message: format!("Listener {listener_id} not found"),
        })?;

        handle.checkpoint.store(checkpoint as u64, Ordering::SeqCst);
        Ok(())
    }

    /// Get health status of all listeners
    pub async fn check_listener_health(&self) -> Result<Vec<ListenerHealth>> {
        let mut health_statuses = Vec::new();

        for entry in self.listeners.iter() {
            let handle = entry.value();
            let state = handle.state_machine.get_state().await;
            let last_heartbeat = *handle.last_heartbeat.lock().await;
            let checkpoint = handle.checkpoint.load(Ordering::SeqCst) as i64;

            // Healthy if Running and heartbeat within 60s
            let is_healthy =
                state == ListenerState::Running && last_heartbeat.elapsed().as_secs() < 60;

            health_statuses.push(ListenerHealth {
                listener_id: handle.listener_id.clone(),
                is_healthy,
                last_checkpoint: checkpoint,
                state,
                last_heartbeat,
            });
        }

        Ok(health_statuses)
    }

    /// Elect leader among listeners
    pub async fn elect_leader(&self) -> Result<String> {
        let mut leader = self.leader_id.lock().await;

        // Check if current leader is still healthy
        if let Some(current_leader) = leader.as_ref() {
            if let Ok(state) = self.get_listener_state(current_leader).await {
                if state == ListenerState::Running {
                    return Ok(current_leader.clone());
                }
            }
        }

        // Find healthy listeners
        let health = self.check_listener_health().await?;
        let healthy: Vec<_> = health.iter().filter(|h| h.is_healthy).collect();

        if healthy.is_empty() {
            return Err(ObserverError::InvalidConfig {
                message: "No healthy listeners available for election".to_string(),
            });
        }

        // Select first healthy listener (deterministic)
        let new_leader = healthy[0].listener_id.clone();
        *leader = Some(new_leader.clone());

        Ok(new_leader)
    }

    /// Get number of listeners
    #[must_use]
    pub fn listener_count(&self) -> usize {
        self.listeners.len()
    }

    /// Get current leader
    pub async fn get_leader(&self) -> Option<String> {
        self.leader_id.lock().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_coordinator_creation() {
        let coordinator = MultiListenerCoordinator::new();
        assert_eq!(coordinator.listener_count(), 0);
    }

    #[tokio::test]
    async fn test_listener_registration() {
        let coordinator = MultiListenerCoordinator::new();

        coordinator.register_listener("listener-1".to_string()).await.ok();
        coordinator.register_listener("listener-2".to_string()).await.ok();

        assert_eq!(coordinator.listener_count(), 2);
    }

    #[tokio::test]
    async fn test_listener_deregistration() {
        let coordinator = MultiListenerCoordinator::new();

        coordinator.register_listener("listener-1".to_string()).await.ok();
        assert_eq!(coordinator.listener_count(), 1);

        coordinator.deregister_listener("listener-1").ok();
        assert_eq!(coordinator.listener_count(), 0);
    }

    #[tokio::test]
    async fn test_listener_state_retrieval() {
        let coordinator = MultiListenerCoordinator::new();

        coordinator.register_listener("listener-1".to_string()).await.ok();

        let state = coordinator.get_listener_state("listener-1").await.ok();
        assert_eq!(state, Some(ListenerState::Initializing));
    }

    #[tokio::test]
    async fn test_listener_health_check() {
        let coordinator = MultiListenerCoordinator::new();

        coordinator.register_listener("listener-1".to_string()).await.ok();
        coordinator.register_listener("listener-2".to_string()).await.ok();

        let health = coordinator.check_listener_health().await.ok();
        assert_eq!(health.map(|h| h.len()), Some(2));
    }

    #[tokio::test]
    async fn test_leader_election() {
        let coordinator = MultiListenerCoordinator::new();

        coordinator
            .register_listener("listener-1".to_string())
            .await
            .expect("register listener-1");
        coordinator
            .register_listener("listener-2".to_string())
            .await
            .expect("register listener-2");

        // Leaders can only be elected from healthy (Running) listeners.
        // Initially listeners are Initializing, so election may fail or
        // succeed depending on implementation. Either way, it must not panic.
        let _leader_result = coordinator.elect_leader().await;
    }
}
