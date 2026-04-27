//! Host context trait for function runtime access to FraiseQL services.

use crate::types::{EventPayload, LogLevel};
use fraiseql_error::Result;
use std::future::Future;

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
pub struct NoopHostContext {
    event_payload: EventPayload,
}

impl NoopHostContext {
    /// Create a new no-op host context for testing.
    pub const fn new(event_payload: EventPayload) -> Self {
        Self { event_payload }
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

    fn log(&self, _level: LogLevel, _message: &str) {
        // No-op
    }
}
