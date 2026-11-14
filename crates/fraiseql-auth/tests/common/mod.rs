//! Shared helpers for the auth crate's integration tests.

use std::sync::Arc;

use fraiseql_auth::{AccessTokenSigner, AuthError, Claims, PostgresSessionStore};
use sqlx::PgPool;

/// Signs every access token ES256 with a key nobody publishes.
///
/// For suites whose subject is what a session carries or when one is refused (MFA, tenant,
/// provisioning), not how it is signed. The token has the shape the first-party issuer
/// mints (ES256, `kid`, `typ` `at+jwt`), so a test reads the claims it carries; signing and
/// key publication are covered by `postgres_signing_key.rs`.
#[derive(Debug)]
pub struct UnpublishedKeySigner;

impl AccessTokenSigner for UnpublishedKeySigner {
    fn sign_access_token(&self, claims: &Claims) -> Result<String, AuthError> {
        let claims = serde_json::to_value(claims).map_err(|e| AuthError::Internal {
            message: e.to_string(),
        })?;
        Ok(fraiseql_test_support::issuer::sign_with_unpublished_key("auth-test", &claims))
    }
}

/// A session store over `pool` that can mint sessions.
pub fn minting_store(pool: PgPool) -> PostgresSessionStore {
    PostgresSessionStore::with_signer(pool, Arc::new(UnpublishedKeySigner))
}
