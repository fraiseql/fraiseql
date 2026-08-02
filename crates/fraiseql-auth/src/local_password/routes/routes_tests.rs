//! Tests for the local-password HTTP routes (#367).
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
use super::*;

/// axum validates path-capture syntax inside `Router::route`, so a bad
/// literal panics here in `cargo test` rather than at first server boot
/// (the repo's axum-bump checklist). `connect_lazy` needs no live database:
/// the router is built, never driven.
#[tokio::test]
async fn local_password_router_constructs() {
    let pool =
        sqlx::PgPool::connect_lazy("postgres://unused/unused").expect("lazy pool needs no server");
    let accounts = Arc::new(crate::InMemoryAccountStore::new());
    let state = LocalPasswordRouteState {
        authenticator: Arc::new(LocalPasswordAuthenticator::new(pool.clone(), accounts)),
        session_store: Arc::new(crate::PostgresSessionStore::new(pool)),
        rate_limiters: Arc::new(crate::RateLimiters::default()),
    };
    let _router = local_password_routes(Arc::new(state));
}
