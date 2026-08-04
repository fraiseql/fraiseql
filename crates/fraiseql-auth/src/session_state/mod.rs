//! Durable per-thread conversation / session state (#389).
//!
//! Agents and multi-turn applications backed by FraiseQL need short-term
//! working memory — intermediate reasoning, accumulated context, thread
//! summaries — with deterministic retention, without bolting on a separate KV
//! store. This module is that store:
//!
//! - [`SessionState`] — the policy layer every caller goes through: per-entry TTL stamping, the
//!   [`MAX_SESSION_VALUE_BYTES`] size cap, and the [`Summarizer`] collapse at a configurable
//!   per-thread threshold.
//! - [`SessionStateBackend`] — `memory` (tests / local dev, volatile) or `postgres`
//!   (`_system.session_state`, created by [`PostgresSessionStateStore::init`] at startup like
//!   `_system.sessions`). A configured `postgres` backend that is unreachable at boot must refuse
//!   to boot — never downgrade to in-memory (the P21 backend rule).
//!
//! Isolation is application-layer, exactly like `_system.sessions`: the
//! `session_id` comes from the authenticated context (an MCP session, a user
//! session), never from a client-named field.

mod memory;
mod postgres;
mod store;
mod summarizer;

use std::sync::Arc;

use chrono::{Duration as ChronoDuration, Utc};
pub use memory::InMemorySessionStateStore;
pub use postgres::PostgresSessionStateStore;
pub use store::{MAX_SESSION_VALUE_BYTES, SUMMARY_KEY, SessionStateEntry};
pub use summarizer::{NoOpSummarizer, SummarizeFuture, Summarizer};
use uuid::Uuid;

use crate::error::{AuthError, Result};

#[cfg(test)]
mod tests;

/// The configured storage backend.
///
/// An enum rather than a trait object so the hot path stays static-dispatch
/// and no new `#[async_trait]` is introduced (the ratchet).
pub enum SessionStateBackend {
    /// Volatile in-process storage (tests, local development).
    InMemory(InMemorySessionStateStore),
    /// Durable storage in `_system.session_state`.
    Postgres(PostgresSessionStateStore),
}

impl std::fmt::Debug for SessionStateBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InMemory(_) => f.write_str("SessionStateBackend::InMemory"),
            Self::Postgres(_) => f.write_str("SessionStateBackend::Postgres"),
        }
    }
}

impl SessionStateBackend {
    async fn get(
        &self,
        session_id: Uuid,
        thread_id: &str,
        key: &str,
    ) -> Result<Option<SessionStateEntry>> {
        match self {
            Self::InMemory(s) => s.get(session_id, thread_id, key),
            Self::Postgres(s) => s.get(session_id, thread_id, key).await,
        }
    }

    async fn set(&self, entry: &SessionStateEntry) -> Result<()> {
        match self {
            Self::InMemory(s) => s.set(entry.clone()),
            Self::Postgres(s) => s.set(entry).await,
        }
    }

    async fn delete(&self, session_id: Uuid, thread_id: &str, key: &str) -> Result<()> {
        match self {
            Self::InMemory(s) => s.delete(session_id, thread_id, key),
            Self::Postgres(s) => s.delete(session_id, thread_id, key).await,
        }
    }

    async fn list_thread(
        &self,
        session_id: Uuid,
        thread_id: &str,
    ) -> Result<Vec<SessionStateEntry>> {
        match self {
            Self::InMemory(s) => s.list_thread(session_id, thread_id),
            Self::Postgres(s) => s.list_thread(session_id, thread_id).await,
        }
    }

    async fn expire_thread(&self, session_id: Uuid, thread_id: &str) -> Result<()> {
        match self {
            Self::InMemory(s) => s.expire_thread(session_id, thread_id),
            Self::Postgres(s) => s.expire_thread(session_id, thread_id).await,
        }
    }

    async fn thread_count(
        &self,
        session_id: Uuid,
        thread_id: &str,
        exclude_key: &str,
    ) -> Result<usize> {
        match self {
            Self::InMemory(s) => s.thread_count(session_id, thread_id, exclude_key),
            Self::Postgres(s) => s.thread_count(session_id, thread_id, exclude_key).await,
        }
    }

    async fn replace_thread(&self, summary: &SessionStateEntry) -> Result<()> {
        match self {
            Self::InMemory(s) => s.replace_thread(summary.clone()),
            Self::Postgres(s) => s.replace_thread(summary).await,
        }
    }

    /// Remove every expired entry; returns how many were evicted.
    ///
    /// Public because the server's background sweep calls it directly.
    ///
    /// # Errors
    ///
    /// Returns the backend's storage error.
    pub async fn evict_expired(&self) -> Result<u64> {
        match self {
            Self::InMemory(s) => s.evict_expired(),
            Self::Postgres(s) => s.evict_expired().await,
        }
    }
}

/// The session-state subsystem: one backend plus the shared policy layer.
///
/// Every write goes through [`set`](Self::set), which stamps the TTL, enforces
/// the size cap, and fires the summarisation collapse — so the two backends
/// cannot drift on policy.
pub struct SessionState {
    backend:         SessionStateBackend,
    default_ttl:     ChronoDuration,
    /// Collapse a thread once its ordinary-entry count exceeds this.
    summarize_after: Option<usize>,
    summarizer:      Option<Arc<dyn Summarizer>>,
}

