//! Observer management route mounting.

use axum::Router;
use fraiseql_core::db::traits::DatabaseAdapter;
use tracing::info;

use super::super::Server;

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
    #[cfg(feature = "observers")]
    pub(super) fn add_observer_routes(&self, app: Router) -> Router {
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

        let observer_state = ObserverState {
            repository: ObserverRepository::new(db_pool.clone()),
        };

        let changelog_state = ChangelogState { pool: db_pool };

        let app = app
            .nest("/api/observers", observer_routes(observer_state))
            .nest("/api/observers", observer_changelog_routes(changelog_state));

        if let Some(ref runtime) = self.observer_runtime {
            info!(
                path = "/api/observers",
                "Observer management, runtime health, and DLQ delivery status endpoints enabled"
            );

            let runtime_state = RuntimeHealthState {
                runtime: runtime.clone(),
            };

            let dlq_state = DlqState {
                runtime: runtime.clone(),
            };

            app.merge(observer_runtime_routes(runtime_state))
                .nest("/api/observers", observer_dlq_routes(dlq_state))
        } else {
            app
        }
    }
}
