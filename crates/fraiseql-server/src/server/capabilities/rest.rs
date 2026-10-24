//! The REST transport.

#[cfg(feature = "rest")]
use super::ServeCx;
use crate::capability::Capability;

/// `rest_config` in the compiled schema.
#[derive(Debug)]
pub struct Rest;

#[async_trait::async_trait]
impl Capability for Rest {
    #[cfg(feature = "rest")]
    fn configured(&self, cx: &ServeCx<'_>) -> bool {
        cx.app.executor().schema().rest_config.as_ref().is_some_and(|rest| rest.enabled)
    }
}
