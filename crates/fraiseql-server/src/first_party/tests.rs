use fraiseql_auth::KeyWrap as _;

use super::SealedWithFieldEncryption;

const KEK: [u8; 32] = [7; 32];
const PRIVATE: &str = "-----BEGIN PRIVATE KEY-----\nnot a real key\n-----END PRIVATE KEY-----";

fn wrap(kek: &[u8; 32]) -> SealedWithFieldEncryption {
    SealedWithFieldEncryption::new(kek).expect("a 32-byte key-encryption key")
}

#[test]
fn a_sealed_key_opens_under_the_aad_it_was_sealed_with() {
    let wrap = wrap(&KEK);
    let sealed = wrap.seal(PRIVATE, "fraiseql/auth.signing_key/v1/kid-a").expect("seal");
    assert!(
        !sealed.windows(PRIVATE.len()).any(|w| w == PRIVATE.as_bytes()),
        "sealed, not copied"
    );
    assert_eq!(wrap.open(&sealed, "fraiseql/auth.signing_key/v1/kid-a").expect("open"), PRIVATE);
}

#[test]
fn a_key_sealed_for_one_kid_does_not_open_as_another() {
    let wrap = wrap(&KEK);
    let sealed = wrap.seal(PRIVATE, "fraiseql/auth.signing_key/v1/kid-a").expect("seal");
    assert!(wrap.open(&sealed, "fraiseql/auth.signing_key/v1/kid-b").is_err());
}

#[test]
fn a_key_sealed_under_one_key_encryption_key_does_not_open_under_another() {
    let sealed = wrap(&KEK).seal(PRIVATE, "aad").expect("seal");
    assert!(wrap(&[8; 32]).open(&sealed, "aad").is_err());
}

fn issuer_config() -> crate::server_config::IssuerConfig {
    crate::server_config::IssuerConfig {
        url:                    "https://api.example.com".to_string(),
        audience:               "my-api".to_string(),
        key_encryption_key_env: "FRAISEQL_TEST_FIRST_PARTY_KEK_NEVER_SET".to_string(),
    }
}

#[test]
fn the_issuers_tokens_are_checked_as_its_own_es256_tokens_with_its_leeway_fixed() {
    let oidc = super::issuer_oidc_config(&issuer_config());
    assert_eq!(oidc.issuer.as_deref(), Some("https://api.example.com"));
    assert_eq!(oidc.audience.as_deref(), Some("my-api"));
    assert_eq!(oidc.allowed_algorithms, vec!["ES256".to_string()]);
    assert_eq!(oidc.clock_skew_secs, super::EXPIRY_GRACE_SECS);
    assert!(oidc.jwks_uri.is_none() && oidc.additional_issuers.is_empty() && oidc.me.is_none());
}

fn lazy_pool() -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://u:p@127.0.0.1:1/never")
        .expect("a lazy pool")
}

fn session(ttl_secs: Option<u64>) -> fraiseql_core::schema::SessionConfig {
    fraiseql_core::schema::SessionConfig {
        ttl_secs,
        ..fraiseql_core::schema::SessionConfig::default()
    }
}

fn refusal(result: Result<super::FirstPartyIssuer, crate::ServerError>) -> String {
    result.expect_err("refused").to_string()
}

#[tokio::test]
async fn the_issuer_needs_a_session_lifetime_to_bound_its_tokens() {
    let message =
        refusal(super::FirstPartyIssuer::new(&issuer_config(), None, false, Some(&lazy_pool())));
    assert!(message.contains("[auth_issuer]") && message.contains("[auth.session] ttl_secs"));
    let message = refusal(super::FirstPartyIssuer::new(
        &issuer_config(),
        Some(&session(None)),
        false,
        Some(&lazy_pool()),
    ));
    assert!(message.contains("[auth.session] ttl_secs"), "{message}");
}

