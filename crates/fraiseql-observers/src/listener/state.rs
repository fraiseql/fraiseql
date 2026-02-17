//! Listener state machine for managing listener lifecycle in multi-listener setup.
//!
//! Tracks listener states: Initializing → Connecting → Running → Recovering
//! Provides state transitions, duration tracking, and recovery management.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::Mutex;

use crate::error::{ObserverError, Result};

/// Listener lifecycle state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListenerState {
    /// Listener being initialized
    Initializing,
    /// Listener connecting to database
    Connecting,
    /// Listener actively processing events
    Running,
    /// Listener recovering from failure
    Recovering,
    /// Listener stopped
    Stopped,
}

impl std::fmt::Display for ListenerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Initializing => write!(f, "initializing"),
            Self::Connecting => write!(f, "connecting"),
            Self::Running => write!(f, "running"),
            Self::Recovering => write!(f, "recovering"),
            Self::Stopped => write!(f, "stopped"),
        }
    }
}

/// State machine for tracking listener lifecycle
#[derive(Clone)]
pub struct ListenerStateMachine {
    current_state:         Arc<Mutex<ListenerState>>,
    state_change_time:     Arc<Mutex<Instant>>,
    listener_id:           String,
    max_recovery_attempts: u32,
    recovery_attempts:     Arc<Mutex<u32>>,
}

impl ListenerStateMachine {
    /// Create a new listener state machine
    #[must_use]
    pub fn new(listener_id: String) -> Self {
        Self {
            current_state: Arc::new(Mutex::new(ListenerState::Initializing)),
            state_change_time: Arc::new(Mutex::new(Instant::now())),
            listener_id,
            max_recovery_attempts: 3,
            recovery_attempts: Arc::new(Mutex::new(0)),
        }
    }

    /// Create with custom max recovery attempts
    #[must_use]
    pub const fn with_max_recovery_attempts(mut self, max_attempts: u32) -> Self {
        self.max_recovery_attempts = max_attempts;
        self
    }

    /// Transition to a new state
    pub async fn transition(&self, next_state: ListenerState) -> Result<()> {
        let current = *self.current_state.lock().await;

        // Validate state transition
        if !self.is_valid_transition(current, next_state) {
            return Err(ObserverError::InvalidConfig {
                message: format!("Invalid state transition: {current} → {next_state}"),
            });
        }

        // Reset recovery attempts on successful transition to Running
        if next_state == ListenerState::Running {
            *self.recovery_attempts.lock().await = 0;
        }

        // Increment recovery attempts on Recovering state
        if next_state == ListenerState::Recovering {
            let mut attempts = self.recovery_attempts.lock().await;
            *attempts += 1;
            if *attempts > self.max_recovery_attempts {
                return Err(ObserverError::InvalidConfig {
                    message: "Max recovery attempts exceeded".to_string(),
                });
            }
        }

        *self.current_state.lock().await = next_state;
        *self.state_change_time.lock().await = Instant::now();

        Ok(())
    }

    /// Get current state
    pub async fn get_state(&self) -> ListenerState {
        *self.current_state.lock().await
    }

    /// Get duration in current state
    pub async fn get_state_duration(&self) -> Duration {
        self.state_change_time.lock().await.elapsed()
    }

    /// Get listener ID
    #[must_use]
    pub fn listener_id(&self) -> &str {
        &self.listener_id
    }

    /// Get recovery attempt count
    pub async fn get_recovery_attempts(&self) -> u32 {
        *self.recovery_attempts.lock().await
    }

    /// Check if recovery is possible
    pub async fn can_recover(&self) -> bool {
        let attempts = self.recovery_attempts.lock().await;
        *attempts < self.max_recovery_attempts
    }

