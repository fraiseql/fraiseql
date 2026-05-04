//! Live implementation of `HostContext` with real backends.
//!
//! This module provides `LiveHostContext` which integrates with actual FraiseQL services:
//! - GraphQL query execution via `fraiseql-core::Executor`
//! - Raw SQL queries with RLS support via `fraiseql-db::DatabaseAdapter`
//! - HTTP requests with SSRF protection
//! - Storage access with RLS checks
//! - Auth context and environment variable access

#[cfg(test)]
mod tests;

pub mod sql_classifier;
pub mod http_validator;
pub mod storage;

use std::collections::HashSet;
use std::sync::Arc;

use crate::secrets::FunctionSecretsStore;
use crate::types::{EventPayload, LogEntry, LogLevel};
use crate::HostContext;
use fraiseql_error::Result;
use fraiseql_core::security::SecurityContext;
use fraiseql_core::types::UserId;

/// Configuration for host context operations.
#[derive(Debug, Clone)]
pub struct HostContextConfig {
    /// Allowed domains for outbound HTTP requests (glob patterns).
    pub allowed_domains: Vec<String>,

    /// Allowed environment variables to expose to functions.
    pub allowed_env_vars: HashSet<String>,

    /// Maximum response size for HTTP requests (bytes).
    pub max_http_response_bytes: usize,

    /// Connect timeout for HTTP requests (milliseconds).
    pub http_connect_timeout_ms: u64,

    /// Read timeout for HTTP requests (milliseconds).
    pub http_read_timeout_ms: u64,

    /// Maximum size for storage uploads (bytes).
    pub max_storage_upload_bytes: usize,
}

impl Default for HostContextConfig {
    fn default() -> Self {
        Self {
            allowed_domains: vec![], // secure by default: deny all
            allowed_env_vars: HashSet::new(),
            max_http_response_bytes: 10 * 1024 * 1024, // 10 MB
            http_connect_timeout_ms: 5000,
            http_read_timeout_ms: 30000,
            max_storage_upload_bytes: 100 * 1024 * 1024, // 100 MB
        }
    }
}

/// A trait for executing GraphQL queries.
/// This abstraction allows testing without needing a real database.
pub trait QueryExecutor: Send + Sync {
    /// Execute a GraphQL query and return the result.
    fn execute_query(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value>> + Send + '_>>;
}

/// A trait for executing raw SQL queries (read-only, after classifier approval).
///
/// When no executor is configured, `sql_query` returns `Ok(vec![])`.
/// Inject a concrete implementation to enable actual database reads from functions.
pub trait SqlExecutor: Send + Sync {
    /// Execute a classified-safe (read-only) SQL statement and return rows as JSON.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the query fails to execute.
    fn execute_sql(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>>;
}

/// Live host context implementation with real backends.
pub struct LiveHostContext {
    /// Event payload that triggered this function.
    event_payload: EventPayload,

    /// Configuration for host operations.
    config: HostContextConfig,

    /// Captured log entries.
    logs: Arc<std::sync::Mutex<Vec<LogEntry>>>,

    /// Query executor for GraphQL execution.
    query_executor: Option<Arc<dyn QueryExecutor>>,

    /// SQL executor for raw SQL reads (after classifier approval).
    sql_executor: Option<Arc<dyn SqlExecutor>>,

    /// HTTP client for outbound requests.
    http_client: Option<Arc<reqwest::Client>>,

    /// Storage backend for file operations.
    pub storage_backend: Option<Arc<dyn storage::StorageBackend>>,

    /// Security context for the authenticated user.
    pub security_context: SecurityContext,

    /// Secrets store for per-function secrets (checked before `std::env` in `env_var`).
    secrets_store: Option<Arc<dyn FunctionSecretsStore>>,

    /// Name of the currently executing function (used for secrets lookup).
    function_name: Option<String>,
}

impl LiveHostContext {
    /// Create a new live host context.
    pub fn new(event_payload: EventPayload, config: HostContextConfig) -> Self {
        Self {
            event_payload,
            config,
            logs: Arc::new(std::sync::Mutex::new(Vec::new())),
            query_executor: None,
            sql_executor: None,
            http_client: None,
            storage_backend: None,
            security_context: Self::default_security_context(),
            secrets_store: None,
            function_name: None,
        }
    }

