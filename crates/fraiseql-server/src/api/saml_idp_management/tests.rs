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

// ── dto ───────────────────────────────────────────────────────────────────────

mod dto {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

    use chrono::Utc;
    use fraiseql_auth::saml::SamlIdpRecord;
    use uuid::Uuid;

    use crate::api::saml_idp_management::SamlIdpDto;

    fn record(tenant_id: Option<Uuid>, trust_asserted_email: bool) -> SamlIdpRecord {
        SamlIdpRecord {
            id: Uuid::new_v4(),
            idp_name: "acme-okta".to_string(),
            tenant_id,
            sp_entity_id: "https://sp.example.com/metadata".to_string(),
            acs_url: "https://sp.example.com/auth/saml/acs".to_string(),
            metadata_xml: "<EntityDescriptor/>".to_string(),
            idp_entity_id: "https://idp.example.com".to_string(),
            trust_asserted_email,
            certificate_expires_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// The API must not let an operator believe a recorded flag is in force. A tenant-bound
    /// `IdP`'s `trust_asserted_email` is stored but inert, because the account store keys
    /// verified email globally — so the DTO reports both, separately (#1088).
    #[test]
    fn tenant_bound_optin_is_reported_as_stored_but_not_effective() {
        let dto = SamlIdpDto::from(record(Some(Uuid::new_v4()), true));
        assert!(dto.trust_asserted_email, "the stored flag is reported as stored");
        assert!(
            !dto.email_linking_effective,
            "a tenant-bound IdP's opt-in is inert and must be reported as such"
        );
    }

    #[test]
    fn untenanted_optin_is_effective() {
        let dto = SamlIdpDto::from(record(None, true));
        assert!(dto.email_linking_effective, "the untenanted case is what the opt-in is for");
    }

    #[test]
    fn no_optin_is_never_effective() {
        assert!(!SamlIdpDto::from(record(None, false)).email_linking_effective);
        assert!(!SamlIdpDto::from(record(Some(Uuid::new_v4()), false)).email_linking_effective);
    }
}
