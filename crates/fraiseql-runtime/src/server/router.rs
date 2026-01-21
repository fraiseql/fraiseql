use axum::{
    Router,
    routing::get,
};
use std::sync::Arc;

use crate::state::AppState;
use crate::lifecycle::health::{liveness_handler, readiness_handler, startup_handler};

/// Router builder with testable component injection
pub struct RuntimeRouter {
    state: Arc<AppState>,
}

impl RuntimeRouter {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /// Build the complete router with all configured features
    pub fn build(self) -> Router {
        let lifecycle = self.state.config.lifecycle.clone().unwrap_or_default();

        // Lifecycle endpoints (always enabled)
        Router::new()
            .route(&lifecycle.health_path, get(liveness_handler))
            .route(&lifecycle.ready_path, get(readiness_handler))
            .route("/startup", get(startup_handler))
            .with_state(self.state)

        // TODO Phase 2+: Add GraphQL endpoint
        // TODO Phase 3: Add webhook routes
        // TODO Phase 4: Add file upload routes
        // TODO Phase 5: Add auth routes
        // TODO Phase 6: Add metrics endpoint
    }
}

/// Builder pattern for testing with mock components
pub struct TestableRouterBuilder {
    state: Arc<AppState>,
}

impl TestableRouterBuilder {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    pub fn build(self) -> Router {
        RuntimeRouter::new(self.state).build()
    }
}
