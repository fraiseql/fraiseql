//! MCP (Model Context Protocol) server integration.
//!
//! Exposes FraiseQL queries and mutations as MCP tools, enabling AI/LLM clients
//! (e.g., Claude Desktop) to interact with the database through the standard
//! Model Context Protocol.

pub mod executor;
pub mod handler;
pub mod tools;

/// MCP configuration type re-exported from the core schema for use in this crate.
pub use fraiseql_core::schema::McpConfig;
