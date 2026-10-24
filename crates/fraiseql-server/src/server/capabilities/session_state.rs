//! Session state (#389): server-held per-session values, on every transport that has a
//! session.

#[cfg(feature = "auth")]
use super::ServeCx;
use crate::capability::Capability;

/// `[session_state]`.
#[derive(Debug)]
pub struct SessionState;

#[async_trait::async_trait]
impl Capability for SessionState {
    #[cfg(feature = "auth")]
    fn configured(&self, cx: &ServeCx<'_>) -> bool {
        cx.server.session_state.is_some()
    }
}
