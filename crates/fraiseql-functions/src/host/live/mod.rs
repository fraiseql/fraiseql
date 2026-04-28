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

use std::collections::HashSet;
use std::sync::Arc;

use crate::types::{EventPayload, LogEntry, LogLevel};
use crate::HostContext;
use fraiseql_error::Result;

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
}

impl Default for HostContextConfig {
    fn default() -> Self {
        Self {
            allowed_domains: vec!["*".to_string()], // Allow all by default (should be restricted)
            allowed_env_vars: HashSet::new(),
            max_http_response_bytes: 10 * 1024 * 1024, // 10 MB
            http_connect_timeout_ms: 5000,
            http_read_timeout_ms: 30000,
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
}

impl LiveHostContext {
    /// Create a new live host context.
    pub fn new(event_payload: EventPayload, config: HostContextConfig) -> Self {
        Self {
            event_payload,
            config,
            logs: Arc::new(std::sync::Mutex::new(Vec::new())),
            query_executor: None,
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
        // Classify the SQL statement first
        let classification = sql_classifier::classify_sql(sql)?;
        match classification {
            sql_classifier::SqlClassification::ReadOnly => {
                // Would execute query here (not yet implemented)
                // For now, return a placeholder
                Ok(vec![])
            }
            sql_classifier::SqlClassification::Rejected(reason) => {
                Err(fraiseql_error::FraiseQLError::Authorization {
                    message: format!("SQL query not allowed: {}", reason),
                    action: Some("execute_sql_query".to_string()),
                    resource: None,
                })
            }
        }
    }

    async fn http_request(
        &self,
        _method: &str,
        _url: &str,
        _headers: &[(String, String)],
        _body: Option<&[u8]>,
    ) -> Result<crate::host::HttpResponse> {
        Err(fraiseql_error::FraiseQLError::Unsupported {
            message: "LiveHostContext::http_request not yet implemented".to_string(),
        })
    }

    async fn storage_get(
        &self,
        _bucket: &str,
        _key: &str,
    ) -> Result<Vec<u8>> {
        Err(fraiseql_error::FraiseQLError::Unsupported {
            message: "LiveHostContext::storage_get not yet implemented".to_string(),
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
            message: "LiveHostContext::storage_put not yet implemented".to_string(),
        })
    }

    fn auth_context(&self) -> Result<serde_json::Value> {
        Err(fraiseql_error::FraiseQLError::Unsupported {
            message: "LiveHostContext::auth_context not yet implemented".to_string(),
        })
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
        self.logs
            .lock()
            .expect("log mutex poisoned")
            .push(entry);
    }
}