impl std::fmt::Debug for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionState")
            .field("backend", &self.backend)
            .field("default_ttl", &self.default_ttl)
            .field("summarize_after", &self.summarize_after)
            .field("summarizer_installed", &self.summarizer.is_some())
            .finish()
    }
}

impl SessionState {
    /// Create the subsystem over `backend` with the given per-entry TTL.
    ///
    /// `default_ttl_secs` is clamped to at least 1 second — a zero TTL would
    /// make every write instantly invisible, which can only be a
    /// misconfiguration.
    #[must_use]
    pub fn new(backend: SessionStateBackend, default_ttl_secs: u64) -> Self {
        let secs = i64::try_from(default_ttl_secs.max(1)).unwrap_or(i64::MAX);
        Self {
            backend,
            default_ttl: ChronoDuration::seconds(secs),
            summarize_after: None,
            summarizer: None,
        }
    }

    /// Install the summarisation hook: collapse a thread into a single
    /// [`SUMMARY_KEY`] entry once it holds more than `after_entries` ordinary
    /// entries. Without this, threads grow until their entries expire.
    #[must_use]
    pub fn with_summarizer(
        mut self,
        summarizer: Arc<dyn Summarizer>,
        after_entries: usize,
    ) -> Self {
        self.summarizer = Some(summarizer);
        self.summarize_after = Some(after_entries.max(1));
        self
    }

    /// Fetch one live entry.
    ///
    /// # Errors
    ///
    /// Returns the backend's storage error.
    pub async fn get(
        &self,
        session_id: Uuid,
        thread_id: &str,
        key: &str,
    ) -> Result<Option<SessionStateEntry>> {
        self.backend.get(session_id, thread_id, key).await
    }

    /// Write one entry (TTL-stamped from the configured default) and fire the
    /// summarisation collapse if the thread just crossed the threshold.
    ///
    /// # Errors
    ///
    /// - [`AuthError::SessionError`] when `value` serializes beyond [`MAX_SESSION_VALUE_BYTES`], or
    ///   when `key` is the reserved [`SUMMARY_KEY`] (summaries are written only by the collapse
    ///   itself).
    /// - The backend's storage error, or the [`Summarizer`]'s error (the thread is left intact in
    ///   that case).
    pub async fn set(
        &self,
        session_id: Uuid,
        thread_id: &str,
        key: &str,
        value: serde_json::Value,
    ) -> Result<()> {
        if key == SUMMARY_KEY {
            return Err(AuthError::SessionError {
                message: format!(
                    "session-state key '{SUMMARY_KEY}' is reserved for the summarisation collapse"
                ),
            });
        }
        let serialized_len = value.to_string().len();
        if serialized_len > MAX_SESSION_VALUE_BYTES {
            return Err(AuthError::SessionError {
                message: format!(
                    "session-state value for '{key}' is {serialized_len} bytes serialized — the \
                     cap is {MAX_SESSION_VALUE_BYTES}. Store working state, not blobs."
                ),
            });
        }

        let now = Utc::now();
        self.backend
            .set(&SessionStateEntry {
                session_id,
                thread_id: thread_id.to_string(),
                key: key.to_string(),
                value,
                updated_at: now,
                expires_at: now + self.default_ttl,
            })
            .await?;

        self.maybe_summarize(session_id, thread_id).await
    }

    /// Remove one entry (idempotent).
    ///
    /// # Errors
    ///
    /// Returns the backend's storage error.
    pub async fn delete(&self, session_id: Uuid, thread_id: &str, key: &str) -> Result<()> {
        self.backend.delete(session_id, thread_id, key).await
    }

    /// Every live entry of a thread, oldest write first.
    ///
    /// # Errors
    ///
    /// Returns the backend's storage error.
    pub async fn list_thread(
        &self,
        session_id: Uuid,
        thread_id: &str,
    ) -> Result<Vec<SessionStateEntry>> {
        self.backend.list_thread(session_id, thread_id).await
    }

    /// Drop a whole thread.
    ///
    /// # Errors
    ///
    /// Returns the backend's storage error.
    pub async fn expire_thread(&self, session_id: Uuid, thread_id: &str) -> Result<()> {
        self.backend.expire_thread(session_id, thread_id).await
    }

    /// Remove every expired entry across all sessions; returns the count.
    ///
    /// # Errors
    ///
    /// Returns the backend's storage error.
    pub async fn evict_expired(&self) -> Result<u64> {
        self.backend.evict_expired().await
    }

    async fn maybe_summarize(&self, session_id: Uuid, thread_id: &str) -> Result<()> {
        let (Some(summarizer), Some(after)) = (self.summarizer.as_ref(), self.summarize_after)
        else {
            return Ok(());
        };
        if self.backend.thread_count(session_id, thread_id, SUMMARY_KEY).await? <= after {
            return Ok(());
        }

        let entries = self.backend.list_thread(session_id, thread_id).await?;
        let summary_value = summarizer.summarize(entries).await?;
        let now = Utc::now();
        self.backend
            .replace_thread(&SessionStateEntry {
                session_id,
                thread_id: thread_id.to_string(),
                key: SUMMARY_KEY.to_string(),
                value: summary_value,
                updated_at: now,
                expires_at: now + self.default_ttl,
            })
            .await
    }
}
