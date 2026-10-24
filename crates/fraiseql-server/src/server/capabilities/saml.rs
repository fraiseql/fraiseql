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

    /// SAML identity-provider management (#947): mounted only when
    /// `[saml] store_enabled = true`, behind the admin gate. Writes go through the live
    /// registry, so a created provider serves and a deleted one stops without a restart.
    #[cfg(feature = "auth-saml")]
    fn routes(&self, cx: &ServeCx<'_>) -> Option<axum::Router> {
        let server = cx.server;
        let saml = server.saml_state.as_ref().filter(|saml| saml.registry().has_store())?;
        let Some(ref token) = server.config.admin_token else {
            tracing::error!(
                "SAML IdP management disabled — [saml] store_enabled = true but admin_token \
                 is not set. Stored IdPs are served but cannot be managed over HTTP; set \
                 admin_token to enable the management endpoints."
            );
            return None;
        };
        tracing::info!(
            "SAML IdP management endpoints enabled at /api/saml/idps (admin bearer token \
             required)"
        );
        let mgmt_state = crate::api::SamlIdpManagementState {
            registry: saml.registry().clone(),
        };
        // Accepts tenant administrators (#1089); the router enforces their tenant.
        let auth_state = server.admin_principal_state(token);
        Some(crate::api::saml_idp_management_router(mgmt_state).route_layer(
            axum::middleware::from_fn_with_state(
                auth_state,
                crate::middleware::admin_principal_middleware,
            ),
        ))
    }
}
