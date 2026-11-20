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

/// Timestamps are written as stored documents hold them today: the fraction padded to
/// `.120` / `.500`, none at a whole second, and a `Z` suffix. A document read and written
/// again keeps those bytes; webhooks, idempotency keys and JSONB comparisons depend on them.
mod stored_time_format {
    use crate::api::saml_idp_management::SamlIdpDto;

    #[test]
    fn a_saml_idp_keeps_its_stored_times() {
        let doc = serde_json::json!({"id":"00000000-0000-0000-0000-000000000001","idp_name":"n","tenant_id":null,"sp_entity_id":"sp","acs_url":"https://example.test/acs","metadata_xml":"<x/>","idp_entity_id":"idp","trust_asserted_email":false,"certificate_expires_at":"2025-11-20T12:00:01.500Z","created_at":"2025-11-20T12:00:00.120Z","updated_at":"2025-11-20T12:00:02Z"});
        let read: SamlIdpDto =
            serde_json::from_value(doc.clone()).expect("the stored document reads");
        assert_eq!(serde_json::to_value(&read).expect("it writes"), doc);
    }
}
