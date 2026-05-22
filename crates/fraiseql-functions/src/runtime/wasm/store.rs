//! Store data and host import implementations for WASM component execution.
//!
//! This module defines the `StoreData` struct which holds all per-invocation state
//! and implements the host import traits that allow WASM components to call back
//! into the host for logging, context, and I/O operations.

use crate::types::{EventPayload, LogEntry, LogLevel, ResourceLimits};

/// Per-invocation state for WASM component execution.
///
/// This struct is attached to the wasmtime `Store` as user data and implements
/// the host import traits for `fraiseql:host/*` interfaces.
pub struct StoreData {
    /// The event that triggered this function invocation.
    pub event_payload: EventPayload,

    /// Reference to the host context for I/O and auth operations.
    pub host_context: Option<std::sync::Arc<dyn std::any::Any + Send + Sync>>,

    /// Logs captured during execution.
    pub logs: Vec<LogEntry>,

    /// Resource limits for this invocation.
    pub limits: ResourceLimits,

    /// Peak memory usage observed during execution (in bytes).
    pub memory_peak_bytes: u64,

    /// Current memory usage (for tracking).
    pub memory_current_bytes: u64,
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
        }
    }

    /// Set the host context reference for this store.
    pub fn set_host_context<C>(&mut self, context: std::sync::Arc<C>)
    where
        C: std::any::Any + Send + Sync + 'static,
    {
        self.host_context = Some(context);
    }

    /// Log a message at the specified level.
    ///
    /// Respects the `max_log_entries` limit and silently drops excess logs.
    pub fn log(&mut self, level: LogLevel, message: &str) {
        if self.logs.len() < self.limits.max_log_entries {
            let entry = LogEntry {
                level,
                message: message.to_string(),
                timestamp: chrono::Utc::now(),
            };
            self.logs.push(entry);

            // Emit tracing event at the appropriate level
            match level {
                LogLevel::Debug => tracing::debug!("{}", message),
                LogLevel::Info => tracing::info!("{}", message),
                LogLevel::Warn => tracing::warn!("{}", message),
                LogLevel::Error => tracing::error!("{}", message),
            }
        }
    }

    /// Get the event payload as a JSON string.
    ///
    /// # Errors
    ///
    /// Returns `Err` if serialization fails (should not happen for valid `EventPayload`).
    pub fn get_event_payload_json(&self) -> wasmtime::Result<String> {
        serde_json::to_string(&self.event_payload).map_err(|e| wasmtime::Error::msg(e.to_string()))
    }

    /// Get the auth context (if available) as JSON or an error string.
    ///
    /// # Implementation Note
    ///
    /// Stub implementation. Auth context extraction requires the `LiveHostContext` bridge
    /// to be wired. Always returns an error until then.
    ///
    /// # Errors
    ///
    /// Always returns an error since auth context is not yet available.
    pub fn get_auth_context_json(&self) -> wasmtime::Result<String> {
        Err(wasmtime::Error::msg("auth context not available"))
    }

    /// Get an environment variable value.
    ///
    /// # Implementation Note
    ///
    /// Stub implementation. Environment variable access requires the `LiveHostContext`
    /// bridge to be wired. Always returns `None` until then.
    ///
    /// # Errors
    ///
    /// Never returns an error.
    #[allow(clippy::missing_const_for_fn)] // Reason: returns Result with generic type
    pub fn get_env_var_value(&self, _name: &str) -> wasmtime::Result<Option<String>> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests;
