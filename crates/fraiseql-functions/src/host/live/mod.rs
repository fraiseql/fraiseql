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

pub mod http_validator;
pub mod sql_classifier;
pub mod storage;

use std::{collections::HashSet, sync::Arc};

use fraiseql_core::security::SecurityContext;
use fraiseql_error::Result;

use crate::{
    HostContext,
    types::{EventPayload, LogEntry, LogLevel},
};

/// Configuration for host context operations.
#[derive(Debug, Clone)]
pub struct HostContextConfig {
    /// Allowed domains for outbound HTTP requests (glob patterns).
    ///
    /// Deny-by-default: empty (the default) permits no outbound host. A caller
    /// must explicitly allow each domain.
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
            // Deny-by-default (fail-closed): no outbound host is permitted until
            // the caller explicitly populates the allowlist.
            allowed_domains:          vec![],
            allowed_env_vars:         HashSet::new(),
            max_http_response_bytes:  10 * 1024 * 1024, // 10 MB
            http_connect_timeout_ms:  5000,
            http_read_timeout_ms:     30000,
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

    /// HTTP client for outbound requests.
    http_client: Option<Arc<reqwest::Client>>,

    /// Storage backend for file operations.
    pub storage_backend: Option<Arc<dyn storage::StorageBackend>>,

    /// Security context for the authenticated user.
    pub security_context: SecurityContext,

    /// Sender-identity resolver for `send_email` — resolves the host-owned `from`
    /// from the authenticated context. `None` → `send_email` is unconfigured and
    /// fails loud (mirrors the `sql_query` fail-loud-until-wired stance).
    sender_resolver: Option<Arc<dyn crate::outbound::SenderIdentityResolver>>,

    /// Email transport for `send_email`. `None` → `send_email` fails loud.
    email_transport: Option<Arc<dyn crate::outbound::EmailTransport>>,

    /// Per-dispatch idempotency token, injected by the durable dispatcher. `None`
    /// on non-dispatched paths (bare `invoke`). Stable across retries of the same
    /// dispatch and distinct per dispatch — see
    /// [`derive_idempotency_token`](fraiseql_observers::derive_idempotency_token).
    idempotency_token: Option<String>,

    /// Durable-cursor binding for a Model B scheduled source (#573). `None` on every
    /// non-source invocation, so `cursor()` / `advance_cursor()` are inert there.
    source_cursor: Option<SourceCursorBinding>,
}

/// Binds a [`LiveHostContext`] to exactly one source's durable cursor (Model B).
///
/// The guest can only read/advance *its own* source's cursor — the source name is
/// fixed by the host at construction, never supplied by the guest, so a source
/// cannot touch another's watermark.
struct SourceCursorBinding {
    /// The source this host is bound to (the cursor row + isolation key).
    source_name: String,
    /// The durable cursor store.
    store:       fraiseql_observers::PostgresSourceCursorStore,
}

impl LiveHostContext {
    /// Create a new live host context.
    #[must_use]
    pub fn new(event_payload: EventPayload, config: HostContextConfig) -> Self {
        Self {
            event_payload,
            config,
            logs: Arc::new(std::sync::Mutex::new(Vec::new())),
            query_executor: None,
            http_client: None,
            storage_backend: None,
            security_context: Self::default_security_context(),
            sender_resolver: None,
            email_transport: None,
            idempotency_token: None,
            source_cursor: None,
        }
    }

