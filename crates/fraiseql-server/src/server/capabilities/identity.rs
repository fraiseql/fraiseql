//! The identity store: per-request principal enrichment and its cache admin routes.

#[cfg(feature = "auth")]
use super::ServeCx;
use crate::capability::Capability;

/// `[identity]`.
#[derive(Debug)]
pub struct Identity;

#[async_trait::async_trait]
impl Capability for Identity {
    #[cfg(feature = "auth")]
    fn configured(&self, cx: &ServeCx<'_>) -> bool {
        !cx.app.identity.is_empty()
    }
}
