//! OAuth audit event types for compliance logging.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// OAuth audit event for logging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthAuditEvent {
    /// Event type: authorization, token_exchange, token_refresh, logout
    pub event_type: String,
    /// Provider name
    pub provider:   String,
    /// User ID (if known)
    pub user_id:    Option<String>,
    /// Status: success, failed
    pub status:     String,
    /// Error message (if failed)
    pub error:      Option<String>,
    /// Timestamp
    pub timestamp:  DateTime<Utc>,
    /// Additional metadata
    pub metadata:   HashMap<String, String>,
}

impl OAuthAuditEvent {
    /// Create new audit event
    pub fn new(
        event_type: impl Into<String>,
        provider: impl Into<String>,
        status: impl Into<String>,
    ) -> Self {
        Self {
            event_type: event_type.into(),
            provider:   provider.into(),
            user_id:    None,
            status:     status.into(),
            error:      None,
            timestamp:  Utc::now(),
            metadata:   HashMap::new(),
        }
    }

    /// Set user ID
    pub fn with_user_id(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }

    /// Set error message
    pub fn with_error(mut self, error: String) -> Self {
        self.error = Some(error);
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}
