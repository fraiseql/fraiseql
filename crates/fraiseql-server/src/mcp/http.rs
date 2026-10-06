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
///
/// `require_auth` is `[mcp] require_auth`; it decides the `Host` check, see
/// [`transport_config`].
pub fn streamable_http_service(
    factory: impl Fn() -> FraiseQLMcpService + Send + Sync + 'static,
    sessions: Arc<LocalSessionManager>,
    require_auth: bool,
) -> McpHttpService {
    StreamableHttpService::new(move || Ok(factory()), sessions, transport_config(require_auth))
}

/// The transport's configuration: rmcp's defaults, with the `Host` check set by
/// `require_auth`.
///
/// rmcp accepts only loopback `Host` values by default, against DNS rebinding: a page in
/// the browser of someone running a server locally rebinds its own name to 127.0.0.1 and
/// calls it. That default refused every request to a deployment's own hostname, and every
/// tenant addressed by its domain, with 403. With `require_auth` a tool call needs a bearer
/// token, which the browser never attaches to a rebound host, so the check guards nothing
/// and is off. Without it (development only) the loopback check stays.
fn transport_config(require_auth: bool) -> StreamableHttpServerConfig {
    let config = StreamableHttpServerConfig::default();
    if require_auth {
        config.disable_allowed_hosts()
    } else {
        config
    }
}
