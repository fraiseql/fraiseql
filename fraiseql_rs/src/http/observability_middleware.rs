//! HTTP observability middleware for request/response tracking
//!
//! This module provides context and helper functions for tracking GraphQL requests
//! through the HTTP layer, integrating with audit logging and metrics collection.

use crate::security::audit::{AuditEntry, AuditLevel, AuditLogger};
use axum::http::HeaderMap;
use chrono::Utc;
use std::time::Instant;
use uuid::Uuid;

/// HTTP observability context for tracking request lifecycle
///
/// Created at request start and updated throughout request processing
/// to capture timing, authentication, and security information.
#[derive(Debug, Clone)]
pub struct ObservabilityContext {
    /// Unique request identifier for distributed tracing
    pub request_id: Uuid,

    /// Request start time (for duration calculation)
    pub start_time: Instant,

    /// Client IP address (extracted from `ConnectInfo`)
    pub client_ip: String,

    /// Authenticated user ID (optional - None for anonymous requests)
    pub user_id: Option<i64>,

    /// GraphQL operation type: query, mutation, or subscription
    pub operation: String,
}

impl ObservabilityContext {
    /// Create a new observability context at request start
    ///
    /// # Arguments
    ///
    /// * `client_ip` - Client IP address from request
    /// * `operation` - GraphQL operation type
    #[must_use]
    pub fn new(client_ip: String, operation: String) -> Self {
        Self {
            request_id: Uuid::new_v4(),
            start_time: Instant::now(),
            client_ip,
            user_id: None,
            operation,
        }
    }

    /// Get elapsed duration since request start in milliseconds
    #[must_use]
    pub fn elapsed_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    /// Update `user_id` after authentication
    pub const fn set_user_id(&mut self, user_id: i64) {
        self.user_id = Some(user_id);
    }
}

/// Response status code classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseStatus {
    /// 200 OK - Successful execution
    Success,

    /// 400 Bad Request - Query validation failure
    ValidationError,

    /// 401 Unauthorized - Auth failure (invalid/expired token)
    AuthError,

    /// 403 Forbidden - CSRF or permission denied
    ForbiddenError,

    /// 429 Too Many Requests - Rate limit violation
    RateLimitError,

    /// 500 Internal Server Error - Unexpected error
    InternalError,
}

impl ResponseStatus {
    /// Convert to HTTP status code
    #[must_use]
    pub const fn status_code(&self) -> u16 {
        match self {
            Self::Success => 200,
            Self::ValidationError => 400,
            Self::AuthError => 401,
            Self::ForbiddenError => 403,
            Self::RateLimitError => 429,
            Self::InternalError => 500,
        }
    }
}

/// Create an audit entry from request context
///
/// Helper function to construct `AuditEntry` for logging to audit logger
#[must_use]
pub fn create_audit_entry(
    context: &ObservabilityContext,
    query: &str,
    variables: &serde_json::Value,
    headers: &HeaderMap,
    _status: ResponseStatus,
    error: Option<&str>,
) -> AuditEntry {
    let duration_ms = context.elapsed_ms() as i32;
    let level = if error.is_some() {
        AuditLevel::ERROR
    } else {
        AuditLevel::INFO
    };

    AuditEntry {
        id: None,
        timestamp: Utc::now(),
        level,
        user_id: context.user_id.unwrap_or(0), // 0 = anonymous
        tenant_id: extract_tenant_id(headers).unwrap_or(0),
        operation: context.operation.clone(),
        query: query.to_string(),
        variables: variables.clone(),
        ip_address: context.client_ip.clone(),
        user_agent: extract_user_agent(headers).unwrap_or_default(),
        error: error.map(std::string::ToString::to_string),
        duration_ms: Some(duration_ms),
    }
}

/// Log an audit entry asynchronously (non-blocking)
///
/// Spawns a background task to write to audit logger without blocking request
pub fn log_audit_entry_async(audit_logger: &AuditLogger, entry: AuditEntry) {
    let logger = audit_logger.clone();
    tokio::spawn(async move {
        if let Err(e) = logger.log(entry).await {
            eprintln!("Failed to write audit log: {e}");
        }
    });
}

/// Extract tenant ID from request headers
///
/// Looks for X-Tenant-ID header. Returns None if not present.
fn extract_tenant_id(headers: &HeaderMap) -> Option<i64> {
    headers
        .get("X-Tenant-ID")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.parse().ok())
}

