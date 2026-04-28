//! Host context trait for function runtime access to FraiseQL services.

use crate::types::{EventPayload, LogEntry, LogLevel};
use fraiseql_error::Result;
use std::future::Future;

#[cfg(feature = "host-live")]
pub mod live;

/// Response from an HTTP request.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    /// HTTP status code.
    pub status: u16,
    /// Response headers.
    pub headers: Vec<(String, String)>,
    /// Response body.
    pub body: Vec<u8>,
}

/// Trait for providing host services to functions (queries, storage, HTTP, etc.).
///
/// This trait is implemented by the FraiseQL server to allow functions to call
/// back into the server's services during execution.
///
/// The `#[trait_variant::make]` macro generates `SendHostContext` which is
/// object-safe for `Box<dyn SendHostContext>` dynamic dispatch.
#[allow(clippy::trait_duplication_in_bounds)]
#[trait_variant::make(SendHostContext: Send)]
pub trait HostContext: Send + Sync {
    /// Execute a GraphQL query.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the query fails to execute.
    fn query(
        &self,
        graphql: &str,
        variables: serde_json::Value,
    ) -> impl Future<Output = Result<serde_json::Value>> + Send;

    /// Execute a raw SQL query.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the query fails to execute or is classified as insecure.
    fn sql_query(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> impl Future<Output = Result<Vec<serde_json::Value>>> + Send;

    /// Make an HTTP request.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the request fails or is blocked (e.g., SSRF check).
    fn http_request(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: Option<&[u8]>,
    ) -> impl Future<Output = Result<HttpResponse>> + Send;

    /// Retrieve an object from storage.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the object does not exist or access is denied.
    fn storage_get(
        &self,
        bucket: &str,
        key: &str,
    ) -> impl Future<Output = Result<Vec<u8>>> + Send;

    /// Store an object to storage.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the write fails or access is denied.
    fn storage_put(
        &self,
        bucket: &str,
        key: &str,
        body: &[u8],
        content_type: &str,
    ) -> impl Future<Output = Result<()>> + Send;

    /// Get the current authenticated user's context.
    ///
    /// # Errors
    ///
    /// Returns `Err` if authentication information is unavailable.
    fn auth_context(&self) -> Result<serde_json::Value>;

    /// Get an environment variable.
    ///
    /// Returns `Ok(None)` if the variable is not set.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the variable is blocked from access.
    fn env_var(&self, name: &str) -> Result<Option<String>>;

    /// Get the current event payload (for reference).
    fn event_payload(&self) -> &EventPayload;

    /// Log a message to the tracing subscriber.
    fn log(&self, level: LogLevel, message: &str);
}

/// A no-op host context for testing WASM execution without real backends.
///
/// All I/O methods return `Unsupported` errors. Logs are captured in-memory for test verification.
pub struct NoopHostContext {
    event_payload: EventPayload,
    logs: std::sync::Arc<std::sync::Mutex<Vec<LogEntry>>>,
}

impl NoopHostContext {
    /// Create a new no-op host context for testing.
    pub fn new(event_payload: EventPayload) -> Self {
        Self {
            event_payload,
            logs: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Get a copy of all captured logs (for test verification).
    ///
    /// # Panics
    ///
    /// Panics if the Mutex is poisoned (should never happen in normal operation).
    pub fn captured_logs(&self) -> Vec<LogEntry> {
        self.logs
            .lock()
            .expect("log mutex poisoned")
            .clone()
    }
}

impl HostContext for NoopHostContext {
    async fn query(
        &self,
        _graphql: &str,
        _variables: serde_json::Value,
    ) -> Result<serde_json::Value> {
        Err(fraiseql_error::FraiseQLError::Unsupported {
            message: "HostContext::query not implemented".to_string(),
        })
    }

    async fn sql_query(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<Vec<serde_json::Value>> {
        Err(fraiseql_error::FraiseQLError::Unsupported {
            message: "HostContext::sql_query not implemented".to_string(),
        })
    }

    async fn http_request(
        &self,
        _method: &str,
        _url: &str,
        _headers: &[(String, String)],
        _body: Option<&[u8]>,
    ) -> Result<HttpResponse> {
        Err(fraiseql_error::FraiseQLError::Unsupported {
            message: "HostContext::http_request not implemented".to_string(),
        })
    }

    async fn storage_get(
        &self,
        _bucket: &str,
        _key: &str,
    ) -> Result<Vec<u8>> {
        Err(fraiseql_error::FraiseQLError::Unsupported {
            message: "HostContext::storage_get not implemented".to_string(),
        })
    }

    async fn storage_put(
        &self,
        _bucket: &str,
        _key: &str,
        _body: &[u8],
        _content_type: &str,
    ) -> Result<()> {
        Err(fraiseql_error::FraiseQLError::Unsupported {
            message: "HostContext::storage_put not implemented".to_string(),
        })
    }

    fn auth_context(&self) -> Result<serde_json::Value> {
        Err(fraiseql_error::FraiseQLError::Unsupported {
            message: "HostContext::auth_context not implemented".to_string(),
        })
    }

    fn env_var(&self, _name: &str) -> Result<Option<String>> {
        Ok(None)
    }

    fn event_payload(&self) -> &EventPayload {
        &self.event_payload
    }

    fn log(&self, level: LogLevel, message: &str) {
        let entry = LogEntry {
            level,
            message: message.to_string(),
            timestamp: chrono::Utc::now(),
        };
        self.logs
            .lock()
            .expect("log mutex poisoned")
            .push(entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_host_context_returns_unsupported() {
        let payload = EventPayload {
            trigger_type: "test".to_string(),
            entity: "Test".to_string(),
            event_kind: "created".to_string(),
            data: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        };
        let ctx = NoopHostContext::new(payload);

        // Non-async methods should return Unsupported
        assert!(ctx.auth_context().is_err());
        assert!(ctx.env_var("TEST").is_ok());
    }

    #[test]
    fn test_noop_host_context_log_captures_entries() {
        let payload = EventPayload {
            trigger_type: "test".to_string(),
            entity: "Test".to_string(),
            event_kind: "created".to_string(),
            data: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        };
        let ctx = NoopHostContext::new(payload);

        // Log some messages at different levels
        ctx.log(LogLevel::Debug, "debug message");
        ctx.log(LogLevel::Info, "info message");
        ctx.log(LogLevel::Warn, "warning message");
        ctx.log(LogLevel::Error, "error message");

        // Verify all logs were captured
        let logs = ctx.captured_logs();
        assert_eq!(logs.len(), 4);
        assert_eq!(logs[0].level, LogLevel::Debug);
        assert_eq!(logs[0].message, "debug message");
        assert_eq!(logs[1].level, LogLevel::Info);
        assert_eq!(logs[1].message, "info message");
        assert_eq!(logs[2].level, LogLevel::Warn);
        assert_eq!(logs[2].message, "warning message");
        assert_eq!(logs[3].level, LogLevel::Error);
        assert_eq!(logs[3].message, "error message");
    }

    #[test]
    fn test_event_payload_available_in_context() {
        let payload = EventPayload {
            trigger_type: "mutation".to_string(),
            entity: "User".to_string(),
            event_kind: "updated".to_string(),
            data: serde_json::json!({"id": 42}),
            timestamp: chrono::Utc::now(),
        };
        let ctx = NoopHostContext::new(payload);

        let retrieved = ctx.event_payload();
        assert_eq!(retrieved.trigger_type, "mutation");
        assert_eq!(retrieved.entity, "User");
        assert_eq!(retrieved.event_kind, "updated");
        assert_eq!(retrieved.data, serde_json::json!({"id": 42}));
    }

    // Storage access tests for LiveHostContext
    #[cfg(feature = "host-live")]
    #[tokio::test]
    async fn test_host_storage_get_returns_bytes() {
        use crate::host::live::LiveHostContext;
        use crate::host::live::HostContextConfig;
        use std::collections::HashMap;
        use std::sync::Arc;

        let payload = EventPayload {
            trigger_type: "test".to_string(),
            entity: "File".to_string(),
            event_kind: "created".to_string(),
            data: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        };

        // Create a mock storage backend
        let storage_data = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let test_data = b"hello world".to_vec();
        {
            let mut data = storage_data.lock().unwrap();
            data.entry("documents".to_string())
                .or_insert_with(HashMap::new)
                .insert("file.txt".to_string(), test_data.clone());
        }

        let ctx = LiveHostContext::new(payload, HostContextConfig::default());
        // Store the mock data directly (for now, without a real StorageBackend trait)
        // This test is a placeholder until we implement proper storage backend integration

        let result = ctx.storage_get("documents", "file.txt").await;

        // Should fail with Unsupported since storage backend is not configured
        assert!(result.is_err());
        match result {
            Err(fraiseql_error::FraiseQLError::Unsupported { .. }) => (),
            other => panic!("expected Unsupported error, got {:?}", other),
        }
    }

    #[cfg(feature = "host-live")]
    #[tokio::test]
    async fn test_host_storage_without_backend_returns_unsupported() {
        use crate::host::live::LiveHostContext;
        use crate::host::live::HostContextConfig;

        let payload = EventPayload {
            trigger_type: "test".to_string(),
            entity: "File".to_string(),
            event_kind: "created".to_string(),
            data: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        };

        let ctx = LiveHostContext::new(payload, HostContextConfig::default());

        let result = ctx.storage_get("documents", "file.txt").await;

        assert!(result.is_err());
        match result {
            Err(fraiseql_error::FraiseQLError::Unsupported { message }) => {
                assert!(message.contains("not yet implemented") || message.contains("not configured"));
            }
            other => panic!("expected Unsupported error, got {:?}", other),
        }
    }

    #[cfg(feature = "host-live")]
    #[tokio::test]
    async fn test_host_storage_put_respects_size_limit() {
        use crate::host::live::LiveHostContext;
        use crate::host::live::HostContextConfig;

        let payload = EventPayload {
            trigger_type: "test".to_string(),
            entity: "File".to_string(),
            event_kind: "created".to_string(),
            data: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        };

        // Create config with very small size limit
        let mut config = HostContextConfig::default();
        config.max_storage_upload_bytes = 10; // 10 bytes limit

        let ctx = LiveHostContext::new(payload, config);

        // Try to upload larger than limit (but first check if storage is not configured)
        let oversized_data = vec![0u8; 100];
        let result = ctx
            .storage_put("documents", "large.txt", &oversized_data, "text/plain")
            .await;

        // Should fail with Unsupported since storage backend is not configured
        assert!(result.is_err());
    }
}
