//! The MCP Streamable HTTP transport, as mounted at `[mcp] path`.
//!
//! One constructor, so the route and its tests build the same service.

use std::sync::Arc;

use rmcp::transport::{
    StreamableHttpServerConfig, StreamableHttpService,
    streamable_http_server::session::local::LocalSessionManager,
};

use super::handler::FraiseQLMcpService;

/// The tower service mounted at `[mcp] path`.
pub type McpHttpService = StreamableHttpService<FraiseQLMcpService, LocalSessionManager>;

/// Build the Streamable HTTP service over `sessions`.
///
/// `factory` builds one [`FraiseQLMcpService`] per MCP session. Authentication happens per
/// tool call inside that service, so everything the transport does before a session's first
/// tool call — including what it allocates for a request that never becomes a session — is
/// reachable without credentials.
pub fn streamable_http_service(
    factory: impl Fn() -> FraiseQLMcpService + Send + Sync + 'static,
    sessions: Arc<LocalSessionManager>,
) -> McpHttpService {
    StreamableHttpService::new(
        move || Ok(factory()),
        sessions,
        StreamableHttpServerConfig::default(),
    )
}
