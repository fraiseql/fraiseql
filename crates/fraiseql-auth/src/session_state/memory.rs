//! In-memory session-state backend, for tests and local development.

use std::{collections::HashMap, sync::RwLock};

use chrono::Utc;
use uuid::Uuid;

use crate::{
    error::{AuthError, Result},
    session_state::store::SessionStateEntry,
};

type Key = (Uuid, String, String);

/// In-memory [`SessionStateEntry`] store.
///
/// A single `RwLock<HashMap>` rather than a sharded map: the summary collapse
/// must replace a whole thread atomically, which needs one write lock over the
/// full map. Volatile by design — a restart loses all threads; production
/// deployments configure the `postgres` backend.
#[derive(Default)]
pub struct InMemorySessionStateStore {
    entries: RwLock<HashMap<Key, SessionStateEntry>>,
}

impl InMemorySessionStateStore {
    /// Create an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn lock_err() -> AuthError {
        AuthError::Internal {
            message: "session-state lock poisoned".to_string(),
        }
    }

    /// Fetch one live (non-expired) entry.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Internal`] if the lock is poisoned.
    pub fn get(
        &self,
        session_id: Uuid,
        thread_id: &str,
        key: &str,
    ) -> Result<Option<SessionStateEntry>> {
        let map = self.entries.read().map_err(|_| Self::lock_err())?;
        Ok(map
            .get(&(session_id, thread_id.to_string(), key.to_string()))
            .filter(|e| e.expires_at > Utc::now())
            .cloned())
    }

    /// Insert or overwrite an entry.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Internal`] if the lock is poisoned.
    pub fn set(&self, entry: SessionStateEntry) -> Result<()> {
        let mut map = self.entries.write().map_err(|_| Self::lock_err())?;
        map.insert((entry.session_id, entry.thread_id.clone(), entry.key.clone()), entry);
        Ok(())
    }

    /// Remove one entry (idempotent).
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Internal`] if the lock is poisoned.
    pub fn delete(&self, session_id: Uuid, thread_id: &str, key: &str) -> Result<()> {
        let mut map = self.entries.write().map_err(|_| Self::lock_err())?;
        map.remove(&(session_id, thread_id.to_string(), key.to_string()));
        Ok(())
    }

    /// Every live entry of a thread, oldest write first (ties by key).
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Internal`] if the lock is poisoned.
    pub fn list_thread(&self, session_id: Uuid, thread_id: &str) -> Result<Vec<SessionStateEntry>> {
        let map = self.entries.read().map_err(|_| Self::lock_err())?;
        let now = Utc::now();
        let mut entries: Vec<SessionStateEntry> = map
            .values()
            .filter(|e| {
                e.session_id == session_id && e.thread_id == thread_id && e.expires_at > now
            })
            .cloned()
            .collect();
        entries.sort_by(|a, b| a.updated_at.cmp(&b.updated_at).then_with(|| a.key.cmp(&b.key)));
        Ok(entries)
    }

    /// Remove every entry of a thread.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Internal`] if the lock is poisoned.
    pub fn expire_thread(&self, session_id: Uuid, thread_id: &str) -> Result<()> {
        let mut map = self.entries.write().map_err(|_| Self::lock_err())?;
        map.retain(|(sid, tid, _), _| !(*sid == session_id && tid == thread_id));
        Ok(())
    }

    /// Count a thread's live entries, excluding `exclude_key`.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Internal`] if the lock is poisoned.
    pub fn thread_count(
        &self,
        session_id: Uuid,
        thread_id: &str,
        exclude_key: &str,
    ) -> Result<usize> {
        let map = self.entries.read().map_err(|_| Self::lock_err())?;
        let now = Utc::now();
        Ok(map
            .values()
            .filter(|e| {
                e.session_id == session_id
                    && e.thread_id == thread_id
                    && e.expires_at > now
                    && e.key != exclude_key
            })
            .count())
    }

    /// Atomically replace a whole thread with the single `summary` entry.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Internal`] if the lock is poisoned.
    pub fn replace_thread(&self, summary: SessionStateEntry) -> Result<()> {
        let mut map = self.entries.write().map_err(|_| Self::lock_err())?;
        map.retain(|(sid, tid, _), _| !(*sid == summary.session_id && *tid == summary.thread_id));
        map.insert((summary.session_id, summary.thread_id.clone(), summary.key.clone()), summary);
        Ok(())
    }

    /// Remove every expired entry; returns how many were evicted.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Internal`] if the lock is poisoned.
    pub fn evict_expired(&self) -> Result<u64> {
        let mut map = self.entries.write().map_err(|_| Self::lock_err())?;
        let now = Utc::now();
        let before = map.len();
        map.retain(|_, e| e.expires_at > now);
        Ok((before - map.len()) as u64)
    }
}
