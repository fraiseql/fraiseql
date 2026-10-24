//! The Model Context Protocol transport: mounted over HTTP, or served alone on stdio.

#[cfg(feature = "mcp")]
use super::ServeCx;
use crate::capability::Capability;

/// `mcp_config` in the compiled schema.
#[derive(Debug)]
pub struct Mcp;

#[async_trait::async_trait]
impl Capability for Mcp {
    #[cfg(feature = "mcp")]
    fn configured(&self, cx: &ServeCx<'_>) -> bool {
        cx.server.mcp_config.is_some()
    }
}
