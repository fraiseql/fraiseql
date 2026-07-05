//! Store data and host import implementations for WASM component execution.
//!
//! This module defines the `StoreData` struct which holds all per-invocation state
//! and implements the host import traits that allow WASM components to call back
//! into the host for logging, context, and I/O operations.

use std::sync::Arc;

use super::{
    bindings::fraiseql::host::{context, io, logging},
    host_bridge::DynHostContext,
};
use crate::types::{EventPayload, LogEntry, LogLevel, ResourceLimits};

/// Per-invocation state for WASM component execution.
///
/// This struct is attached to the wasmtime `Store` as user data and implements
/// the host import traits for `fraiseql:host/*` interfaces.
pub struct StoreData {
    /// The event that triggered this function invocation.
    pub event_payload: EventPayload,

    /// Reference to the host context for I/O and auth operations.
    pub host_context: Option<Arc<dyn DynHostContext>>,

    /// Logs captured during execution.
    pub logs: Vec<LogEntry>,

    /// Resource limits for this invocation.
    pub limits: ResourceLimits,

    /// Peak memory usage observed during execution (in bytes).
    pub memory_peak_bytes: u64,

    /// Current memory usage (for tracking).
    pub memory_current_bytes: u64,

    /// WASI context for guests compiled with wasm32-wasip2.
    wasi_ctx: wasmtime_wasi::WasiCtx,

    /// WASI resource table for file/stream handles.
    wasi_table: wasmtime::component::ResourceTable,
}

impl StoreData {
    /// Create a new store data for an invocation.
    #[must_use]
    pub fn new(event_payload: EventPayload, limits: ResourceLimits) -> Self {
        Self {
            event_payload,
            host_context: None,
            logs: Vec::new(),
            limits,
            memory_peak_bytes: 0,
            memory_current_bytes: 0,
            wasi_ctx: wasmtime_wasi::WasiCtxBuilder::new().build(),
            wasi_table: wasmtime::component::ResourceTable::new(),
        }
    }

    /// Set the host context reference for this store.
    pub fn set_host_context(&mut self, context: Arc<dyn DynHostContext>) {
        self.host_context = Some(context);
    }

    /// Get a reference to the host context, or return an error string.
    fn require_host_context(&self) -> Result<&dyn DynHostContext, String> {
        self.host_context
            .as_deref()
            .ok_or_else(|| "host context not available".to_string())
    }

    /// Log a message at the specified level.
    ///
    /// Respects the `max_log_entries` limit and silently drops excess logs.
    pub fn log_message(&mut self, level: LogLevel, message: &str) {
        if self.logs.len() < self.limits.max_log_entries {
            let entry = LogEntry {
                level,
                message: message.to_string(),
                timestamp: chrono::Utc::now(),
            };
            self.logs.push(entry);

            match level {
                LogLevel::Debug => tracing::debug!("{}", message),
                LogLevel::Info => tracing::info!("{}", message),
                LogLevel::Warn => tracing::warn!("{}", message),
                LogLevel::Error => tracing::error!("{}", message),
            }
        }
    }

    /// Get a reference to the event payload.
    #[must_use]
    pub const fn event_payload_ref(&self) -> &EventPayload {
        &self.event_payload
    }

    /// Get the event payload as a JSON string.
    ///
    /// # Errors
    ///
    /// Returns `Err` if serialization fails (should not happen for valid `EventPayload`).
    pub fn get_event_payload_json(&self) -> wasmtime::Result<String> {
        serde_json::to_string(&self.event_payload).map_err(|e| wasmtime::Error::msg(e.to_string()))
    }
}

impl wasmtime_wasi::WasiView for StoreData {
    fn ctx(&mut self) -> wasmtime_wasi::WasiCtxView<'_> {
        wasmtime_wasi::WasiCtxView {
            ctx:   &mut self.wasi_ctx,
            table: &mut self.wasi_table,
        }
    }
}

/// Map WIT log-level to internal `LogLevel`.
const fn map_log_level(level: logging::LogLevel) -> LogLevel {
    match level {
        logging::LogLevel::Debug => LogLevel::Debug,
        logging::LogLevel::Info => LogLevel::Info,
        logging::LogLevel::Warn => LogLevel::Warn,
        logging::LogLevel::Error => LogLevel::Error,
    }
}

