//! SAML SP-initiated single sign-on.

#[cfg(feature = "auth-saml")]
use super::ServeCx;
use crate::capability::Capability;

/// `[saml]`.
#[derive(Debug)]
pub struct Saml;

#[async_trait::async_trait]
impl Capability for Saml {
    #[cfg(feature = "auth-saml")]
    fn configured(&self, cx: &ServeCx<'_>) -> bool {
        cx.server.saml_state.is_some()
    }
}
