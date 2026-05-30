//! Observer management route mounting.

use axum::{Router, middleware};
use fraiseql_core::db::traits::DatabaseAdapter;
use tracing::info;

use super::super::Server;
use crate::middleware::{OidcAuthState, oidc_auth_middleware};

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Add observer-related routes to the router.
    ///
    /// # PostgreSQL requirement
    ///
    /// The `observers` feature requires a PostgreSQL connection pool (`db_pool`).
    /// When this feature is enabled, `Server::new()` must receive a `Some(PgPool)` as the
    /// `db_pool` argument. If no pool is provided, observer management routes are skipped
    /// and an error is logged rather than panicking, so the server can still serve other
    /// requests. Callers should treat a missing pool as a configuration error.
    ///
    /// # Authentication requirement (since v2.4.0)
    ///
    /// The observer admin API — create / update / delete observers, reload runtime,
    /// inspect DLQ, read the changelog — exposes write-side cluster-state mutations
    /// and read-side endpoints that return bearer-token secrets stored in observer
    /// `actions[].headers`.  All four routers are gated behind `oidc_auth_middleware`.
    /// If no OIDC validator is configured (`[auth]` absent in `fraiseql.toml`), the
    /// routes are *not* mounted and a `WARN` is logged at startup, rather than
    /// mounting them open.  This closes the FW-21 class anonymous-write primitive
    /// (issue #348).
    #[cfg(feature = "observers")]
    pub(super) fn add_observer_routes(&self, app: Router) -> Router {
        use std::sync::Arc;

        use crate::observers::{
            ChangelogState, DlqState, ObserverRepository, ObserverState, RuntimeHealthState,
            observer_changelog_routes, observer_dlq_routes, observer_routes,
            observer_runtime_routes,
        };

        let Some(db_pool) = self.db_pool.clone() else {
            tracing::error!(
                "Observer management routes not mounted: \
                 the `observers` feature requires a PostgreSQL pool (`db_pool`). \
                 Pass `Some(sqlx::PgPool)` to Server::new() to enable observer endpoints."
            );
            return app;
        };

        let Some(ref validator) = self.oidc_validator else {
            tracing::warn!(
                "Observer admin API not mounted: \
                 the observer routes expose cluster-state mutations and bearer-token \
                 secrets (in actions[].headers) and so require an OIDC validator. \
                 Configure [auth] in fraiseql.toml to enable observer endpoints. \
                 The observer runtime itself (in-process triggers and dispatch) is \
                 unaffected — only the HTTP admin API is skipped."
            );
            return app;
        };

        let observer_state = ObserverState {
            repository: ObserverRepository::new(db_pool.clone()),
        };

        let changelog_state = ChangelogState { pool: db_pool };

        let auth_layer = || {
            let auth_state = OidcAuthState::new(Arc::clone(validator));
            middleware::from_fn_with_state(auth_state, oidc_auth_middleware)
        };

        let app = app
            .nest("/api/observers", observer_routes(observer_state).route_layer(auth_layer()))
            .nest(
                "/api/observers",
                observer_changelog_routes(changelog_state).route_layer(auth_layer()),
            );

        if let Some(ref runtime) = self.observer_runtime {
            info!(
                path = "/api/observers",
                "Observer management, runtime health, and DLQ delivery status endpoints \
                 enabled (auth-gated)"
            );

            let runtime_state = RuntimeHealthState {
                runtime: runtime.clone(),
            };

            let dlq_state = DlqState {
                runtime: runtime.clone(),
            };

            app.merge(observer_runtime_routes(runtime_state).route_layer(auth_layer()))
                .nest("/api/observers", observer_dlq_routes(dlq_state).route_layer(auth_layer()))
        } else {
            app
        }
    }
}