impl logging::Host for StoreData {
    async fn log(&mut self, level: logging::LogLevel, message: String) {
        self.log_message(map_log_level(level), &message);
    }
}

impl context::Host for StoreData {
    async fn get_event_payload(&mut self) -> String {
        self.get_event_payload_json()
            .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}",))
    }

    async fn get_auth_context(&mut self) -> Result<String, String> {
        let host = self.require_host_context()?;
        let value = host.auth_context().map_err(|e| e.to_string())?;
        serde_json::to_string(&value).map_err(|e| e.to_string())
    }

    async fn get_env_var(&mut self, name: String) -> Option<String> {
        let Ok(host) = self.require_host_context() else {
            return None;
        };
        host.env_var(&name).ok().flatten()
    }

    async fn get_idempotency_token(&mut self) -> Option<String> {
        let Ok(host) = self.require_host_context() else {
            return None;
        };
        host.idempotency_token()
    }
}

/// Stringify a host error for the WIT `result<_, string>` boundary, tagging a
/// client error (4xx) as permanent so the runtime dead-letters it immediately
/// instead of retrying (parity with the Deno op path).
#[allow(clippy::needless_pass_by_value)] // Reason: consumed via `.map_err` at each WIT call site
fn host_err_string(error: fraiseql_error::FraiseQLError) -> String {
    if error.is_client_error() {
        format!("{} {error}", crate::types::PERMANENT_ERROR_MARKER)
    } else {
        error.to_string()
    }
}

impl io::Host for StoreData {
    async fn query(&mut self, graphql: String, variables: String) -> Result<String, String> {
        let host = self.require_host_context()?;
        let vars: serde_json::Value =
            serde_json::from_str(&variables).map_err(|e| e.to_string())?;
        let result = host.query(&graphql, vars).await.map_err(host_err_string)?;
        serde_json::to_string(&result).map_err(|e| e.to_string())
    }

    async fn sql_query(&mut self, sql: String, params: String) -> Result<String, String> {
        let host = self.require_host_context()?;
        let params_vec: Vec<serde_json::Value> =
            serde_json::from_str(&params).map_err(|e| e.to_string())?;
        let result = host.sql_query(&sql, &params_vec).await.map_err(host_err_string)?;
        serde_json::to_string(&result).map_err(|e| e.to_string())
    }

    async fn http_request(
        &mut self,
        method: String,
        url: String,
        headers: Vec<(String, String)>,
        body: Option<Vec<u8>>,
    ) -> Result<io::HttpResponse, String> {
        let host = self.require_host_context()?;
        let response = host
            .http_request(&method, &url, &headers, body.as_deref())
            .await
            .map_err(host_err_string)?;
        Ok(io::HttpResponse {
            status:  response.status,
            headers: response.headers,
            body:    response.body,
        })
    }

    async fn storage_get(&mut self, bucket: String, key: String) -> Result<Vec<u8>, String> {
        let host = self.require_host_context()?;
        host.storage_get(&bucket, &key).await.map_err(host_err_string)
    }

    async fn storage_put(
        &mut self,
        bucket: String,
        key: String,
        body: Vec<u8>,
        content_type: String,
    ) -> Result<(), String> {
        let host = self.require_host_context()?;
        host.storage_put(&bucket, &key, &body, &content_type)
            .await
            .map_err(host_err_string)
    }

    async fn send_email(&mut self, request: String) -> Result<String, String> {
        let host = self.require_host_context()?;
        let req: crate::outbound::SendEmailRequest =
            serde_json::from_str(&request).map_err(|e| e.to_string())?;
        let response = host.send_email(&req).await.map_err(host_err_string)?;
        serde_json::to_string(&response).map_err(|e| e.to_string())
    }
}

impl wasmtime::ResourceLimiter for StoreData {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        self.memory_current_bytes = desired as u64;
        if self.memory_current_bytes > self.memory_peak_bytes {
            self.memory_peak_bytes = self.memory_current_bytes;
        }
        let max = self.limits.max_memory_bytes;
        if self.memory_current_bytes > max {
            return Err(wasmtime::Error::msg(format!(
                "Memory limit exceeded: {} > {max}",
                self.memory_current_bytes
            )));
        }
        Ok(true)
    }

    fn table_growing(
        &mut self,
        _current: usize,
        _desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
