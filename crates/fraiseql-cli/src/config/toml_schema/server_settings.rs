//! Server settings configuration for TOML schema (validation, debug, MCP).

use serde::{Deserialize, Serialize};

/// MCP (Model Context Protocol) server configuration.
///
/// Enables AI/LLM tools to interact with FraiseQL queries and mutations
/// through the standardized Model Context Protocol.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct McpConfig {
    /// Enable MCP server endpoint.
    pub enabled:      bool,
    /// Transport mode: "http", "stdio", or "both".
    pub transport:    String,
    /// HTTP path for MCP endpoint (e.g., "/mcp").
    pub path:         String,
    /// Require authentication for MCP requests.
    pub require_auth: bool,
    /// Whitelist of query/mutation names to expose (empty = all).
    #[serde(default)]
    pub include:      Vec<String>,
    /// Blacklist of query/mutation names to hide.
    #[serde(default)]
    pub exclude:      Vec<String>,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled:      false,
            transport:    "http".to_string(),
            path:         "/mcp".to_string(),
            require_auth: true,
            include:      Vec::new(),
            exclude:      Vec::new(),
        }
    }
}

/// Query validation limits (depth and complexity).
///
/// ```toml
/// [validation]
/// max_query_depth = 10
/// max_query_complexity = 100
/// ```
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ValidationConfig {
    /// Maximum allowed query nesting depth. `None` uses the server default (10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_query_depth: Option<u32>,

    /// Maximum allowed query complexity score. `None` uses the server default (100).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_query_complexity: Option<u32>,
}

/// Debug/development configuration.
///
/// Controls features that should only be enabled during development or
/// in trusted environments. All flags default to off.
///
/// ```toml
/// [debug]
/// enabled = true
/// database_explain = true
/// expose_sql = true
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DebugConfig {
    /// Master switch — all debug features require this to be `true`.
    pub enabled: bool,

    /// When `true`, the explain endpoint will also run `EXPLAIN` against the
    /// database and include the query plan in the response.
    pub database_explain: bool,

    /// When `true`, the explain endpoint includes the generated SQL in the
    /// response. Defaults to `true` (SQL is shown even without
    /// `database_explain`).
    pub expose_sql: bool,
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            enabled:          false,
            database_explain: false,
            expose_sql:       true,
        }
    }
}