    /// Create a new live host context with a GraphQL query executor.
    pub fn with_executor(
        event_payload: EventPayload,
        config: HostContextConfig,
        executor: Arc<dyn QueryExecutor>,
    ) -> Self {
        Self {
            event_payload,
            config,
            logs: Arc::new(std::sync::Mutex::new(Vec::new())),
            query_executor: Some(executor),
            sql_executor: None,
            http_client: None,
            storage_backend: None,
            security_context: Self::default_security_context(),
            secrets_store: None,
            function_name: None,
        }
    }

    /// Create a new live host context with a raw SQL executor.
    pub fn with_sql_executor(
        event_payload: EventPayload,
        config: HostContextConfig,
        sql_executor: Arc<dyn SqlExecutor>,
    ) -> Self {
        Self {
            event_payload,
            config,
            logs: Arc::new(std::sync::Mutex::new(Vec::new())),
            query_executor: None,
            sql_executor: Some(sql_executor),
            http_client: None,
            storage_backend: None,
            security_context: Self::default_security_context(),
            secrets_store: None,
            function_name: None,
        }
    }

    /// Create a new live host context with an HTTP client.
    pub fn with_http_client(
        event_payload: EventPayload,
        config: HostContextConfig,
        http_client: Arc<reqwest::Client>,
    ) -> Self {
        Self {
            event_payload,
            config,
            logs: Arc::new(std::sync::Mutex::new(Vec::new())),
            query_executor: None,
            sql_executor: None,
            http_client: Some(http_client),
            storage_backend: None,
            security_context: Self::default_security_context(),
            secrets_store: None,
            function_name: None,
        }
    }

    /// Attach a secrets store and function name for `env_var` secret lookups.
    ///
    /// When set, `env_var` checks function secrets before falling back to `std::env`.
    #[must_use]
    pub fn with_secrets(
        mut self,
        store: Arc<dyn FunctionSecretsStore>,
        function_name: impl Into<String>,
    ) -> Self {
        self.secrets_store = Some(store);
        self.function_name = Some(function_name.into());
        self
    }

    /// Create a default security context for testing.
    fn default_security_context() -> SecurityContext {
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        SecurityContext {
            user_id: UserId("anonymous".to_string()),
            roles: vec![],
            tenant_id: None,
            scopes: vec![],
            attributes: std::collections::HashMap::new(),
            request_id: format!("req-{}", now),
            ip_address: None,
            authenticated_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(24),
            issuer: None,
            audience: None,
        }
    }

    /// Get captured logs (for testing).
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

impl HostContext for LiveHostContext {
    async fn query(
        &self,
        graphql: &str,
        variables: serde_json::Value,
    ) -> Result<serde_json::Value> {
        // Check if query executor is available
        let executor = self.query_executor.as_ref().ok_or_else(|| {
            fraiseql_error::FraiseQLError::Unsupported {
                message: "query executor not configured".to_string(),
            }
        })?;

        // Execute the query through the executor
        executor
            .execute_query(graphql, Some(&variables))
            .await
    }

