//! Connection state machine

use crate::{Result, WireError};

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConnectionState {
    /// Initial state (not connected)
    Initial,

    /// Startup sent, awaiting authentication request
    AwaitingAuth,

    /// Authentication in progress
    Authenticating,

    /// Idle (ready for query)
    Idle,

    /// Query in progress
    QueryInProgress,

    /// Reading query results
    ReadingResults,

    /// Closed
    Closed,
}

impl ConnectionState {
    /// Check if transition is valid
    pub const fn can_transition_to(&self, next: ConnectionState) -> bool {
        use ConnectionState::{
            Authenticating, AwaitingAuth, Closed, Idle, Initial, QueryInProgress, ReadingResults,
        };

        matches!(
            (self, next),
            (Initial, AwaitingAuth)
                | (AwaitingAuth, Authenticating)
                | (Authenticating | ReadingResults, Idle)
                | (Idle, QueryInProgress)
                | (QueryInProgress, ReadingResults)
                | (_, Closed)
        )
    }

    /// Transition to new state
    ///
    /// # Errors
    ///
    /// Returns [`WireError::InvalidState`] if the transition from the current state to `next`
    /// is not permitted by the state machine.
    pub fn transition(&mut self, next: ConnectionState) -> Result<()> {
        if !self.can_transition_to(next) {
            return Err(WireError::InvalidState {
                expected: format!("valid transition from {:?}", self),
                actual: format!("{:?}", next),
            });
        }
        *self = next;
        Ok(())
    }
}

impl std::fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Initial => write!(f, "initial"),
            Self::AwaitingAuth => write!(f, "awaiting_auth"),
            Self::Authenticating => write!(f, "authenticating"),
            Self::Idle => write!(f, "idle"),
            Self::QueryInProgress => write!(f, "query_in_progress"),
            Self::ReadingResults => write!(f, "reading_results"),
            Self::Closed => write!(f, "closed"),
        }
    }
}

#[cfg(test)]
mod tests;
