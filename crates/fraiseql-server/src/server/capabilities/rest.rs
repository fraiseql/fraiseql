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

    /// The REST transport, read and write on one mount (#865), so the two cannot diverge
    /// in auth posture: whichever router the boot path built goes through the same
    /// `attach_auth` (#812). `route_layer` does not cross a merge, so the layer goes on the
    /// REST router itself.
    #[cfg(feature = "rest")]
    fn routes(&self, cx: &ServeCx<'_>) -> Option<axum::Router> {
        use crate::routes::rest::rest_query_router;

        let server = cx.server;
        // Whether an auth layer will actually be attached below. Passed into the router so
        // the served OpenAPI advertises the security the server enforces rather than a
        // static template (#810).
        let authenticated = server.oidc_validator.is_some() || server.hs256_auth.is_some();
        let mount = crate::routes::rest::RestMountConfig {
            compression_enabled: server.config.compression_enabled,
            auth_layer_attached: authenticated,
            // #917: the operator's `[export]` table reaches the transport.
            export:              std::sync::Arc::new(server.config.export.clone()),
        };
        let rest_app = match server.rest_router_builder.as_ref() {
            Some(build_with_writes) => build_with_writes(cx.app, &mount),
            None => rest_query_router(cx.app, &mount),
        }?;
        Some(server.attach_auth(
            rest_app,
            crate::server::routing::AuthPosture::Authenticated,
            "rest",
        ))
    }
}
