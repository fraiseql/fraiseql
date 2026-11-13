//! The first-party issuer: the keys this server signs its own sessions with
//! (`[auth_issuer]`, #1964).

use std::{sync::Arc, time::Duration};

use fraiseql_auth::{AccessTokenSigner, LiveKeyRing, PgSigningKeyStore, RingSigner};
use fraiseql_core::security::{OidcConfig, OidcValidator};
use fraiseql_jwks::JwksKeys;

use crate::{ServerError, server_config::IssuerConfig};

/// How long past its `exp` a session this server minted is still accepted, so a denial of
/// one must outlive it by as much.
pub const EXPIRY_GRACE_SECS: u64 = 60;

mod key_wrap;
mod ring_keys;
mod routes;

pub use key_wrap::SealedWithFieldEncryption;
pub use ring_keys::RingKeys;
pub use routes::{DISCOVERY_PATH, JWKS_PATH, discovery_document, routes};

/// How often a lookup of a `kid` the ring does not publish may re-read the table.
const RELOAD_COOLDOWN: Duration = Duration::from_secs(30);

/// How often the ring rotates, sweeps and re-reads on its own.
pub const TICK_INTERVAL: Duration = Duration::from_hours(1);

/// The oldest read of the ring a replica signs from: two ticks.
///
/// One failed tick does not stop sign-in. A retired key stays published this much longer,
/// since a replica that has not re-read yet still signs with it.
pub const STALE_MAX: Duration = Duration::from_hours(2);

/// The ring this server signs with, over `store`.
///
/// Its tokens live at most `max_lifetime_secs`: they validate for that long plus
/// [`EXPIRY_GRACE_SECS`], and it signs from reads at most [`STALE_MAX`] old. Every ring the
/// server or its tests build comes from here, so the publish window has one definition.
#[must_use]
pub const fn key_ring(
    store: PgSigningKeyStore,
    max_lifetime_secs: u64,
    reload_cooldown: Duration,
) -> LiveKeyRing {
    LiveKeyRing::new(
        store,
        max_lifetime_secs.saturating_add(EXPIRY_GRACE_SECS),
        STALE_MAX.as_secs(),
        reload_cooldown,
    )
}

/// The signing-key store `config` names, built as the server builds it.
///
/// It is on the auth pool `database` describes and seals under the key-encryption key in the
/// variable `config` names: what `fraiseql auth rotate-signing-key` acts on.
///
/// # Errors
///
/// [`ServerError::ConfigError`] without `[auth.database]`, when its variable is unset or not
/// a PostgreSQL URL, and when the key-encryption key cannot be read. No error quotes the URL
/// or the key.
pub fn signing_key_store(
    config: &IssuerConfig,
    database: Option<&fraiseql_core::schema::AuthDatabaseConfig>,
) -> Result<PgSigningKeyStore, ServerError> {
    let pool =
        crate::auth_database::build_auth_pool(database, &["[auth_issuer]"])?.ok_or_else(|| {
            ServerError::ConfigError(
                "[auth_issuer] requires [auth.database]: its signing keys are rows in \
                 auth.signing_key"
                    .to_string(),
            )
        })?;
    sealed_store(config, pool)
}

/// The store over `pool`, sealing under the key-encryption key `config` names.
fn sealed_store(
    config: &IssuerConfig,
    pool: sqlx::PgPool,
) -> Result<PgSigningKeyStore, ServerError> {
    let kek =
        fraiseql_auth::signing_key::key_encryption_key_from_env(&config.key_encryption_key_env)
            .map_err(|e| ServerError::ConfigError(format!("[auth_issuer]: {e}")))?;
    let wrap = SealedWithFieldEncryption::new(&kek)
        .map_err(|e| ServerError::ConfigError(format!("[auth_issuer]: {e}")))?;
    Ok(PgSigningKeyStore::new(pool, Arc::new(wrap)))
}

/// Unix seconds now.
///
/// # Errors
///
/// [`ServerError::ConfigError`] when the system clock reads before 1970.
pub fn unix_now() -> Result<u64, ServerError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .map_err(|e| ServerError::ConfigError(format!("the system clock is before 1970: {e}")))
}

/// The validator profile the first-party issuer's tokens are checked against: its own
/// issuer and audience, ES256 only, with the issuer's leeway fixed.
#[must_use]
pub fn issuer_oidc_config(config: &IssuerConfig) -> OidcConfig {
    OidcConfig {
        issuer: Some(config.url.clone()),
        audience: Some(config.audience.clone()),
        allowed_algorithms: vec!["ES256".to_string()],
        clock_skew_secs: EXPIRY_GRACE_SECS,
        ..OidcConfig::default()
    }
}

