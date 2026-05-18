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
mod middleware;
#[cfg(feature = "observers")]
mod observers;
mod state;

use axum::Router;
use fraiseql_core::db::traits::DatabaseAdapter;

use super::Server;
use crate::routes::graphql::AppState;

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
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

        // Mount extension routes (MCP, API, RBAC, storage, functions, REST, realtime).
        app = self.mount_extensions(app, &state);

        // Apply global middleware layers (metrics, tracing, CORS, limits, timeout, rate limiting).
        app = self.apply_middleware(app, &state);

        (app, state)
    }
}