    /// Validate state transition
    #[allow(clippy::unnested_or_patterns)]
    const fn is_valid_transition(&self, current: ListenerState, next: ListenerState) -> bool {
        matches!(
            (current, next),
            // Initial transitions
            (ListenerState::Initializing, ListenerState::Connecting)
                | (ListenerState::Initializing, ListenerState::Stopped)
            // Connection flow
            | (ListenerState::Connecting, ListenerState::Running)
                | (ListenerState::Connecting, ListenerState::Stopped)
            // Running to recovery or stopped
            | (ListenerState::Running, ListenerState::Recovering)
                | (ListenerState::Running, ListenerState::Stopped)
            // Recovery back to running or stopped
            | (ListenerState::Recovering, ListenerState::Running)
                | (ListenerState::Recovering, ListenerState::Stopped)
            // Stopped is final
            | (ListenerState::Stopped, ListenerState::Stopped)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_listener_state_creation() {
        let state_machine = ListenerStateMachine::new("listener-1".to_string());

        assert_eq!(state_machine.get_state().await, ListenerState::Initializing);
        assert_eq!(state_machine.get_recovery_attempts().await, 0);
    }

    #[tokio::test]
    async fn test_listener_state_transitions() {
        let state_machine = ListenerStateMachine::new("listener-1".to_string());

        // Valid transition: Initializing → Connecting
        assert!(state_machine.transition(ListenerState::Connecting).await.is_ok());
        assert_eq!(state_machine.get_state().await, ListenerState::Connecting);

        // Valid transition: Connecting → Running
        assert!(state_machine.transition(ListenerState::Running).await.is_ok());
        assert_eq!(state_machine.get_state().await, ListenerState::Running);

        // Valid transition: Running → Recovering
        assert!(state_machine.transition(ListenerState::Recovering).await.is_ok());
        assert_eq!(state_machine.get_state().await, ListenerState::Recovering);

        // Valid transition: Recovering → Running
        assert!(state_machine.transition(ListenerState::Running).await.is_ok());
        assert_eq!(state_machine.get_state().await, ListenerState::Running);
    }

    #[tokio::test]
    async fn test_listener_invalid_transitions() {
        let state_machine = ListenerStateMachine::new("listener-1".to_string());

        // Invalid transition: Initializing → Running (skip Connecting)
        assert!(state_machine.transition(ListenerState::Running).await.is_err());

        // Invalid transition: Initializing → Recovering
        assert!(state_machine.transition(ListenerState::Recovering).await.is_err());
    }

    #[tokio::test]
    async fn test_listener_state_duration_tracking() {
        let state_machine = ListenerStateMachine::new("listener-1".to_string());

        let initial_duration = state_machine.get_state_duration().await;
        assert!(initial_duration.as_millis() < 100);

        // Transition and wait
        state_machine.transition(ListenerState::Connecting).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        let connecting_duration = state_machine.get_state_duration().await;
        assert!(connecting_duration.as_millis() >= 50);
    }

    #[tokio::test]
    async fn test_listener_recovery_attempts() {
        let state_machine =
            ListenerStateMachine::new("listener-1".to_string()).with_max_recovery_attempts(3);

        state_machine.transition(ListenerState::Connecting).await.unwrap();
        state_machine.transition(ListenerState::Running).await.unwrap();

        // First recovery
        state_machine.transition(ListenerState::Recovering).await.unwrap();
        assert_eq!(state_machine.get_recovery_attempts().await, 1);
        assert!(state_machine.can_recover().await);

        state_machine.transition(ListenerState::Running).await.unwrap();
        assert_eq!(state_machine.get_recovery_attempts().await, 0); // Reset on success

        // Multiple recoveries
        for _ in 0..3 {
            state_machine.transition(ListenerState::Recovering).await.unwrap();
            state_machine.transition(ListenerState::Running).await.unwrap();
        }
    }

    #[test]
    fn test_listener_state_display() {
        assert_eq!(ListenerState::Initializing.to_string(), "initializing");
        assert_eq!(ListenerState::Connecting.to_string(), "connecting");
        assert_eq!(ListenerState::Running.to_string(), "running");
        assert_eq!(ListenerState::Recovering.to_string(), "recovering");
        assert_eq!(ListenerState::Stopped.to_string(), "stopped");
    }

    #[test]
    fn test_listener_id() {
        let state_machine = ListenerStateMachine::new("my-listener".to_string());
        assert_eq!(state_machine.listener_id(), "my-listener");
    }
}
