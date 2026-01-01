//! Global Tokio runtime for async database operations.
//!
//! This module provides a single global Tokio runtime that is initialized
//! once when the `fraiseql_rs` module is imported from Python. The runtime
//! is shared across all database operations for efficiency.

use crate::db::errors::{DatabaseError, DatabaseResult};
use once_cell::sync::OnceCell;
use std::sync::Arc;
use tokio::runtime::Runtime;

/// Global Tokio runtime instance.
///
/// Initialized once via `init_runtime()` and accessed via `runtime()`.
static TOKIO_RUNTIME: OnceCell<Arc<Runtime>> = OnceCell::new();

/// Configuration for the Tokio runtime.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Number of worker threads (default: number of CPUs)
    pub worker_threads: Option<usize>,
    /// Thread name prefix (default: "fraiseql-worker")
    pub thread_name: String,
    /// Enable I/O driver (default: true)
    pub enable_io: bool,
    /// Enable time driver (default: true)
    pub enable_time: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            worker_threads: None, // Auto-detect CPU count
            thread_name: "fraiseql-worker".to_string(),
            enable_io: true,
            enable_time: true,
        }
    }
}

/// Initialize the global Tokio runtime.
///
/// This function should be called once when the module is imported.
/// Subsequent calls will return `Ok(())` without reinitializing.
///
/// # Errors
///
/// Returns `DatabaseError::RuntimeInitialization` if the runtime cannot be created.
///
/// # Example
///
/// ```rust
/// use fraiseql_rs::db::runtime::{init_runtime, RuntimeConfig};
///
/// // Initialize with default config
/// init_runtime(RuntimeConfig::default())?;
/// # Ok::<(), fraiseql_rs::db::errors::DatabaseError>(())
/// ```
pub fn init_runtime(config: RuntimeConfig) -> DatabaseResult<()> {
    TOKIO_RUNTIME
        .get_or_try_init(|| {
            let mut builder = tokio::runtime::Builder::new_multi_thread();

            // Configure worker threads
            if let Some(threads) = config.worker_threads {
                builder.worker_threads(threads);
            }

            // Configure thread naming
            builder.thread_name(&config.thread_name);

            // Enable drivers
            if config.enable_io && config.enable_time {
                builder.enable_all();
            } else {
                if config.enable_io {
                    builder.enable_io();
                }
                if config.enable_time {
                    builder.enable_time();
                }
            }

            // Build runtime
            builder
                .build()
                .map(Arc::new)
                .map_err(|e| DatabaseError::RuntimeInitialization(e.to_string()))
        })
        .map(|_| ())
}

/// Access the global Tokio runtime.
///
/// # Panics
///
/// Panics if `init_runtime()` was not called first. This is a programming
/// error and should never happen in production (runtime is initialized on
/// module import).
///
/// # Example
///
/// ```rust
/// use fraiseql_rs::db::runtime::{init_runtime, runtime, RuntimeConfig};
///
/// init_runtime(RuntimeConfig::default()).unwrap();
/// let result = runtime().block_on(async {
///     // Your async code here
///     42
/// });
/// assert_eq!(result, 42);
/// ```
#[must_use]
pub fn runtime() -> &'static Runtime {
    TOKIO_RUNTIME
        .get()
        .expect("Tokio runtime not initialized. This is a bug - runtime should be initialized on module import.")
        .as_ref()
}

/// Check if the runtime is initialized.
///
/// Useful for testing and debugging.
#[must_use]
pub fn is_initialized() -> bool {
    TOKIO_RUNTIME.get().is_some()
}

/// Get runtime statistics (for monitoring/debugging).
///
/// # Note
///
/// Uses Tokio's metrics API which may change between versions.
/// The `num_workers()` method is stable as of tokio 1.35+.
#[must_use]
pub fn stats() -> RuntimeStats {
    if let Some(rt) = TOKIO_RUNTIME.get() {
        RuntimeStats {
            initialized: true,
            worker_threads: rt.metrics().num_workers(),
        }
    } else {
        RuntimeStats {
            initialized: false,
            worker_threads: 0,
        }
    }
}

/// Runtime statistics for monitoring.
#[derive(Debug, Clone)]
pub struct RuntimeStats {
    /// Whether the runtime is initialized
    pub initialized: bool,
    /// Number of worker threads
    pub worker_threads: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_initialization() {
        let config = RuntimeConfig::default();
        let result = init_runtime(config);
        assert!(result.is_ok());
        assert!(is_initialized());
    }

    #[test]
    fn test_runtime_access() {
        init_runtime(RuntimeConfig::default()).unwrap();
        let rt = runtime();
        let result = rt.block_on(async { 42 });
        assert_eq!(result, 42);
    }

    #[test]
    fn test_runtime_stats() {
        init_runtime(RuntimeConfig::default()).unwrap();
        let stats = stats();
        assert!(stats.initialized);
        assert!(stats.worker_threads > 0);
    }

    #[test]
    fn test_multiple_init_calls() {
        // First call
        let result1 = init_runtime(RuntimeConfig::default());
        assert!(result1.is_ok());

        // Second call should succeed (already initialized)
        let result2 = init_runtime(RuntimeConfig::default());
        assert!(result2.is_ok());
    }
}
