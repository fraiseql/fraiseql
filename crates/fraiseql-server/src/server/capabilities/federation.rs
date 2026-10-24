//! Apollo federation: the subgraph entity resolver and its admin routes.

#[cfg(feature = "federation")]
use super::ServeCx;
use crate::capability::Capability;

/// `federation` in the compiled schema.
#[derive(Debug)]
pub struct Federation;

#[async_trait::async_trait]
impl Capability for Federation {
    #[cfg(feature = "federation")]
    fn configured(&self, cx: &ServeCx<'_>) -> bool {
        cx.app
            .executor()
            .schema()
            .federation
            .as_ref()
            .is_some_and(|federation| federation.enabled)
    }
}
