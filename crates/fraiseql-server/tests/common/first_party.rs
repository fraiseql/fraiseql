//! How this server signs the sessions it mints, for every suite that signs in.
//!
//! A suite that boots a server which mints its own sessions (local, social and SAML
//! sign-in, session refresh, SCIM, tenant administration) takes its signing setup from
//! here, so a change to how the server signs reaches all of them at once.

use fraiseql_server::server_config::{ServerConfig, hs256::Hs256Config};
use sqlx::PgPool;

/// The environment variable the signing secret is read from.
const SECRET_ENV: &str = "FRAISEQL_TEST_FIRST_PARTY_HS256";
/// The signing secret: 32 bytes.
const SECRET: &str = "first-party-test-hs256-secret-32";

/// The first-party signing setup of one suite: the issuer and audience its tokens name.
#[derive(Debug, Clone, Copy)]
pub struct FirstPartyAuth {
    issuer:   &'static str,
    audience: &'static str,
}

impl FirstPartyAuth {
    /// Tokens naming `issuer` and `audience`.
    pub const fn new(issuer: &'static str, audience: &'static str) -> Self {
        Self { issuer, audience }
    }

    /// Set what the server reads from the environment to sign. Every reader wants
    /// exactly these values, so setting them unconditionally is safe under the
    /// parallel runner.
    pub fn set_env(&self) {
        std::env::set_var(SECRET_ENV, SECRET);
    }

    /// `config`, signing this suite's sessions.
    pub fn apply(&self, config: ServerConfig) -> ServerConfig {
        ServerConfig {
            auth_hs256: Some(Hs256Config {
                secret_env: SECRET_ENV.to_string(),
                issuer:     Some(self.issuer.to_string()),
                audience:   Some(self.audience.to_string()),
            }),
            ..config
        }
    }

    /// A session store over `pool` that signs as the server does.
    pub fn session_store(&self, pool: PgPool) -> fraiseql_auth::PostgresSessionStore {
        fraiseql_auth::PostgresSessionStore::with_hs256_secret(pool, SECRET.into())
    }

    /// A token carrying `claims`, signed as the server signs its own sessions.
    pub fn sign(&self, claims: &serde_json::Value) -> String {
        jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
            claims,
            &jsonwebtoken::EncodingKey::from_secret(SECRET.as_bytes()),
        )
        .expect("sign a session token")
    }

    /// A token carrying `claims`, signed by a key this server does not hold.
    pub fn sign_foreign(&self, claims: &serde_json::Value) -> String {
        jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
            claims,
            &jsonwebtoken::EncodingKey::from_secret(b"not-this-servers-secret-32-bytes"),
        )
        .expect("sign a foreign token")
    }
}
