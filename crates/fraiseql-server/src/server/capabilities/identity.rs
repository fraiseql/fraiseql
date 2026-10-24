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

    /// The identity-cache admin API (flush), behind the admin bearer gate, mounted only
    /// when an enrichment resolver exists (#539). It lets an operator propagate a revoke or
    /// a provision immediately instead of waiting out the TTL.
    #[cfg(feature = "auth")]
    fn routes(&self, cx: &ServeCx<'_>) -> Option<axum::Router> {
        let server = cx.server;
        let (Some(resolver), Some(token)) =
            (cx.app.identity.resolver(), server.config.admin_token.as_ref())
        else {
            return None;
        };
        let auth_state = crate::server::BearerAuthState::with_max_failures(
            token.clone(),
            server.config.admin_auth_max_failures,
        );
        tracing::info!(
            "Identity-cache admin API enabled (POST /api/identity/flush[-all]; admin bearer \
             token required)"
        );
        Some(crate::identity::identity_admin_router(resolver.clone()).route_layer(
            axum::middleware::from_fn_with_state(auth_state, crate::server::bearer_auth_middleware),
        ))
    }
}