    /// Create a new live host context with a query executor.
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
            http_client: None,
            storage_backend: None,
            security_context: Self::default_security_context(),
            sender_resolver: None,
            email_transport: None,
            idempotency_token: None,
            source_cursor: None,
        }
    }

    /// Create a new live host context with an HTTP client.
    #[must_use]
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
            http_client: Some(http_client),
            storage_backend: None,
            security_context: Self::default_security_context(),
            sender_resolver: None,
            email_transport: None,
            idempotency_token: None,
            source_cursor: None,
        }
    }

    /// Create a default security context for testing.
    fn default_security_context() -> SecurityContext {
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();

        SecurityContext {
            user_id:          fraiseql_core::types::UserId("anonymous".to_string()),
            roles:            vec![],
            tenant_id:        None,
            scopes:           vec![],
            attributes:       std::collections::HashMap::new(),
            request_id:       format!("req-{}", now),
            ip_address:       None,
            authenticated_at: chrono::Utc::now(),
            expires_at:       chrono::Utc::now() + chrono::Duration::hours(24),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        }
    }

    /// Get captured logs (for testing).
    ///
    /// # Panics
    ///
    /// Panics if the Mutex is poisoned (should never happen in normal operation).
    #[must_use]
    pub fn captured_logs(&self) -> Vec<LogEntry> {
        self.logs.lock().expect("log mutex poisoned").clone()
    }

    /// Attach a sender-identity resolver and email transport, enabling
    /// [`send_email`](HostContext::send_email).
    ///
    /// The resolver produces the host-owned `from` from the authenticated context
    /// (the #539 seam — `LoginEmailSender` by default, a DB-backed resolver where
    /// the sending mailbox differs from the login email); the transport relays the
    /// message. Without both, `send_email` fails loud.
    #[must_use]
    pub fn with_email(
        mut self,
        sender_resolver: Arc<dyn crate::outbound::SenderIdentityResolver>,
        email_transport: Arc<dyn crate::outbound::EmailTransport>,
    ) -> Self {
        self.sender_resolver = Some(sender_resolver);
        self.email_transport = Some(email_transport);
        self
    }

    /// Attach the per-dispatch idempotency token surfaced to the guest via
    /// [`idempotency_token`](HostContext::idempotency_token).
    ///
    /// The durable dispatcher derives the token once (from the dispatch's stable
    /// identity) and sets it on the fresh host it builds for every retry attempt,
    /// so the guest observes the same token on each attempt.
    #[must_use]
    pub fn with_idempotency_token(mut self, token: impl Into<String>) -> Self {
        self.idempotency_token = Some(token.into());
        self
    }

    /// Bind this host to a Model B source's durable cursor (#573). `source_name`
    /// fixes which cursor row the guest's `fraiseql_cursor_get` /
    /// `fraiseql_cursor_advance` ops read and write; the guest cannot name another.
    #[must_use]
    pub fn with_source_cursor(
        mut self,
        source_name: impl Into<String>,
        store: fraiseql_observers::PostgresSourceCursorStore,
    ) -> Self {
        self.source_cursor = Some(SourceCursorBinding {
            source_name: source_name.into(),
            store,
        });
        self
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
        executor.execute_query(graphql, Some(&variables)).await
    }

    async fn sql_query(
        &self,
        sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<Vec<serde_json::Value>> {
        // Classify the SQL statement first
        let classification = sql_classifier::classify_sql(sql)?;
        match classification {
            sql_classifier::SqlClassification::ReadOnly => {
                // The query passed read-only classification, but execution is not
                // wired. Returning `Ok(vec![])` (M-sql-query-stub) made a valid
                // SELECT look like it ran and returned zero rows; fail loud instead.
                Err(fraiseql_error::FraiseQLError::Unsupported {
                    message: "sql_query host function is not implemented: the statement \
                              was accepted as read-only but no execution backend is wired"
                        .to_string(),
                })
            },
            sql_classifier::SqlClassification::Rejected(reason) => {
                Err(fraiseql_error::FraiseQLError::Authorization {
                    message:  format!("SQL query not allowed: {}", reason),
                    action:   Some("execute_sql_query".to_string()),
                    resource: None,
                })
            },
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
            allowed_domains:    self.config.allowed_domains.clone(),
            max_response_bytes: self.config.max_http_response_bytes,
            connect_timeout_ms: self.config.http_connect_timeout_ms,
            read_timeout_ms:    self.config.http_read_timeout_ms,
        };
        // SSRF validation: allowlist + literal-IP + DNS-rebinding checks. Async
        // because it resolves the host before any network contact.
        http_validator::validate_outbound_url(url, &http_config).await?;

        // Get or create HTTP client
        let client = if let Some(client) = &self.http_client {
            client.clone()
        } else {
            // Create a new client with configured timeouts.
            // Redirects are disabled (`Policy::none()`) so a 3xx response cannot
            // bounce the request to an un-validated internal target, bypassing
            // the SSRF guard that was applied only to the initial URL.
            let client = reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(std::time::Duration::from_millis(
                    self.config.http_connect_timeout_ms,
                ))
                .timeout(std::time::Duration::from_millis(self.config.http_read_timeout_ms))
                .build()
                .map_err(|e| fraiseql_error::FraiseQLError::Internal {
                    message: format!("failed to create HTTP client: {}", e),
                    source:  None,
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
                    path:    None,
                });
            },
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
        let response = req.send().await.map_err(|e| fraiseql_error::FraiseQLError::Internal {
            message: format!("HTTP request failed: {}", e),
            source:  None,
        })?;

        let status = response.status().as_u16();

        // Collect response headers
        let response_headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();

        // Read response body with size limit
        let body_bytes =
            response.bytes().await.map_err(|e| fraiseql_error::FraiseQLError::Internal {
                message: format!("failed to read response body: {}", e),
                source:  None,
            })?;

        if body_bytes.len() > self.config.max_http_response_bytes {
            return Err(fraiseql_error::FraiseQLError::Validation {
                message: format!(
                    "response body too large: {} > {}",
                    body_bytes.len(),
                    self.config.max_http_response_bytes
                ),
                path:    None,
            });
        }

        Ok(crate::host::HttpResponse {
            status,
            headers: response_headers,
            body: body_bytes.to_vec(),
        })
    }

    async fn storage_get(&self, bucket: &str, key: &str) -> Result<Vec<u8>> {
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
                path:    None,
            });
        }

        let backend = self.storage_backend.as_ref().ok_or_else(|| {
            fraiseql_error::FraiseQLError::Unsupported {
                message: "storage backend not configured".to_string(),
            }
        })?;

        backend.put(bucket, key, body, content_type).await
    }

    async fn send_email(
        &self,
        request: &crate::outbound::SendEmailRequest,
    ) -> Result<crate::outbound::SendEmailResponse> {
        // Fail loud, not silent, when the op is unconfigured — mirror `sql_query`.
        let resolver = self.sender_resolver.as_ref().ok_or_else(|| {
            fraiseql_error::FraiseQLError::Unsupported {
                message: "send_email is not configured: no sender-identity resolver is wired \
                          (configure a mailbox with an SMTP send half)"
                    .to_string(),
            }
        })?;
        let transport = self.email_transport.as_ref().ok_or_else(|| {
            fraiseql_error::FraiseQLError::Unsupported {
                message: "send_email is not configured: no email transport is wired (configure a \
                          mailbox with an SMTP send half)"
                    .to_string(),
            }
        })?;

        // The `from` is host-owned: resolve it from the authenticated context, not
        // from the guest request. A refusal is fail-closed and never falls back to
        // a shared mailbox. Map the refusal's permanence onto the error status
        // durable dispatch classifies by: permanent → 403 (dead-letter), transient
        // → 503 (retry).
        let auth = self.auth_context()?;
        let sender = resolver.resolve_sender(&auth).await.map_err(|error| {
            if error.retryable {
                fraiseql_error::FraiseQLError::ServiceUnavailable {
                    message:     error.message,
                    retry_after: None,
                }
            } else {
                fraiseql_error::FraiseQLError::Authorization {
                    message:  error.message,
                    action:   Some("send_email".to_string()),
                    resource: None,
                }
            }
        })?;

        // The per-dispatch context: the send-id is the host idempotency token (the
        // VERP correlation key + exactly-once dedup key); the tenant scopes the
        // send-status / suppression rows. Both are host-owned, never guest input.
        let context = crate::outbound::SendContext {
            send_id: self.idempotency_token.as_deref(),
            tenant:  self.security_context.tenant_id.as_ref().map(|tenant| tenant.as_str()),
        };
        transport.send(&sender, request, context).await
    }

    fn auth_context(&self) -> Result<serde_json::Value> {
        // Build auth context JSON from security context
        // Excludes sensitive fields like ip_address, raw tokens, etc.
        //
        // `email` / `display_name` are the connected user's verified identity —
        // the per-user sending address a paired outbound email must use (see
        // `crate::outbound::resolve_sender_identity`). They are `null` when the
        // authenticated identity carries none.
        Ok(serde_json::json!({
            "sub": self.security_context.user_id,
            "user_id": self.security_context.user_id, // Alias for convenience
            "roles": self.security_context.roles,
            "scopes": self.security_context.scopes,
            "tenant_id": self.security_context.tenant_id,
            "email": self.security_context.email,
            "display_name": self.security_context.display_name,
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

    fn event_payload(&self) -> &EventPayload {
        &self.event_payload
    }

    fn log(&self, level: LogLevel, message: &str) {
        let entry = LogEntry {
            level,
            message: message.to_string(),
            timestamp: chrono::Utc::now(),
        };
        self.logs.lock().expect("log mutex poisoned").push(entry);
    }

    fn idempotency_token(&self) -> Option<String> {
        self.idempotency_token.clone()
    }

    async fn cursor(&self) -> Result<Option<serde_json::Value>> {
        use fraiseql_observers::SourceCursorStore;
        match &self.source_cursor {
            Some(binding) => {
                let snapshot =
                    binding.store.load(&binding.source_name).await.map_err(|error| {
                        fraiseql_error::FraiseQLError::database(error.to_string())
                    })?;
                Ok(snapshot.value)
            },
            None => Ok(None),
        }
    }

    async fn advance_cursor(&self, value: serde_json::Value) -> Result<()> {
        use fraiseql_observers::SourceCursorStore;
        let Some(binding) = &self.source_cursor else {
            return Err(fraiseql_error::FraiseQLError::validation(
                "advance_cursor: this function is not a scheduled source (no cursor binding)",
            ));
        };
        // Model B advances after its writes commit; load the current snapshot for the
        // compare-and-swap `from` version, then advance. Under single-firing there is
        // no concurrent writer, so a failed CAS means the source overlapped itself.
        let snapshot = binding
            .store
            .load(&binding.source_name)
            .await
            .map_err(|error| fraiseql_error::FraiseQLError::database(error.to_string()))?;
        let applied = binding
            .store
            .advance(&binding.source_name, &snapshot, value)
            .await
            .map_err(|error| fraiseql_error::FraiseQLError::database(error.to_string()))?;
        if applied {
            Ok(())
        } else {
            Err(fraiseql_error::FraiseQLError::database(
                "advance_cursor: the cursor moved concurrently (lost the compare-and-swap)",
            ))
        }
    }
}
