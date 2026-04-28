//! Core types for function execution.

use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::time::Duration;

/// Supported runtime types for serverless functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RuntimeType {
    /// `WebAssembly` Component Model runtime.
    Wasm,
    /// Deno (`JavaScript`/`TypeScript` via V8) runtime.
    Deno,
}

impl RuntimeType {
    /// Get supported file extensions for this runtime.
    pub const fn supported_extensions(&self) -> &[&str] {
        match self {
            RuntimeType::Wasm => &[".wasm"],
            RuntimeType::Deno => &[".js", ".ts", ".mjs", ".mts"],
        }
    }
}

/// A compiled function module ready for execution.
#[derive(Debug, Clone)]
pub struct FunctionModule {
    /// Unique name for this function.
    pub name: String,
    /// Hash of the module source (for caching).
    pub source_hash: String,
    /// Compiled bytecode or source text.
    pub bytecode: bytes::Bytes,
    /// Which runtime executes this module.
    pub runtime: RuntimeType,
}

impl FunctionModule {
    /// Create a new WASM module from compiled bytecode.
    pub fn from_bytecode(name: String, bytecode: bytes::Bytes) -> Self {
        let source_hash = format!("{:x}", sha2::Sha256::digest(&bytecode));
        Self {
            name,
            source_hash,
            bytecode,
            runtime: RuntimeType::Wasm,
        }
    }

    /// Create a new source-based module (JavaScript/TypeScript).
    pub fn from_source(
        name: String,
        source: String,
        runtime: RuntimeType,
    ) -> Self {
        let bytecode = bytes::Bytes::from(source);
        let source_hash = format!("{:x}", sha2::Sha256::digest(&bytecode));
        Self {
            name,
            source_hash,
            bytecode,
            runtime,
        }
    }
}

/// Trigger event payload for a function invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPayload {
    /// Type of trigger: "mutation", "subscription", "cron", "webhook", etc.
    pub trigger_type: String,
    /// Entity name (e.g., "User", "Post").
    pub entity: String,
    /// Event kind (e.g., "created", "updated", "deleted").
    pub event_kind: String,
    /// Event data (JSON).
    pub data: serde_json::Value,
    /// Timestamp when the event occurred.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Definition of a serverless function for deployment and execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// Unique name for this function.
    pub name: String,
    /// Trigger type and configuration (e.g., "after:mutation:createUser", "cron:0 * * * *", "http:GET:/users/:id").
    pub trigger: String,
    /// Which runtime executes this function.
    pub runtime: RuntimeType,
    /// Optional timeout in milliseconds (overrides defaults).
    /// - For `before:mutation` triggers: defaults to 500ms
    /// - For other triggers: defaults to 5s
    pub timeout_ms: Option<u64>,
}

impl FunctionDefinition {
    /// Create a new function definition.
    pub fn new(name: &str, trigger: &str, runtime: RuntimeType) -> Self {
        Self {
            name: name.to_string(),
            trigger: trigger.to_string(),
            runtime,
            timeout_ms: None,
        }
    }

    /// Set a custom timeout for this function.
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }

    /// Get the effective timeout for this function.
    pub fn effective_timeout(&self) -> Duration {
        match self.timeout_ms {
            Some(ms) => Duration::from_millis(ms),
            None => {
                // before:mutation defaults to 500ms; others default to 5s
                if self.trigger.starts_with("before:mutation") {
                    Duration::from_millis(500)
                } else {
                    Duration::from_secs(5)
                }
            }
        }
    }

    /// Check if this function is a before:mutation trigger.
    pub fn is_before_mutation(&self) -> bool {
        self.trigger.starts_with("before:mutation:")
    }

    /// Check if this function is an after:mutation trigger.
    pub fn is_after_mutation(&self) -> bool {
        self.trigger.starts_with("after:mutation:")
    }

    /// Check if this function is an after:storage trigger.
    pub fn is_after_storage(&self) -> bool {
        self.trigger.starts_with("after:storage:")
    }

    /// Check if this function is a cron trigger.
    pub fn is_cron(&self) -> bool {
        self.trigger.starts_with("cron:")
    }

    /// Check if this function is an HTTP trigger.
    pub fn is_http(&self) -> bool {
        self.trigger.starts_with("http:")
    }
}

/// Log level for structured logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum LogLevel {
    /// Debug level.
    Debug,
    /// Info level.
    Info,
    /// Warning level.
    Warn,
    /// Error level.
    Error,
}

/// A single log entry from function execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    /// Log level.
    pub level: LogLevel,
    /// Log message.
    pub message: String,
    /// When the log was written.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Result of a function invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionResult {
    /// Return value from the function (may be None if function returns void).
    pub value: Option<serde_json::Value>,
    /// All logs captured during execution.
    pub logs: Vec<LogEntry>,
    /// Total execution duration.
    pub duration: Duration,
    /// Peak memory usage in bytes.
    pub memory_peak_bytes: u64,
}

/// Resource limits for function execution.
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Maximum memory allocation in bytes.
    pub max_memory_bytes: u64,
    /// Maximum execution duration.
    pub max_duration: Duration,
    /// Maximum number of log entries to capture.
    pub max_log_entries: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: 128 * 1024 * 1024, // 128 MB
            max_duration: Duration::from_secs(5),  // 5 seconds
            max_log_entries: 10_000,
        }
    }
}
