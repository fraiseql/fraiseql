//! Session state (#389): server-held per-session values, on every transport that has a
//! session.

#[cfg(feature = "auth")]
use super::ServeCx;
use crate::capability::Capability;
#[cfg(feature = "auth")]
use crate::server::provision::{BootCx, ProvisionStep};

/// `[session_state]`.
#[derive(Debug)]
pub struct SessionState;

#[async_trait::async_trait]
impl Capability for SessionState {
    #[cfg(feature = "auth")]
    fn configured(&self, cx: &ServeCx<'_>) -> bool {
        cx.server.session_state.is_some()
    }

    /// `_system.session_state`, for the Postgres backend.
    #[cfg(feature = "auth")]
    fn provision(&self, cx: &BootCx<'_>) -> Vec<ProvisionStep> {
        let server = cx.server;
        let postgres =
            server.config.session_state.as_ref().is_some_and(|c| c.backend == "postgres");
        match server.db_pool.as_ref() {
            Some(pool) if postgres => vec![ProvisionStep::new(
                "session state",
                pool,
                &[fraiseql_auth::session_state::SESSION_STATE_SCHEMA_SQL],
            )],
            _ => Vec::new(),
        }
    }
}