/// Extract User-Agent from request headers
fn extract_user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get("User-Agent")
        .and_then(|h| h.to_str().ok())
        .map(std::string::ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_observability_context_creation() {
        let ctx = ObservabilityContext::new("192.168.1.1".to_string(), "query".to_string());

        assert_eq!(ctx.client_ip, "192.168.1.1");
        assert_eq!(ctx.operation, "query");
        assert!(ctx.user_id.is_none());
    }

    #[test]
    fn test_observability_context_set_user_id() {
        let mut ctx = ObservabilityContext::new("127.0.0.1".to_string(), "mutation".to_string());

        ctx.set_user_id(42);
        assert_eq!(ctx.user_id, Some(42));
    }

    #[test]
    fn test_observability_context_elapsed_ms() {
        let ctx = ObservabilityContext::new("127.0.0.1".to_string(), "query".to_string());
        std::thread::sleep(std::time::Duration::from_millis(10));

        let elapsed = ctx.elapsed_ms();
        assert!(elapsed >= 10, "Expected at least 10ms, got {elapsed}ms");
    }

    #[test]
    fn test_response_status_success_code() {
        assert_eq!(ResponseStatus::Success.status_code(), 200);
    }

    #[test]
    fn test_response_status_validation_error_code() {
        assert_eq!(ResponseStatus::ValidationError.status_code(), 400);
    }

    #[test]
    fn test_response_status_auth_error_code() {
        assert_eq!(ResponseStatus::AuthError.status_code(), 401);
    }

    #[test]
    fn test_response_status_forbidden_error_code() {
        assert_eq!(ResponseStatus::ForbiddenError.status_code(), 403);
    }

    #[test]
    fn test_response_status_rate_limit_error_code() {
        assert_eq!(ResponseStatus::RateLimitError.status_code(), 429);
    }

    #[test]
    fn test_response_status_internal_error_code() {
        assert_eq!(ResponseStatus::InternalError.status_code(), 500);
    }

    #[test]
    fn test_create_audit_entry_success() {
        let ctx = ObservabilityContext::new("192.168.1.1".to_string(), "query".to_string());
        let query = "{ user { id } }";
        let variables = serde_json::json!({});
        let headers = HeaderMap::new();

        let entry = create_audit_entry(
            &ctx,
            query,
            &variables,
            &headers,
            ResponseStatus::Success,
            None,
        );

        assert_eq!(entry.query, query);
        assert_eq!(entry.user_id, 0); // anonymous
        assert!(entry.error.is_none());
        assert_eq!(entry.level, AuditLevel::INFO);
    }

    #[test]
    fn test_create_audit_entry_error() {
        let ctx = ObservabilityContext::new("192.168.1.1".to_string(), "query".to_string());
        let query = "{ invalid }";
        let variables = serde_json::json!({});
        let headers = HeaderMap::new();
        let error_msg = "Parse error";

        let entry = create_audit_entry(
            &ctx,
            query,
            &variables,
            &headers,
            ResponseStatus::ValidationError,
            Some(error_msg),
        );

        assert_eq!(entry.error, Some(error_msg.to_string()));
        assert_eq!(entry.level, AuditLevel::ERROR);
    }

    #[test]
    fn test_create_audit_entry_with_user() {
        let mut ctx = ObservabilityContext::new("192.168.1.1".to_string(), "mutation".to_string());
        ctx.set_user_id(123);

        let query = "mutation { createUser { id } }";
        let variables = serde_json::json!({ "name": "Alice" });
        let headers = HeaderMap::new();

        let entry = create_audit_entry(
            &ctx,
            query,
            &variables,
            &headers,
            ResponseStatus::Success,
            None,
        );

        assert_eq!(entry.user_id, 123);
        assert_eq!(entry.operation, "mutation");
    }

    #[test]
    fn test_extract_tenant_id_from_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("X-Tenant-ID", "999".parse().unwrap());

        let tenant_id = extract_tenant_id(&headers);
        assert_eq!(tenant_id, Some(999));
    }

    #[test]
    fn test_extract_tenant_id_missing() {
        let headers = HeaderMap::new();
        let tenant_id = extract_tenant_id(&headers);
        assert_eq!(tenant_id, None);
    }

    #[test]
    fn test_extract_user_agent() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "User-Agent",
            "Mozilla/5.0 (X11; Linux x86_64)".parse().unwrap(),
        );

        let ua = extract_user_agent(&headers);
        assert_eq!(ua, Some("Mozilla/5.0 (X11; Linux x86_64)".to_string()));
    }

    #[test]
    fn test_extract_user_agent_missing() {
        let headers = HeaderMap::new();
        let ua = extract_user_agent(&headers);
        assert_eq!(ua, None);
    }
}
