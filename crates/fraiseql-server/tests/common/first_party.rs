//! How this server signs the sessions it mints, for every suite that signs in.
//!
//! A suite that boots a server which mints its own sessions (local, social and SAML
//! sign-in, session refresh, SCIM, tenant administration) takes its signing setup from
//! here, so a change to how the server signs reaches all of them at once.
//!
//! The server signs with the ES256 keys of its ring in `auth.signing_key`, sealed under
//! the key-encryption key [`FirstPartyAuth::set_env`] provides. What a suite signs here,
//! it signs with the same ring, read from the same database.

use std::{sync::Arc, time::Duration};

use fraiseql_auth::{LiveKeyRing, PgSigningKeyStore, PostgresSessionStore, RingSigner};
use fraiseql_server::{
    first_party::{EXPIRY_GRACE_SECS, SealedWithFieldEncryption},
    server_config::{IssuerConfig, ServerConfig},
};
use sqlx::PgPool;

/// The environment variable the key-encryption key is read from.
const KEK_ENV: &str = "FRAISEQL_TEST_FIRST_PARTY_KEK";
/// The key-encryption key: 32 bytes, as 64 hex digits.
const KEK: [u8; 32] = [7; 32];

/// The access-token lifetime every suite's `[auth.session]` names, unless the suite is
/// about that lifetime.
pub const SESSION_TTL_SECS: u64 = 3600;

/// The longest token a suite signs here: a day, which no suite's lifetime exceeds.
const MAX_SIGNED_LIFETIME_SECS: u64 = 24 * 3600;

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
        std::env::set_var(KEK_ENV, hex::encode(KEK));
    }

    /// `config`, signing this suite's sessions. The compiled schema must carry
    /// `[auth.session]` with a `ttl_secs`: see [`Self::session`].
    pub fn apply(&self, config: ServerConfig) -> ServerConfig {
        ServerConfig {
            auth_issuer: Some(IssuerConfig {
                url:                    self.issuer.to_string(),
                audience:               self.audience.to_string(),
                key_encryption_key_env: KEK_ENV.to_string(),
            }),
            ..config
        }
    }

    /// The `[auth.session]` a server signing its sessions needs: its access tokens'
    /// lifetime bounds every token it signs.
    pub fn session(&self) -> fraiseql_core::schema::SessionConfig {
        fraiseql_core::schema::SessionConfig {
            ttl_secs: Some(SESSION_TTL_SECS),
            ..fraiseql_core::schema::SessionConfig::default()
        }
    }

    /// [`Self::session`] as the compiled schema's JSON.
    pub fn session_json(&self) -> serde_json::Value {
        serde_json::json!({ "ttl_secs": SESSION_TTL_SECS })
    }

    /// The server's key ring in `pool`'s database, read now. A ring with no key gets one,
    /// which the server then reads as its own.
    pub async fn ring(&self, pool: &PgPool) -> Arc<LiveKeyRing> {
        let wrap = SealedWithFieldEncryption::new(&KEK).expect("a 32-byte key-encryption key");
        let store = PgSigningKeyStore::new(pool.clone(), Arc::new(wrap));
        let now = fraiseql_server::first_party::unix_now().expect("the clock");
        Arc::new(
            LiveKeyRing::open(
                store,
                now,
                MAX_SIGNED_LIFETIME_SECS + EXPIRY_GRACE_SECS,
                Duration::from_secs(1),
            )
            .await
            .expect("read the signing keys"),
        )
    }

    /// A session store over `pool` that signs as the server does.
    pub async fn session_store(&self, pool: &PgPool) -> PostgresSessionStore {
        let signer = RingSigner::new(self.ring(pool).await, MAX_SIGNED_LIFETIME_SECS);
        PostgresSessionStore::with_signer(pool.clone(), Arc::new(signer))
            .with_token_claims(self.issuer, self.audience)
    }

    /// A token carrying `claims`, signed as the server signs its own sessions, with the
    /// key it signs with in `pool`'s database.
    pub async fn sign(&self, pool: &PgPool, claims: &serde_json::Value) -> String {
        let ring = self.ring(pool).await;
        ring.current()
            .expect("a read ring")
            .active()
            .sign("at+jwt", claims)
            .expect("sign a session token")
    }

    /// A token carrying `claims` and naming the server's signing `kid`, signed by a key
    /// the server does not hold.
    pub async fn sign_foreign(&self, pool: &PgPool, claims: &serde_json::Value) -> String {
        let ring = self.ring(pool).await;
        let kid = ring.current().expect("a read ring").active().kid().to_string();
        fraiseql_test_support::issuer::sign_with_unpublished_key(&kid, claims)
    }
}
