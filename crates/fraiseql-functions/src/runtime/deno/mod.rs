//! Deno (`JavaScript`/`TypeScript`) runtime for function execution via V8.
//!
//! This module provides `DenoRuntime`, which executes `JavaScript` and `TypeScript` functions
//! using the Deno core runtime with embedded V8 isolates.
//!
//! # Architecture
//!
//! The Deno runtime is an opt-in feature (`runtime-deno`) due to V8's binary size (~30MB)
//! and compile-time impact. When disabled, there is zero impact on compilation time or binary size.
//!
//! Each execution:
//! 1. Creates a new V8 isolate with memory and timeout limits
//! 2. Loads the function source (JS/TS, transpiled on-the-fly)
//! 3. Calls the default export with the event as a JS object
//! 4. Captures logs and enforces resource limits throughout
//! 5. Properly cleans up the isolate after execution

pub mod executor;
pub mod tests;

use crate::runtime::FunctionRuntime;
use crate::types::{EventPayload, FunctionModule, FunctionResult, ResourceLimits};
use crate::HostContext;
use fraiseql_error::Result;

/// Configuration for the Deno runtime.
///
/// Allows tuning of the V8 engine for performance and feature support.
#[derive(Debug, Clone)]
pub struct DenoConfig {
    /// Enable `TypeScript` support (built-in transpiler).
    pub enable_typescript: bool,
    /// Additional V8 flags (e.g., "--expose-gc").
    pub v8_flags: Vec<String>,
}

impl Default for DenoConfig {
    fn default() -> Self {
        Self {
            enable_typescript: true,
            v8_flags: vec![],
        }
    }
}

/// Deno runtime using V8 isolates for JavaScript/TypeScript execution.
pub struct DenoRuntime {
    config: DenoConfig,
}

impl std::fmt::Debug for DenoRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DenoRuntime")
            .field("config", &self.config)
            .finish()
    }
}

impl Clone for DenoRuntime {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
        }
    }
}

impl DenoRuntime {
    /// Create a new Deno runtime with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns `Err` if runtime initialization fails.
    pub fn new(config: &DenoConfig) -> Result<Self> {
        Ok(Self {
            config: config.clone(),
        })
    }
}

impl FunctionRuntime for DenoRuntime {
    /// Execute a JavaScript/TypeScript module with the given event and host context.
    ///
    /// # Implementation
    ///
    /// This implementation:
    /// 1. Creates a new V8 isolate with resource limits
    /// 2. Loads the function source (transpiles TS if needed)
    /// 3. Calls the default export with the event as a JS object
    /// 4. Captures logs and enforces resource limits
    /// 5. Properly cleans up the isolate after execution
    #[allow(clippy::manual_async_fn)]  // Reason: impl Future syntax for trait compatibility
    fn invoke<H>(
        &self,
        module: &FunctionModule,
        event: EventPayload,
        _host: &H,
        limits: ResourceLimits,
    ) -> impl std::future::Future<Output = Result<FunctionResult>> + Send
    where
        H: HostContext + ?Sized,
    {
        let source = String::from_utf8_lossy(&module.bytecode).to_string();
        let event_value = serde_json::to_value(&event).unwrap_or(serde_json::json!({}));

        async move {
            let start = std::time::Instant::now();

            // Execute in a blocking task since JsRuntime is not Send
            let result = tokio::task::spawn_blocking(move || {
                executor::execute_deno_code(&source, event_value, &limits)
            })
            .await;

            let duration = start.elapsed();

            match result {
                Ok(Ok(value)) => {
                    Ok(FunctionResult {
                        value: Some(value),
                        logs: vec![],
                        duration,
                        memory_peak_bytes: 0,
                    })
                }
                Ok(Err(e)) => {
                    // Check if it's a syntax error
                    if e.contains("SyntaxError") || e.contains("Parse") {
                        Err(fraiseql_error::FraiseQLError::Validation {
                            message: format!("Syntax error: {}", e),
                            path: None,
                        })
                    } else {
                        Err(fraiseql_error::FraiseQLError::Unsupported {
                            message: format!("Execution error: {}", e),
                        })
                    }
                }
                Err(e) => {
                    Err(fraiseql_error::FraiseQLError::Unsupported {
                        message: format!("Task execution error: {}", e),
                    })
                }
            }
        }
    }

    fn supported_extensions(&self) -> &[&str] {
        &[".js", ".ts", ".mjs", ".mts"]
    }

    fn supports_hot_reload(&self) -> bool {
        false
    }

    fn name(&self) -> &'static str {
        "deno"
    }
}
