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

// ── search_body ───────────────────────────────────────────────────────────────

mod search_body {
    //! #1090: the `POST /.search` body shape.
    //!
    //! RFC 7644 defines `attributes` in *two* shapes, and they are not the same type. As a
    //! query parameter (§3.9) it is one comma-separated string; inside a `SearchRequest` body
    //! (§3.4.3) it is a multi-valued **array**. Reading only the string form made every
    //! conformant `.search` request carrying attributes fail deserialization, so axum answered
    //! `422 text/plain` — which a SCIM client reports as "unexpected response content format",
    //! indistinguishable from an empty body.
    #![allow(clippy::panic)] // Reason: test code, panics acceptable

    use serde_json::json;

    use crate::api::scim::{ListQuery, SearchBody, resources::project};

    /// Byte-for-byte the body `scim2-tester` sent in the failing run, captured from the wire.
    const TESTER_BODY: &str = r#"{"attributes":["externalId"],"schemas":["urn:ietf:params:scim:api:messages:2.0:SearchRequest"]}"#;

    fn parse(body: &str) -> ListQuery {
        serde_json::from_str::<SearchBody>(body)
            .unwrap_or_else(|e| panic!("SearchBody must accept {body}: {e}"))
            .into()
    }

    #[test]
    fn the_array_form_of_attributes_is_accepted() {
        let q = parse(TESTER_BODY);
        assert_eq!(q.attributes.as_deref(), Some("externalId"));
    }

    #[test]
    fn a_multi_valued_attributes_array_keeps_every_name() {
        let q = parse(r#"{"attributes":["userName","displayName"]}"#);
        // `project` splits on commas, so the array joins into its query-parameter spelling.
        assert_eq!(q.attributes.as_deref(), Some("userName,displayName"));

        let projected = project(
            json!({"id": "u1", "userName": "ada", "displayName": "Ada", "externalId": "x"}),
            q.attributes.as_deref(),
            q.excluded.as_deref(),
        );
        assert!(projected.get("userName").is_some(), "requested attribute survives");
        assert!(projected.get("displayName").is_some(), "second requested attribute survives");
        assert!(projected.get("externalId").is_none(), "unrequested attribute is dropped");
    }

    #[test]
    fn the_array_form_of_excluded_attributes_is_accepted() {
        let q = parse(r#"{"excludedAttributes":["displayName","externalId"]}"#);
        assert_eq!(q.excluded.as_deref(), Some("displayName,externalId"));
    }

    #[test]
    fn the_comma_separated_string_form_still_parses() {
        // The query-parameter spelling in a body is not RFC 7644's shape, but clients do send
        // it; accepting both is a superset, and refusing it would be a new failure.
        let q = parse(r#"{"attributes":"userName,displayName"}"#);
        assert_eq!(q.attributes.as_deref(), Some("userName,displayName"));
    }

    #[test]
    fn an_absent_or_null_attributes_field_stays_absent() {
        assert_eq!(parse(r#"{"filter":"userName eq \"ada\""}"#).attributes, None);
        assert_eq!(parse(r#"{"attributes":null}"#).attributes, None);
        // An empty array is not "return nothing" — `project` treats an empty want-list as
        // no projection, and the two spellings must agree.
        assert_eq!(parse(r#"{"attributes":[]}"#).attributes.as_deref(), Some(""));
    }
}
