//! Server-specific configuration.

use serde::{Deserialize, Serialize};

/// Server-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CoreServerConfig {
    /// Host to bind to.
    pub host: String,

    /// Port to bind to.
    pub port: u16,

    /// Number of worker threads (0 = auto).
    pub workers: usize,

    /// Request body size limit in bytes.
    pub max_body_size: usize,

    /// Enable request logging.
    pub request_logging: bool,
}

impl Default for CoreServerConfig {
    fn default() -> Self {
        Self {
            host:            "0.0.0.0".to_string(),
            port:            8000,
            workers:         0,           // Auto-detect
            max_body_size:   1024 * 1024, // 1MB
            request_logging: true,
        }
    }
}
