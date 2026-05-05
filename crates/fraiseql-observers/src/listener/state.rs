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
#[non_exhaustive]
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

/// All mutable state held under a single lock for atomic transitions.
struct Inner {
    state:             ListenerState,
    state_change_time: Instant,
    recovery_attempts: u32,
}

/// State machine for tracking listener lifecycle
#[derive(Clone)]
pub struct ListenerStateMachine {
    inner:                 Arc<Mutex<Inner>>,
    listener_id:           String,
    max_recovery_attempts: u32,
}

impl ListenerStateMachine {
    /// Create a new listener state machine
    #[must_use]
    pub fn new(listener_id: String) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                state:             ListenerState::Initializing,
                state_change_time: Instant::now(),
                recovery_attempts: 0,
            })),
            listener_id,
            max_recovery_attempts: 3,
        }
    }

    /// Create with custom max recovery attempts
    #[must_use]
    pub const fn with_max_recovery_attempts(mut self, max_attempts: u32) -> Self {
        self.max_recovery_attempts = max_attempts;
        self
    }

    /// Transition to a new state
    ///
    /// All state mutations happen under a single lock, making transitions atomic.
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::InvalidConfig`] if the transition is not permitted
    /// from the current state, or if the maximum recovery attempt count has been exceeded.
    pub async fn transition(&self, next_state: ListenerState) -> Result<()> {
        let mut inner = self.inner.lock().await;

        // Validate state transition
        if !Self::is_valid_transition(inner.state, next_state) {
            return Err(ObserverError::InvalidConfig {
                message: format!("Invalid state transition: {} → {next_state}", inner.state),
            });
        }

        // Reset recovery attempts on successful transition to Running
        if next_state == ListenerState::Running {
            inner.recovery_attempts = 0;
        }

        // Increment recovery attempts on Recovering state
        if next_state == ListenerState::Recovering {
            inner.recovery_attempts += 1;
            if inner.recovery_attempts > self.max_recovery_attempts {
                return Err(ObserverError::InvalidConfig {
                    message: "Max recovery attempts exceeded".to_string(),
                });
            }
        }

        inner.state = next_state;
        inner.state_change_time = Instant::now();

        Ok(())
    }

    /// Get current state
    pub async fn get_state(&self) -> ListenerState {
        self.inner.lock().await.state
    }

    /// Get duration in current state
    pub async fn get_state_duration(&self) -> Duration {
        self.inner.lock().await.state_change_time.elapsed()
    }

    /// Get listener ID
    #[must_use]
    pub fn listener_id(&self) -> &str {
        &self.listener_id
    }

    /// Get recovery attempt count
    pub async fn get_recovery_attempts(&self) -> u32 {
        self.inner.lock().await.recovery_attempts
    }

    /// Check if recovery is possible
    pub async fn can_recover(&self) -> bool {
        self.inner.lock().await.recovery_attempts < self.max_recovery_attempts
    }

    /// Validate state transition.
    ///
    /// Exposed as `pub` to enable property-based testing of the transition table
    /// in integration test files.
    #[must_use]
    #[allow(clippy::unnested_or_patterns)] // Reason: flat pattern list with comments is clearer for state machine transitions
    pub const fn is_valid_transition(current: ListenerState, next: ListenerState) -> bool {
        matches!(
            (current, next),
            // Initial transitions
            (ListenerState::Initializing, ListenerState::Connecting)
                | (ListenerState::Initializing, ListenerState::Stopped)
            // Connection flow — including Recovering so connection failures at startup
            // don't require a full process restart (L5 fix).
            | (ListenerState::Connecting, ListenerState::Running)
                | (ListenerState::Connecting, ListenerState::Recovering)
                | (ListenerState::Connecting, ListenerState::Stopped)
            // Running to recovery or stopped
            | (ListenerState::Running, ListenerState::Recovering)
                | (ListenerState::Running, ListenerState::Stopped)
            // Recovery back to running or stopped
            | (ListenerState::Recovering, ListenerState::Running)
                | (ListenerState::Recovering, ListenerState::Connecting)
                | (ListenerState::Recovering, ListenerState::Stopped)
            // Stopped is final
            | (ListenerState::Stopped, ListenerState::Stopped)
        )
    }
}

