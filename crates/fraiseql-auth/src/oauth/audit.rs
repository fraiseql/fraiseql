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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_event_creation() {
        let event = OAuthAuditEvent::new("authorization", "auth0", "success");
        assert_eq!(event.event_type, "authorization");
        assert_eq!(event.provider, "auth0");
        assert_eq!(event.status, "success");
        assert!(event.user_id.is_none());
        assert!(event.error.is_none());
        assert!(event.metadata.is_empty());
    }

    #[test]
    fn test_audit_event_with_user_id() {
        let event = OAuthAuditEvent::new("token_exchange", "google", "success")
            .with_user_id("user_456".to_string());
        assert_eq!(event.user_id, Some("user_456".to_string()));
    }

    #[test]
    fn test_audit_event_with_error() {
        let event = OAuthAuditEvent::new("token_exchange", "auth0", "failed")
            .with_error("Provider unreachable".to_string());
        assert_eq!(event.error, Some("Provider unreachable".to_string()));
    }

    #[test]
    fn test_audit_event_with_metadata() {
        let event = OAuthAuditEvent::new("authorization", "auth0", "success")
            .with_metadata("ip".to_string(), "10.0.0.1".to_string())
            .with_metadata("user_agent".to_string(), "TestClient/1.0".to_string());
        assert_eq!(event.metadata.len(), 2);
        assert_eq!(event.metadata.get("ip"), Some(&"10.0.0.1".to_string()));
    }

    #[test]
    fn test_audit_event_serializes_to_json() {
        let event = OAuthAuditEvent::new("logout", "okta", "success");
        let json = serde_json::to_string(&event).expect("audit event must serialize");
        assert!(json.contains("\"event_type\":\"logout\""));
        assert!(json.contains("\"provider\":\"okta\""));
    }
}
