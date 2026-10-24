//! SAML SP-initiated single sign-on.

#[cfg(feature = "auth-saml")]
use tracing::info;

#[cfg(feature = "auth-saml")]
use super::{RunCx, ServeCx};
use crate::capability::Capability;
#[cfg(feature = "auth-saml")]
use crate::{
    capability::Started,
    server::{
        ServerError,
        provision::{BootCx, ProvisionStep},
    },
};

/// `[saml]`.
#[derive(Debug)]
pub struct Saml;

#[async_trait::async_trait]
impl Capability for Saml {
    #[cfg(feature = "auth-saml")]
    fn configured(&self, cx: &ServeCx<'_>) -> bool {
        cx.server.saml_state.is_some()
    }

    /// The assertion replay store, shared across replicas (#949), and the per-tenant
    /// identity-provider store when `store_enabled`.
    #[cfg(feature = "auth-saml")]
    fn provision(&self, cx: &BootCx<'_>) -> Vec<ProvisionStep> {
        let server = cx.server;
        let (Some(_), Some(pool)) = (server.saml_state.as_ref(), server.db_pool.as_ref()) else {
            return Vec::new();
        };
        let mut steps = vec![ProvisionStep::new(
            "SAML replay",
            pool,
            &[fraiseql_auth::saml::PG_SAML_REPLAY_SCHEMA_SQL],
        )];
        if server.config.saml.as_ref().is_some_and(|saml| saml.store_enabled) {
            steps.push(ProvisionStep::new(
                "SAML IdP store",
                pool,
                &[fraiseql_auth::saml::PG_SAML_IDP_SCHEMA_SQL],
            ));
        }
        steps
    }

    /// Load the stored identity providers before the first request, then keep them
    /// current (#947).
    ///
    /// Writes through this server's own admin API refresh synchronously, so the ticker
    /// exists to pick up another replica's changes and to keep the certificate-expiry
    /// warning honest.
    #[cfg(feature = "auth-saml")]
    async fn start(&self, cx: &mut RunCx<'_>) -> crate::Result<Started> {
        let server = &mut *cx.server;
        let (Some(saml), Some(config)) = (server.saml_state.as_ref(), server.config.saml.as_ref())
        else {
            return Ok(Started::Running);
        };
        if !config.store_enabled {
            return Ok(Started::Running);
        }
        let registry = saml.registry().clone();
        registry.refresh().await.map_err(|e| {
            // Fail loud: serving with an unreadable IdP store would serve only the
            // config-file IdPs while reporting the store as enabled.
            ServerError::ConfigError(format!("[saml] initial IdP store load: {e}"))
        })?;
        registry.log_expiry_report(config.certificate_expiry_warning_days);
        info!(
            stored_idps = registry.idp_names().len() - config.idps.len(),
            refresh_interval_secs = config.refresh_interval_secs,
            "SAML per-tenant IdP store enabled (core.tb_saml_idp, admin CRUD at /api/saml/idps)"
        );
        let interval = std::time::Duration::from_secs(config.refresh_interval_secs);
        let warning_days = config.certificate_expiry_warning_days;
        server.tasks.spawn(registry.refresh_loop(interval, warning_days));
        info!(
            refresh_interval_secs = config.refresh_interval_secs,
            "SAML IdP store refresher started"
        );
        Ok(Started::Running)
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
