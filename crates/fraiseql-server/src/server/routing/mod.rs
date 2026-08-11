//! Application router construction and route registration.
//!
//! Split into sub-modules by responsibility:
//! - [`state`]: `AppState` construction
//! - [`graphql`]: GraphQL endpoint with auth and compression
//! - [`admin`]: Base routes, studio, admin API, introspection, metrics, design audit
//! - [`auth`]: PKCE, social login, MFA, session identity, token revocation
//! - [`extensions`]: MCP, API routes, RBAC, observers, storage, functions, REST
//! - [`middleware`]: Tracing, CORS, body/header limits, timeout, rate limiting
//! - [`observers`]: Observer management routes

mod admin;
#[cfg(feature = "auth")]
mod auth;
mod extensions;
mod graphql;
#[cfg(test)]
mod http_query_method_tests;
mod middleware;
#[cfg(test)]
mod mount_authz_tests;
#[cfg(feature = "observers")]
pub(in crate::server) mod observers;
#[cfg(test)]
mod persisted_only_transport_tests;
#[cfg(test)]
mod realtime_removal_survival_tests;
mod state;
#[cfg(test)]
mod storage_policy_admin_tests;

use std::sync::Arc;

use axum::{Router, middleware::from_fn_with_state};
use fraiseql_core::{db::traits::DatabaseAdapter, security::OidcValidator};
use tracing::info;

use super::{OidcAuthState, Server, oidc_auth_middleware};
use crate::{
    middleware::{Hs256AuthState, hs256_auth_middleware},
    routes::graphql::AppState,
};

/// Whether a data-serving transport authenticates its callers.
///
/// Every transport that serves schema-backed data must declare one of these when it is
/// mounted. The variant is not a preference — it is the answer to "who may reach the
/// handlers on this router", and #812 shipped because that question was simply never
/// asked for REST: the router was merged with no auth layer at all, so
/// `security_context` was `None` on every request and the runtime's RLS and
/// session-variable tenant stamping were skipped in silence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AuthPosture {
    /// Callers are authenticated by whichever validator the deployment configured
    /// (OIDC or HS256), exactly as `/graphql` authenticates them. When no validator is
    /// configured this is a no-op layer and the transport serves anonymous callers —
    /// which is why every guard downstream must still fail closed on a `None` context
    /// rather than treat it as "no filter required".
    Authenticated,
}

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Attach the deployment's configured authentication layer to `router`.
    ///
    /// This is the single place any data-serving transport acquires authentication.
    /// `/graphql` and the REST transport both route through it so the two cannot drift:
    /// before #812 they had independent mount code and REST's simply omitted the layer.
    ///
    /// axum's `route_layer` applies only to routes already registered on the *same*
    /// `Router` — `Router::merge` does **not** propagate it — so this must be called on
    /// the transport's own router before it is merged into the application.
    pub(super) fn attach_auth<S>(
        &self,
        router: Router<S>,
        posture: AuthPosture,
        transport: &str,
    ) -> Router<S>
    where
        S: Clone + Send + Sync + 'static,
    {
        let AuthPosture::Authenticated = posture;

        if let Some(ref validator) = self.oidc_validator {
            info!(transport, "transport protected by OIDC authentication");
            let auth_state = self.oidc_auth_state(Arc::clone(validator));
            return router.route_layer(from_fn_with_state(auth_state, oidc_auth_middleware));
        }

        if let Some(ref validator) = self.hs256_auth {
            info!(transport, "transport protected by HS256 authentication");
            let realm = self
                .config
                .auth_hs256
                .as_ref()
                .and_then(|h| h.issuer.clone())
                .unwrap_or_else(|| "fraiseql".to_string());
            // #934: without the authenticator, this layer refuses a bearer-less
            // service-account request before the handler's ADR-0018 seam runs.
            let auth_state = Hs256AuthState::new(Arc::clone(validator), realm)
                .with_service_accounts(self.service_account_authenticator.clone());
            return router.route_layer(from_fn_with_state(auth_state, hs256_auth_middleware));
        }

        info!(
            transport,
            "no authentication configured — transport serves anonymous callers; row-scoping \
             guards must fail closed on an absent security context"
        );
        router
    }
}

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Build an [`OidcAuthState`] for `validator`, attaching the configured
    /// token-revocation manager (if any) so revoked tokens are rejected on **every**
    /// authenticated route (H8).
    ///
    /// All OIDC middleware construction goes through this helper to keep revocation
    /// enforcement uniform: a bare `OidcAuthState::new` at a route would silently skip
    /// the revocation check for that route.
    pub(super) fn oidc_auth_state(&self, validator: Arc<OidcValidator>) -> OidcAuthState {
        OidcAuthState::new(validator).with_revocation(self.revocation_manager.clone())
    }

    /// Build application router and return the shared `AppState`.
    ///
    /// The returned `AppState` is needed by the lifecycle module for
    /// SIGUSR1 schema reload handling.
    pub(super) fn build_router(&self) -> (Router, AppState<A>) {
        let state = self.build_app_state();

        // Build GraphQL route (possibly with auth + Content-Type enforcement).
        let graphql_router = self.build_graphql_router(&state);

        // Mount base routes, studio, admin, introspection, metrics, design audit.
        let mut app = Router::new();
        app = self.mount_base_and_admin_routes(app.merge(graphql_router), &state);

        // Mount auth routes (PKCE, social, MFA, /auth/me, revocation).
        #[cfg(feature = "auth")]
        {
            app = self.mount_auth_routes(app);
        }

        // Mount extension routes (MCP, API, RBAC, storage, functions, REST).
        app = self.mount_extensions(app, &state);

        // Apply global middleware layers (metrics, tracing, CORS, limits, timeout, rate limiting).
        app = self.apply_middleware(app, &state);

        (app, state)
    }
}
