//! gRPC: built and tested, mounted by no entry point yet (D-3 mounts it through `admit()`).

#[cfg(feature = "grpc")]
use super::ServeCx;
use crate::capability::Capability;

/// `[grpc]` in the compiled schema.
#[derive(Debug)]
pub struct Grpc;

#[async_trait::async_trait]
impl Capability for Grpc {
    #[cfg(feature = "grpc")]
    fn configured(&self, cx: &ServeCx<'_>) -> bool {
        cx.app.executor().schema().grpc_config.as_ref().is_some_and(|grpc| grpc.enabled)
    }
}
