//! Router-construction test for the API-key management router.
//!
//! axum validates path-capture syntax inside `Router::route`, so a bad literal
//! panics here in `cargo test` rather than at first server boot (the axum-bump
//! checklist pattern).

use super::*;

#[tokio::test]
async fn api_key_management_router_constructs() {
    let pool = sqlx::PgPool::connect_lazy("postgres://user:pass@localhost/db")
        .expect("lazy pool construction does not touch the network");
    let state = ApiKeyManagementState {
        store: Arc::new(PgApiKeyStore::new(pool)),
    };
    let _router: Router = api_key_management_router(state);
}
