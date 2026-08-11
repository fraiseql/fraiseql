//! SCIM unit tests (#946).
//!
//! The behavioural coverage — provisioning against a real database, and the property that
//! matters (`active = false` revokes sessions *and* blocks sign-in across every credential
//! path) — lives in `crates/fraiseql-server/tests/scim_provisioning_e2e_pg.rs` and the
//! third-party `scim2-tester` conformance run in the Dagger `saml` leg. It has to: none of
//! it is observable without a database and a booted server.
//!
//! What remains here is the pure logic — the filter subset and the `If-Match` comparison —
//! where a wrong answer is a silent widening rather than a visible failure.

// ── router_construction ───────────────────────────────────────────────────────

mod router_construction {
    //! axum validates path-capture syntax inside `Router::route`, so a lingering `:param`
    //! literal panics here at build time rather than at first server boot (issue #316).

    use std::sync::Arc;

    use sqlx::postgres::PgPoolOptions;

    use crate::api::scim::{ScimState, scim_router};

    #[tokio::test]
    async fn scim_router_constructs() {
        // Lazy pool: constructing a Router must not need a reachable database.
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://scim:scim@127.0.0.1:1/scim")
            .expect("lazy pool");
        let state = ScimState {
            tokens: Arc::new(fraiseql_auth::scim::PgScimTokenStore::new(pool.clone())),
            session_store: Arc::new(fraiseql_auth::PostgresSessionStore::new(pool.clone())),
            rbac: Arc::new(crate::api::rbac_management::db_backend::RbacDbBackend::new(
                pool.clone(),
            )),
            pool,
            base_url: "https://api.example.com/scim/v2".to_string(),
        };
        let _router = scim_router(state);
    }
}

// ── filter ────────────────────────────────────────────────────────────────────

mod filter {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

    use crate::api::scim::filter::{expect_attribute, parse_eq};

    #[test]
    fn parses_the_shape_provisioning_clients_actually_send() {
        let parsed = parse_eq(r#"userName eq "alice@example.com""#).expect("should parse");
        assert_eq!(parsed.attribute, "username");
        assert_eq!(parsed.value, "alice@example.com");
        assert_eq!(expect_attribute(&parsed, "userName").unwrap(), "alice@example.com");
    }

    #[test]
    fn operator_and_attribute_are_case_insensitive() {
        let parsed = parse_eq(r#"USERNAME EQ "bob""#).expect("RFC 7644 folds both");
        assert_eq!(parsed.attribute, "username");
        assert_eq!(expect_attribute(&parsed, "userName").unwrap(), "bob");
    }

    #[test]
    fn escaped_quotes_survive() {
        let parsed = parse_eq(r#"userName eq "a\"b""#).expect("should parse");
        assert_eq!(parsed.value, "a\"b");
    }

    /// The whole reason the parser is strict. Ignoring a filter it does not understand would
    /// answer "does this user exist?" with the entire directory, and the client would treat
    /// the first row as a match — provisioning onto the wrong account.
    #[test]
    fn an_unsupported_filter_is_refused_never_ignored() {
        assert!(parse_eq(r#"userName sw "ali""#).is_err(), "only eq is supported");
        assert!(parse_eq("userName eq alice").is_err(), "value must be quoted");
        assert!(parse_eq("userName").is_err(), "a bare attribute is not a filter");
        assert!(
            parse_eq(r#"userName eq "a" and active eq true"#).is_err(),
            "composition must be refused, not silently truncated to the first term"
        );
        assert!(
            parse_eq(r#"userName eq "a" or userName eq "b""#).is_err(),
            "disjunction must be refused"
        );

        // A filter on an attribute this endpoint cannot index is refused too.
        let parsed = parse_eq(r#"externalId eq "x""#).expect("parses");
        assert!(expect_attribute(&parsed, "userName").is_err());
    }
}

// ── concurrency ───────────────────────────────────────────────────────────────

mod concurrency {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

    use axum::http::{HeaderMap, HeaderValue, header};

    use crate::api::scim::{precondition_failed, resources::etag};

    fn if_match(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::IF_MATCH, HeaderValue::from_str(value).unwrap());
        headers
    }

    #[test]
    fn a_matching_version_passes_and_a_stale_one_is_refused() {
        assert!(precondition_failed(&if_match(&etag(3)), 3).is_none(), "current version passes");
        assert!(precondition_failed(&if_match("*"), 3).is_none(), "* always passes");
        assert!(precondition_failed(&HeaderMap::new(), 3).is_none(), "absent If-Match passes");

        // This is the lost-update guard: a client holding version 2 must not overwrite 3.
        assert!(precondition_failed(&if_match(&etag(2)), 3).is_some(), "stale version refused");
    }

    #[test]
    fn the_weak_marker_is_optional_and_a_candidate_list_is_accepted() {
        assert!(precondition_failed(&if_match("\"3\""), 3).is_none(), "bare entity tag matches");
        assert!(
            precondition_failed(&if_match(&format!("{}, {}", etag(2), etag(3))), 3).is_none(),
            "any candidate in the list may match"
        );
    }
}
