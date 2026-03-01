//! MCP (Model Context Protocol) server integration.
//!
//! Exposes FraiseQL queries and mutations as MCP tools, enabling AI/LLM clients
//! (e.g., Claude Desktop) to interact with the database through the standard
//! Model Context Protocol.

pub mod executor;
pub mod handler;
pub mod tools;

use serde::Deserialize;

/// Runtime MCP configuration (deserialized from compiled schema JSON).
#[derive(Debug, Clone, Deserialize)]
pub struct McpConfig {
    /// Whether MCP is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// Transport: "http", "stdio", or "both".
    #[serde(default = "default_transport")]
    pub transport: String,

    /// HTTP path for MCP endpoint.
    #[serde(default = "default_path")]
    pub path: String,

    /// Require authentication for MCP requests.
    #[serde(default = "default_true")]
    pub require_auth: bool,

    /// Whitelist of operation names to expose (empty = all).
    #[serde(default)]
    pub include: Vec<String>,

    /// Blacklist of operation names to hide.
    #[serde(default)]
    pub exclude: Vec<String>,
}

fn default_transport() -> String {
    "http".to_string()
}

fn default_path() -> String {
    "/mcp".to_string()
}

fn default_true() -> bool {
    true
}
