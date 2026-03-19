//! Token refresh scheduler and background worker.

use std::{sync::Arc, time::Duration as StdDuration};

use chrono::{DateTime, Duration, Utc};

use super::super::error::AuthError;

/// Token refresh scheduler
#[derive(Debug, Clone)]
pub struct TokenRefreshScheduler {
    /// Sessions needing refresh
    // std::sync::Mutex is intentional: this lock is never held across .await.
    // Switch to tokio::sync::Mutex if that constraint ever changes.
    refresh_queue: Arc<std::sync::Mutex<Vec<(String, DateTime<Utc>)>>>,
}

impl TokenRefreshScheduler {
    /// Create new refresh scheduler
    pub fn new() -> Self {
        Self {
            refresh_queue: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Schedule token refresh for session
    ///
    /// # Errors
    ///
    /// Returns `AuthError::Internal` if the mutex is poisoned.
    pub fn schedule_refresh(
        &self,
        session_id: String,
        refresh_time: DateTime<Utc>,
    ) -> std::result::Result<(), AuthError> {
        let mut queue = self.refresh_queue.lock().map_err(|_| AuthError::Internal {
            message: "token refresh scheduler mutex poisoned".to_string(),
        })?;
        queue.push((session_id, refresh_time));
        queue.sort_by_key(|(_, time)| *time);
        Ok(())
    }

    /// Get next session to refresh
    ///
    /// # Errors
    ///
    /// Returns `AuthError::Internal` if the mutex is poisoned.
    pub fn get_next_refresh(&self) -> std::result::Result<Option<String>, AuthError> {
        let mut queue = self.refresh_queue.lock().map_err(|_| AuthError::Internal {
            message: "token refresh scheduler mutex poisoned".to_string(),
        })?;
        if let Some((_, refresh_time)) = queue.first() {
            if *refresh_time <= Utc::now() {
                let (id, _) = queue.remove(0);
                return Ok(Some(id));
            }
        }
        Ok(None)
    }

    /// Cancel scheduled refresh
    ///
    /// # Errors
    ///
    /// Returns `AuthError::Internal` if the mutex is poisoned.
    pub fn cancel_refresh(&self, session_id: &str) -> std::result::Result<bool, AuthError> {
        let mut queue = self.refresh_queue.lock().map_err(|_| AuthError::Internal {
            message: "token refresh scheduler mutex poisoned".to_string(),
        })?;
        let len_before = queue.len();
        queue.retain(|(id, _)| id != session_id);
        Ok(queue.len() < len_before)
    }
}

impl Default for TokenRefreshScheduler {
    fn default() -> Self {
        Self::new()
    }
}

/// Callback trait for the token refresh worker to perform provider-specific
/// token refresh and session updates.
#[async_trait::async_trait]
pub trait TokenRefresher: Send + Sync {
    /// Refresh the token for the given session ID.
    ///
    /// Should look up the session, call the appropriate OAuth2 provider's
    /// `refresh_token()`, update the stored session, and return the new expiry.
    /// Returns `None` if the session no longer exists or has no refresh token.
    async fn refresh_session(
        &self,
        session_id: &str,
    ) -> std::result::Result<Option<DateTime<Utc>>, AuthError>;
}

/// Background worker that polls the `TokenRefreshScheduler` and refreshes
/// expiring OAuth tokens.
pub struct TokenRefreshWorker {
    scheduler:     Arc<TokenRefreshScheduler>,
    refresher:     Arc<dyn TokenRefresher>,
    cancel_rx:     tokio::sync::watch::Receiver<bool>,
    poll_interval: StdDuration,
}

impl TokenRefreshWorker {
    /// Create a new token refresh worker.
    ///
    /// Returns the worker and a sender to trigger cancellation (send `true` to
    /// stop).
    pub fn new(
        scheduler: Arc<TokenRefreshScheduler>,
        refresher: Arc<dyn TokenRefresher>,
        poll_interval: StdDuration,
    ) -> (Self, tokio::sync::watch::Sender<bool>) {
        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        (
            Self {
                scheduler,
                refresher,
                cancel_rx,
                poll_interval,
            },
            cancel_tx,
        )
    }

    /// Run the refresh loop until cancelled.
    pub async fn run(mut self) {
        tracing::info!(
            interval_secs = self.poll_interval.as_secs(),
            "Token refresh worker started"
        );
        loop {
            tokio::select! {
                result = self.cancel_rx.changed() => {
                    if result.is_err() || *self.cancel_rx.borrow() {
                        tracing::info!("Token refresh worker stopped");
                        break;
                    }
                },
                () = tokio::time::sleep(self.poll_interval) => {
                    self.process_due_refreshes().await;
                }
            }
        }
    }

    async fn process_due_refreshes(&self) {
        while let Ok(Some(session_id)) = self.scheduler.get_next_refresh() {
            match self.refresher.refresh_session(&session_id).await {
                Ok(Some(new_expiry)) => {
                    // Re-schedule at 80% of the remaining time
                    let remaining = new_expiry - Utc::now();
                    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                    // Reason: intentional 80% scaling via f64; sub-second precision loss is acceptable for a scheduling heuristic
                    let next_refresh_secs = (remaining.num_seconds() as f64 * 0.8) as i64;
                    let next_refresh = Utc::now() + Duration::seconds(next_refresh_secs);
                    if let Err(e) =
                        self.scheduler.schedule_refresh(session_id.clone(), next_refresh)
                    {
                        tracing::warn!(
                            session_id = %session_id,
                            error = %e,
                            "Failed to re-schedule token refresh"
                        );
                    }
                },
                Ok(None) => {
                    tracing::debug!(
                        session_id = %session_id,
                        "Session no longer exists, skipping refresh"
                    );
                },
                Err(e) => {
                    tracing::warn!(
                        session_id = %session_id,
                        error = %e,
                        "Token refresh failed"
                    );
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_scheduler_schedule_and_get_due_refresh() {
        let scheduler = TokenRefreshScheduler::new();
        // Schedule a refresh in the past (already due)
        let past = Utc::now() - Duration::seconds(10);
        scheduler
            .schedule_refresh("session_a".to_string(), past)
            .expect("schedule_refresh must succeed");

        let next = scheduler.get_next_refresh().expect("get_next_refresh must succeed");
        assert_eq!(next, Some("session_a".to_string()));
    }

    #[test]
    fn test_scheduler_future_refresh_not_returned() {
        let scheduler = TokenRefreshScheduler::new();
        // Schedule a refresh far in the future
        let future = Utc::now() + Duration::hours(1);
        scheduler
            .schedule_refresh("session_b".to_string(), future)
            .expect("schedule_refresh must succeed");

        let next = scheduler.get_next_refresh().expect("get_next_refresh must succeed");
        assert!(next.is_none(), "future refresh must not be returned as next");
    }

    #[test]
    fn test_scheduler_ordering_by_time() {
        let scheduler = TokenRefreshScheduler::new();
        let now = Utc::now();
        scheduler
            .schedule_refresh("later".to_string(), now - Duration::seconds(5))
            .expect("schedule must succeed");
        scheduler
            .schedule_refresh("earlier".to_string(), now - Duration::seconds(10))
            .expect("schedule must succeed");

        // The earliest due refresh should come first
        let first = scheduler.get_next_refresh().expect("must succeed");
        assert_eq!(first, Some("earlier".to_string()));
        let second = scheduler.get_next_refresh().expect("must succeed");
        assert_eq!(second, Some("later".to_string()));
    }

    #[test]
    fn test_scheduler_cancel_refresh() {
        let scheduler = TokenRefreshScheduler::new();
        let future = Utc::now() + Duration::hours(1);
        scheduler
            .schedule_refresh("session_c".to_string(), future)
            .expect("schedule must succeed");

        let cancelled = scheduler
            .cancel_refresh("session_c")
            .expect("cancel must succeed");
        assert!(cancelled, "cancel_refresh must return true for existing session");

        let cancelled_again = scheduler
            .cancel_refresh("session_c")
            .expect("cancel must succeed");
        assert!(!cancelled_again, "cancel_refresh must return false for already-removed session");
    }

    #[test]
    fn test_scheduler_cancel_nonexistent_returns_false() {
        let scheduler = TokenRefreshScheduler::new();
        let cancelled = scheduler
            .cancel_refresh("nonexistent")
            .expect("cancel must succeed");
        assert!(!cancelled);
    }

    #[test]
    fn test_scheduler_empty_returns_none() {
        let scheduler = TokenRefreshScheduler::new();
        let next = scheduler.get_next_refresh().expect("must succeed");
        assert!(next.is_none());
    }
}
