//! A local OIDC identity provider for tests: a JWKS endpoint on loopback serving one
//! RS256 key, a validator pointed at it, and tokens signed with that key.
//!
//! The key material is `test_data/idp/`, shared with the integration suites that
//! sign their own tokens, so there is one test key in the tree.

use std::sync::Arc;

use fraiseql_core::security::{OidcConfig, OidcValidator};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

const PRIVATE_PEM: &str = include_str!("../../test_data/idp/rs256-private.pem");
const MODULUS: &str = include_str!("../../test_data/idp/rs256-modulus.txt");
const KID: &str = "fraiseql-test-idp";
const JWKS_PATH: &str = "/jwks";

/// The issuer and audience every token and validator here agree on.
pub const ISSUER: &str = "https://idp.fraiseql.test";
/// See [`ISSUER`].
pub const AUDIENCE: &str = "fraiseql-test-api";

/// A running JWKS endpoint. Dropping it stops the server.
pub struct TestIdp {
    server: MockServer,
}

impl TestIdp {
    /// Start the JWKS endpoint.
    pub async fn start() -> Self {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(JWKS_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "keys": [{
                    "kty": "RSA", "kid": KID, "alg": "RS256", "use": "sig",
                    "n": MODULUS.trim(), "e": "AQAB",
                }]
            })))
            .mount(&server)
            .await;
        Self { server }
    }

    /// A validator that trusts this endpoint's key, for [`ISSUER`] and [`AUDIENCE`].
    ///
    /// # Panics
    ///
    /// If the validator refuses the loopback `jwks_uri`.
    #[must_use]
    pub fn validator(&self, required: bool) -> Arc<OidcValidator> {
        let config = OidcConfig {
            issuer: Some(ISSUER.to_string()),
            audience: Some(AUDIENCE.to_string()),
            required,
            allowed_algorithms: vec!["RS256".to_string()],
            ..OidcConfig::default()
        };
        let uri = format!("{}{JWKS_PATH}", self.server.uri());
        Arc::new(OidcValidator::with_jwks_uri(config, &uri).expect("a loopback jwks_uri"))
    }
}

/// A token for `sub` carrying `jti`, valid for an hour, signed with the endpoint's key.
///
/// # Panics
///
/// If the fixture key does not parse.
#[must_use]
pub fn token(sub: &str, jti: &str) -> String {
    let now = chrono::Utc::now().timestamp();
    let claims = serde_json::json!({
        "iss": ISSUER, "aud": AUDIENCE, "sub": sub, "jti": jti,
        "iat": now, "exp": now + 3600,
    });
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(KID.to_string());
    let key = EncodingKey::from_rsa_pem(PRIVATE_PEM.as_bytes()).expect("the fixture key");
    jsonwebtoken::encode(&header, &claims, &key).expect("signing")
}