#[tokio::test]
async fn the_issuer_needs_the_auth_pool() {
    let message = refusal(super::FirstPartyIssuer::new(
        &issuer_config(),
        Some(&session(Some(900))),
        false,
        None,
    ));
    assert!(message.contains("[auth.database]"), "{message}");
}

#[tokio::test]
async fn the_issuer_refuses_to_boot_without_its_key_encryption_key() {
    let message = refusal(super::FirstPartyIssuer::new(
        &issuer_config(),
        Some(&session(Some(900))),
        false,
        Some(&lazy_pool()),
    ));
    assert!(message.contains("FRAISEQL_TEST_FIRST_PARTY_KEK_NEVER_SET"), "{message}");
}

#[tokio::test]
async fn its_tokens_live_as_long_as_the_session_lifetime_or_anonymous_sign_up_whichever_is_longer()
{
    let kek_env = "FRAISEQL_TEST_FIRST_PARTY_KEK_LIFETIME";
    temp_env::with_var(kek_env, Some("07".repeat(32)), || {
        let config = crate::server_config::IssuerConfig {
            key_encryption_key_env: kek_env.to_string(),
            ..issuer_config()
        };
        let build = |ttl, anonymous| {
            super::FirstPartyIssuer::new(
                &config,
                Some(&session(Some(ttl))),
                anonymous,
                Some(&lazy_pool()),
            )
            .expect("built")
            .max_lifetime_secs()
        };
        assert_eq!(build(900, false), 900);
        assert_eq!(build(900, true), fraiseql_auth::anonymous::ANON_SESSION_TTL_SECS);
        assert_eq!(build(30 * 24 * 3600, true), 30 * 24 * 3600);
    });
}

#[test]
fn the_discovery_document_names_the_issuer_its_keys_and_es256() {
    let document = super::discovery_document(&crate::server_config::IssuerConfig {
        url: "https://api.example.com/".to_string(),
        ..issuer_config()
    });
    assert_eq!(document["issuer"], "https://api.example.com/");
    assert_eq!(document["jwks_uri"], "https://api.example.com/.well-known/jwks.json");
    assert_eq!(document["id_token_signing_alg_values_supported"], serde_json::json!(["ES256"]));
}

/// An issuer over a pool that never connects: its ring has not been read.
fn unread_issuer(kek_env: &str) -> std::sync::Arc<super::FirstPartyIssuer> {
    temp_env::with_var(kek_env, Some("07".repeat(32)), || {
        let config = crate::server_config::IssuerConfig {
            key_encryption_key_env: kek_env.to_string(),
            ..issuer_config()
        };
        std::sync::Arc::new(
            super::FirstPartyIssuer::new(
                &config,
                Some(&session(Some(900))),
                false,
                Some(&lazy_pool()),
            )
            .expect("built"),
        )
    })
}

async fn get(router: axum::Router, path: &str) -> axum::response::Response {
    use tower::ServiceExt as _;
    router
        .oneshot(axum::http::Request::get(path).body(axum::body::Body::empty()).expect("request"))
        .await
        .expect("served")
}

/// axum validates path syntax in `Router::route`: a bad literal panics here, not at boot.
#[tokio::test]
async fn the_publishing_routes_construct_and_serve_the_discovery_document() {
    let router = super::routes(unread_issuer("FRAISEQL_TEST_FIRST_PARTY_KEK_ROUTES"));
    let response = get(router, super::DISCOVERY_PATH).await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.expect("body");
    let document: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(document["jwks_uri"], "https://api.example.com/.well-known/jwks.json");
}

#[tokio::test]
async fn the_jwk_set_is_unavailable_until_the_ring_is_first_read() {
    let router = super::routes(unread_issuer("FRAISEQL_TEST_FIRST_PARTY_KEK_UNREAD"));
    let response = get(router, super::JWKS_PATH).await;
    assert_eq!(response.status(), axum::http::StatusCode::SERVICE_UNAVAILABLE);
}
