//! The session-state entry type and shared limits.

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Hard cap on a single entry's serialized JSON value, in bytes.
///
/// Conversation state is short-term working memory, not blob storage: an
/// oversized value is refused loudly ([`crate::error::AuthError::SessionError`])
/// rather than truncated or silently accepted. Enforced by
/// [`SessionState::set`](super::SessionState::set) before any backend write, so
/// both backends share one limit.
pub const MAX_SESSION_VALUE_BYTES: usize = 65_536;

/// The reserved key a thread's rolled-up summary is stored under.
///
/// When a [`Summarizer`](super::Summarizer) collapses a thread, every ordinary
/// entry is replaced by a single entry with this key. Ordinary writes may not
/// use it — [`SessionState::set`](super::SessionState::set) refuses it so a
/// caller cannot spoof or clobber a summary by hand.
pub const SUMMARY_KEY: &str = "_summary";

/// One durable key/value pair of per-thread conversation state.
///
/// The isolation boundary is the *caller-supplied* `session_id`: the server
/// derives it from the authenticated context (an MCP session, a user session),
/// never from a client-named field — the same application-layer keying
/// `_system.sessions` uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionStateEntry {
    /// The owning session (from the authenticated context).
    pub session_id: Uuid,
    /// The conversation thread within the session.
    pub thread_id:  String,
    /// Entry key, unique within `(session_id, thread_id)`.
    pub key:        String,
    /// The stored JSON value (≤ [`MAX_SESSION_VALUE_BYTES`] serialized).
    pub value:      serde_json::Value,
    /// Last write time.
    pub updated_at: DateTime<Utc>,
    /// Expiry: the entry is invisible to reads from this instant and removed by
    /// the eviction sweep.
    pub expires_at: DateTime<Utc>,
}
