//! SAML `IdP` management API unit tests (#947).
//!
//! The behavioural coverage lives against a real PostgreSQL, in two places: the store and
//! registry semantics in `crates/fraiseql-auth/tests/postgres_saml_idp_store.rs`, and the
//! operator's path — manage `IdPs` over HTTP on a booted server and watch
//! `/auth/saml/login` follow — in `crates/fraiseql-server/tests/saml_mount_e2e_pg.rs`.
//! It has to be there: every property this module carries (hot reload, tenant scoping, a
//! name that is never reissued) is only observable as a side effect in a database.
//!
//! What remains here is what genuinely runs without one.

// ── router_construction ───────────────────────────────────────────────────────

mod router_construction {
    //! See `crates/fraiseql-server/src/observers/routes.rs::tests` for context: axum
    //! validates path-capture syntax inside `Router::route`, so any lingering `:param`
    //! literal panics here at build time rather than at first server boot (issue #316).

    use fraiseql_auth::saml::SamlIdpRegistry;

    use crate::api::saml_idp_management::{SamlIdpManagementState, saml_idp_management_router};

    #[tokio::test]
    async fn saml_idp_management_router_constructs() {
        let _router = saml_idp_management_router(SamlIdpManagementState {
            registry: SamlIdpRegistry::new(),
        });
    }
}