/// The first-party issuer this server runs: what it is configured as, and the ring it
/// signs with.
pub struct FirstPartyIssuer {
    config:            IssuerConfig,
    ring:              Arc<LiveKeyRing>,
    max_lifetime_secs: u64,
    own_tokens:        Arc<OidcValidator>,
}

impl std::fmt::Debug for FirstPartyIssuer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FirstPartyIssuer")
            .field("config", &self.config)
            .field("ring", &self.ring)
            .field("max_lifetime_secs", &self.max_lifetime_secs)
            .finish_non_exhaustive()
    }
}

impl FirstPartyIssuer {
    /// The issuer `config` describes, over the auth pool. Nothing is read yet: the ring
    /// is read by its first [`LiveKeyRing::tick`], once the tables exist.
    ///
    /// The longest a token it signs may live is `[auth.session] ttl_secs`, or anonymous
    /// sign-up's fixed lifetime when `mints_anonymous`, whichever is longer: a retired
    /// key stays published that long, plus the leeway.
    ///
    /// # Errors
    ///
    /// [`ServerError::ConfigError`] without `[auth.session] ttl_secs`, without an auth
    /// pool, or when the key-encryption key cannot be read.
    pub fn new(
        config: &IssuerConfig,
        session: Option<&fraiseql_core::schema::SessionConfig>,
        mints_anonymous: bool,
        auth_pool: Option<&sqlx::PgPool>,
    ) -> Result<Self, ServerError> {
        let ttl_secs = session.and_then(|session| session.ttl_secs).ok_or_else(|| {
            ServerError::ConfigError(
                "[auth_issuer] requires [auth.session] ttl_secs: it bounds how long a token \
                 the issuer signs can live, and so how long a retired signing key stays \
                 published"
                    .to_string(),
            )
        })?;
        let pool = auth_pool.ok_or_else(|| {
            ServerError::ConfigError(
                "[auth_issuer] requires [auth.database]: its signing keys are rows in \
                 auth.signing_key"
                    .to_string(),
            )
        })?;
        let store = sealed_store(config, pool.clone())?;
        let max_lifetime_secs = if mints_anonymous {
            ttl_secs.max(fraiseql_auth::anonymous::ANON_SESSION_TTL_SECS)
        } else {
            ttl_secs
        };
        let ring = Arc::new(key_ring(store, max_lifetime_secs, RELOAD_COOLDOWN));
        let own_tokens = OidcValidator::from_issuer_keys(
            issuer_oidc_config(config),
            Arc::new(RingKeys::new(Arc::clone(&ring))),
        )
        .map_err(|e| ServerError::ConfigError(format!("[auth_issuer]: {e}")))?;
        Ok(Self {
            config: config.clone(),
            ring,
            max_lifetime_secs,
            own_tokens: Arc::new(own_tokens),
        })
    }

    /// What this issuer is configured as.
    #[must_use]
    pub const fn config(&self) -> &IssuerConfig {
        &self.config
    }

    /// The ring this issuer signs with.
    #[must_use]
    pub const fn ring(&self) -> &Arc<LiveKeyRing> {
        &self.ring
    }

    /// The longest a token this issuer signs may live.
    #[must_use]
    pub const fn max_lifetime_secs(&self) -> u64 {
        self.max_lifetime_secs
    }

    /// The validator profile this issuer's tokens are checked against.
    #[must_use]
    pub fn oidc_config(&self) -> OidcConfig {
        issuer_oidc_config(&self.config)
    }

    /// This issuer's keys, for the validator: read from the ring, never fetched.
    #[must_use]
    pub fn keys(&self) -> Arc<dyn JwksKeys> {
        Arc::new(RingKeys::new(Arc::clone(&self.ring)))
    }

    /// A validator that admits this issuer's tokens and no other issuer's: the routes
    /// that act on the caller's own account take a session this server minted, never an
    /// external identity provider's token naming the same `sub`.
    #[must_use]
    pub fn own_tokens(&self) -> Arc<OidcValidator> {
        Arc::clone(&self.own_tokens)
    }

    /// The signer every session store that mints for this issuer signs with.
    #[must_use]
    pub fn signer(&self) -> Arc<dyn AccessTokenSigner> {
        Arc::new(RingSigner::new(Arc::clone(&self.ring), self.max_lifetime_secs))
    }
}

#[cfg(test)]
mod tests;