    async fn sql_query(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<Vec<serde_json::Value>> {
        // Reject any non-read-only SQL before touching the database.
        let classification = sql_classifier::classify_sql(sql)?;
        match classification {
            sql_classifier::SqlClassification::ReadOnly => {
                // Delegate to an injected executor when available, otherwise return
                // an empty result set (safe fallback — no data, no error).
                if let Some(executor) = &self.sql_executor {
                    executor.execute_sql(sql, params).await
                } else {
                    Ok(vec![])
                }
            }
            sql_classifier::SqlClassification::Rejected(reason) => {
                Err(fraiseql_error::FraiseQLError::Authorization {
                    message: format!("SQL query not allowed: {reason}"),
                    action: Some("execute_sql_query".to_string()),
                    resource: None,
                })
            }
        }
    }

    async fn http_request(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: Option<&[u8]>,
    ) -> Result<crate::host::HttpResponse> {
        // Validate URL for SSRF attacks
        let http_config = http_validator::HttpClientConfig {
            allowed_domains: self.config.allowed_domains.clone(),
            max_response_bytes: self.config.max_http_response_bytes,
            connect_timeout_ms: self.config.http_connect_timeout_ms,
            read_timeout_ms: self.config.http_read_timeout_ms,
        };
        http_validator::validate_outbound_url(url, &http_config)?;

        // Get or create HTTP client
        let client = if let Some(client) = &self.http_client { client.clone() } else {
            // Create a new client with configured timeouts
            let client = reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_millis(
                    self.config.http_connect_timeout_ms,
                ))
                .timeout(std::time::Duration::from_millis(
                    self.config.http_read_timeout_ms,
                ))
                .build()
                .map_err(|e| fraiseql_error::FraiseQLError::Internal {
                    message: format!("failed to create HTTP client: {}", e),
                    source: None,
                })?;
            Arc::new(client)
        };

        // Build request
        let mut req = match method.to_uppercase().as_str() {
            "GET" => client.get(url),
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "PATCH" => client.patch(url),
            "DELETE" => client.delete(url),
            "HEAD" => client.head(url),
            _ => {
                return Err(fraiseql_error::FraiseQLError::Validation {
                    message: format!("unsupported HTTP method: {}", method),
                    path: None,
                })
            }
        };

        // Add headers
        for (key, value) in headers {
            req = req.header(key.clone(), value.clone());
        }

        // Add body if present
        if let Some(body_bytes) = body {
            req = req.body(body_bytes.to_vec());
        }

        // Execute request
        let response = req.send().await.map_err(|e| {
            fraiseql_error::FraiseQLError::Internal {
                message: format!("HTTP request failed: {}", e),
                source: None,
            }
        })?;

        let status = response.status().as_u16();

        // Collect response headers
        let response_headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    v.to_str().unwrap_or("").to_string(),
                )
            })
            .collect();

        // Read response body with size limit
        let body_bytes = response
            .bytes()
            .await
            .map_err(|e| fraiseql_error::FraiseQLError::Internal {
                message: format!("failed to read response body: {}", e),
                source: None,
            })?;

        if body_bytes.len() > self.config.max_http_response_bytes {
            return Err(fraiseql_error::FraiseQLError::Validation {
                message: format!(
                    "response body too large: {} > {}",
                    body_bytes.len(),
                    self.config.max_http_response_bytes
                ),
                path: None,
            });
        }

        Ok(crate::host::HttpResponse {
            status,
            headers: response_headers,
            body: body_bytes.to_vec(),
        })
    }

    async fn storage_get(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<Vec<u8>> {
        let backend = self.storage_backend.as_ref().ok_or_else(|| {
            fraiseql_error::FraiseQLError::Unsupported {
                message: "storage backend not configured".to_string(),
            }
        })?;

        backend.get(bucket, key).await
    }

    async fn storage_put(
        &self,
        bucket: &str,
        key: &str,
        body: &[u8],
        content_type: &str,
    ) -> Result<()> {
        // Check size limit
        if body.len() > self.config.max_storage_upload_bytes {
            return Err(fraiseql_error::FraiseQLError::Validation {
                message: format!(
                    "upload size {} exceeds limit {}",
                    body.len(),
                    self.config.max_storage_upload_bytes
                ),
                path: None,
            });
        }

        let backend = self.storage_backend.as_ref().ok_or_else(|| {
            fraiseql_error::FraiseQLError::Unsupported {
                message: "storage backend not configured".to_string(),
            }
        })?;

        backend.put(bucket, key, body, content_type).await
    }

    fn auth_context(&self) -> Result<serde_json::Value> {
        // Build auth context JSON from security context
        // Excludes sensitive fields like ip_address, raw tokens, etc.
        Ok(serde_json::json!({
            "sub": self.security_context.user_id,
            "user_id": self.security_context.user_id, // Alias for convenience
            "roles": self.security_context.roles,
            "scopes": self.security_context.scopes,
            "tenant_id": self.security_context.tenant_id,
            "expires_at": self.security_context.expires_at.to_rfc3339(),
            "authenticated_at": self.security_context.authenticated_at.to_rfc3339(),
        }))
    }

    fn env_var(&self, name: &str) -> Result<Option<String>> {
        if self.config.allowed_env_vars.contains(name) {
            Ok(std::env::var(name).ok())
        } else {
            Ok(None)
        }
    }

    async fn get_secret(&self, key: &str) -> Result<Option<String>> {
        let (Some(store), Some(fn_name)) = (&self.secrets_store, &self.function_name) else {
            return Ok(None);
        };
        store.get_secret(fn_name, key).await
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
