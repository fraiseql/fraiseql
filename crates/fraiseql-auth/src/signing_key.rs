//! The first-party issuer's signing keys: an ES256 key ring kept in `auth.signing_key`.
//!
//! Tokens the first-party issuer signs with these keys need only the published public
//! half to verify. The ring holds:
//!
//! * exactly one **active** key, which signs every new token. A partial unique index makes a second
//!   active row impossible, whatever the callers do;
//! * any number of **retired** keys, which sign nothing but stay published until every token they
//!   signed has expired. How long that is depends on how long a session lasts and on the
//!   validator's leeway, which the caller knows and this table does not, so the window is computed
//!   when the ring is read ([`PgSigningKeyStore::load`]) and swept ([`PgSigningKeyStore::sweep`]),
//!   never stored. A changed session length therefore applies to keys already retired.
//!
//! The private half is stored sealed by a [`KeyWrap`] the caller supplies: the server
//! backs it with its secrets layer. The additional authenticated data is built here and
//! names the key's `kid`, so a sealed key copied onto another row does not open.

use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use p256::{
    SecretKey,
    elliptic_curve::sec1::ToEncodedPoint as _,
    pkcs8::{DecodePrivateKey as _, EncodePrivateKey as _, LineEnding},
};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{PgPool, Row as _};
use zeroize::Zeroizing;

use crate::error::{AuthError, Result};

/// Idempotent DDL for `auth.signing_key`.
pub const SIGNING_KEY_SCHEMA_SQL: &str = concat!(
    claim_auth_schema!(),
    r"
-- One row per signing key the first-party issuer has made. The private key is sealed
-- by the server's key-encryption key; the public half is the JWK the issuer publishes.
-- A row with no `retired_at` is the active key; there is never more than one.
CREATE TABLE IF NOT EXISTS auth.signing_key (
    kid             TEXT PRIMARY KEY,
    alg             TEXT NOT NULL CHECK (alg = 'ES256'),
    public_jwk      JSONB NOT NULL,
    private_key_enc BYTEA NOT NULL,
    created_at      BIGINT NOT NULL,
    retired_at      BIGINT
);
CREATE UNIQUE INDEX IF NOT EXISTS uq_signing_key_one_active
    ON auth.signing_key ((true)) WHERE retired_at IS NULL;
REVOKE ALL ON auth.signing_key FROM PUBLIC;
"
);

/// The environment variable the key-encryption key is read from when the configuration
/// names none.
pub const DEFAULT_KEK_ENV: &str = "FRAISEQL_AUTH_SIGNING_KEK";

/// How old the active key may get before it is replaced: 30 days.
pub const ROTATION_PERIOD_SECS: u64 = 30 * 24 * 60 * 60;

/// The one lock every change to the ring takes, so two replicas rotating at once leave
/// one active key between them. An arbitrary constant; it only has to be this one.
const RING_LOCK: i64 = 0x6672_6169_7365_6b72; // "fraisekr"

/// Seals and opens a signing key's private half.
///
/// `aad` is authenticated, not encrypted: an implementation must refuse to open a
/// ciphertext under any `aad` other than the one it was sealed with. This crate builds
/// the `aad` (it names the key's `kid`); an implementation passes it through unchanged.
pub trait KeyWrap: Send + Sync + fmt::Debug {
    /// Encrypt `plaintext`, binding `aad` to the ciphertext.
    ///
    /// # Errors
    ///
    /// Any failure to encrypt.
    fn seal(&self, plaintext: &str, aad: &str) -> Result<Vec<u8>>;

    /// Decrypt `ciphertext`, which must have been sealed with the same `aad`.
    ///
    /// # Errors
    ///
    /// A wrong key, a wrong `aad` or a damaged ciphertext.
    fn open(&self, ciphertext: &[u8], aad: &str) -> Result<String>;
}

/// The authenticated data a key's private half is sealed with.
fn aad(kid: &str) -> String {
    format!("fraiseql/auth.signing_key/v1/{kid}")
}

/// Read the 32-byte key-encryption key, hex-encoded, from the environment variable `var`.
///
/// # Errors
///
/// [`AuthError::ConfigError`] when the variable is unset, is not 64 hex digits, or
/// decodes to anything but 32 bytes. A server configured to sign must refuse to boot
/// rather than seal its keys under a guessable key.
pub fn key_encryption_key_from_env(var: &str) -> Result<Zeroizing<[u8; 32]>> {
    let value = Zeroizing::new(std::env::var(var).map_err(|_| AuthError::ConfigError {
        message: format!(
            "the signing keys' key-encryption key is read from {var}, which is not set: set it \
             to 64 hex digits (32 random bytes)"
        ),
    })?);
    key_encryption_key_from_hex(var, value.trim())
}

/// [`key_encryption_key_from_env`] on an already-read value; `var` names it in errors.
///
/// # Errors
///
/// See [`key_encryption_key_from_env`].
pub fn key_encryption_key_from_hex(var: &str, hex_digits: &str) -> Result<Zeroizing<[u8; 32]>> {
    let refuse = |why: &str| AuthError::ConfigError {
        message: format!(
            "{var} must be 64 hex digits (32 random bytes) to encrypt the signing keys; {why}"
        ),
    };
    let bytes = Zeroizing::new(hex::decode(hex_digits).map_err(|_| refuse("it is not hex"))?);
    let mut key = Zeroizing::new([0_u8; 32]);
    if bytes.len() != key.len() {
        return Err(refuse(&format!("it decodes to {} bytes", bytes.len())));
    }
    key.copy_from_slice(&bytes);
    Ok(key)
}

/// One ES256 key: what signs, and the JWK that publishes its public half.
pub struct SigningKey {
    kid:      String,
    encoding: EncodingKey,
    jwk:      Value,
}

impl fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SigningKey")
            .field("kid", &self.kid)
            .field("encoding", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl SigningKey {
    /// A fresh P-256 key with a random `kid`, and its private half as PKCS#8 PEM.
    fn generate() -> Result<(Self, Zeroizing<String>)> {
        let secret = loop {
            // A random 32-byte string is a valid scalar unless it is zero or not below the
            // group order; both are vanishingly rare, but retry rather than assume.
            if let Ok(secret) = SecretKey::from_slice(&rand::random::<[u8; 32]>()) {
                break secret;
            }
        };
        let pem = secret.to_pkcs8_pem(LineEnding::LF).map_err(|e| internal("encode", &e))?;
        let pem = Zeroizing::new(pem.to_string());
        let kid = uuid::Uuid::new_v4().simple().to_string();
        Ok((Self::from_pem(kid, &pem)?, pem))
    }

    /// The key whose private half is `pem` (PKCS#8), named `kid`.
    fn from_pem(kid: String, pem: &str) -> Result<Self> {
        let secret = SecretKey::from_pkcs8_pem(pem).map_err(|e| internal("decode", &e))?;
        let encoding =
            EncodingKey::from_ec_pem(pem.as_bytes()).map_err(|e| internal("load", &e))?;
        let point = secret.public_key().to_encoded_point(false);
        let (Some(x), Some(y)) = (point.x(), point.y()) else {
            return Err(internal("publish", &"an uncompressed point has no coordinates"));
        };
        let jwk = json!({
            "kty": "EC",
            "crv": "P-256",
            "alg": "ES256",
            "use": "sig",
            "kid": kid,
            "x": URL_SAFE_NO_PAD.encode(x),
            "y": URL_SAFE_NO_PAD.encode(y),
        });
        Ok(Self { kid, encoding, jwk })
    }

    /// The key's `kid`.
    #[must_use]
    pub fn kid(&self) -> &str {
        &self.kid
    }

    /// The JWK publishing this key's public half.
    #[must_use]
    pub const fn jwk(&self) -> &Value {
        &self.jwk
    }

    /// Sign `claims` with this key: ES256, this key's `kid`, and the header `typ`.
    ///
    /// # Errors
    ///
    /// [`AuthError::Internal`] when the claims do not serialize.
    pub fn sign(&self, typ: &str, claims: &impl Serialize) -> Result<String> {
        let mut header = Header::new(Algorithm::ES256);
        header.typ = Some(typ.to_string());
        header.kid = Some(self.kid.clone());
        jsonwebtoken::encode(&header, claims, &self.encoding).map_err(|e| internal("sign with", &e))
    }
}

fn internal(doing: &str, e: &impl fmt::Display) -> AuthError {
    AuthError::Internal {
        message: format!("failed to {doing} a signing key: {e}"),
    }
}

fn db_error(doing: &str, e: &sqlx::Error) -> AuthError {
    AuthError::DatabaseError {
        message: format!("failed to {doing}: {e}"),
    }
}

/// The ring as read at one instant: the key that signs, and every key published.
#[derive(Debug)]
pub struct KeyRing {
    active:    SigningKey,
    published: Vec<Value>,
}

impl KeyRing {
    /// The key that signs new tokens.
    #[must_use]
    pub const fn active(&self) -> &SigningKey {
        &self.active
    }

    /// The JWK set to publish: the active key and every retired key still in its window.
    #[must_use]
    pub fn jwks(&self) -> Value {
        json!({ "keys": self.published })
    }

    /// The `kid`s of every published key, active first.
    pub fn kids(&self) -> impl Iterator<Item = &str> {
        self.published.iter().filter_map(|jwk| jwk.get("kid").and_then(Value::as_str))
    }
}

/// `auth.signing_key`, read and changed through the auth pool.
pub struct PgSigningKeyStore {
    db:   PgPool,
    wrap: std::sync::Arc<dyn KeyWrap>,
}

impl fmt::Debug for PgSigningKeyStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PgSigningKeyStore")
            .field("wrap", &self.wrap)
            .finish_non_exhaustive()
    }
}

impl PgSigningKeyStore {
    /// A store over `db` (the auth pool), sealing private keys with `wrap`.
    #[must_use]
    pub fn new(db: PgPool, wrap: std::sync::Arc<dyn KeyWrap>) -> Self {
        Self { db, wrap }
    }

    /// Create `auth.signing_key` if it does not exist.
    ///
    /// # Errors
    ///
    /// [`AuthError::DatabaseError`] when the DDL fails.
    pub async fn init(&self) -> Result<()> {
        // raw_sql: a multi-statement DDL batch, which the prepared-statement path refuses.
        sqlx::raw_sql(SIGNING_KEY_SCHEMA_SQL)
            .execute(&self.db)
            .await
            .map_err(|e| db_error("create auth.signing_key", &e))?;
        Ok(())
    }

    /// Make sure the ring has an active key no older than [`ROTATION_PERIOD_SECS`] at
    /// `now`: create the first one, or rotate an old one. Returns the `kid` of a key it
    /// created, or `None` when the active key was kept.
    ///
    /// # Errors
    ///
    /// [`AuthError::DatabaseError`] when the database fails, and the [`KeyWrap`]'s error
    /// when sealing fails.
    pub async fn ensure_active(&self, now: u64) -> Result<Option<String>> {
        let mut tx = self.db.begin().await.map_err(|e| db_error("begin a key change", &e))?;
        lock_ring(&mut tx).await?;
        let created: Option<i64> =
            sqlx::query_scalar("SELECT created_at FROM auth.signing_key WHERE retired_at IS NULL")
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| db_error("read the active signing key", &e))?;
        let due = created.is_none_or(|created| {
            now.saturating_sub(created.cast_unsigned()) >= ROTATION_PERIOD_SECS
        });
        if !due {
            return Ok(None);
        }
        let kid = self.replace_active(&mut tx, now).await?;
        tx.commit().await.map_err(|e| db_error("commit a key change", &e))?;
        Ok(Some(kid))
    }

    /// Retire the active key at `now` and activate a new one, whatever their ages.
    /// Returns the new key's `kid`.
    ///
    /// # Errors
    ///
    /// As [`Self::ensure_active`].
    pub async fn rotate(&self, now: u64) -> Result<String> {
        let mut tx = self.db.begin().await.map_err(|e| db_error("begin a key change", &e))?;
        lock_ring(&mut tx).await?;
        let kid = self.replace_active(&mut tx, now).await?;
        tx.commit().await.map_err(|e| db_error("commit a key change", &e))?;
        Ok(kid)
    }

    /// Retire the active key (if any) and insert a new active one, inside `tx`.
    async fn replace_active(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        now: u64,
    ) -> Result<String> {
        let (key, pem) = SigningKey::generate()?;
        let sealed = self.wrap.seal(&pem, &aad(&key.kid))?;
        sqlx::query("UPDATE auth.signing_key SET retired_at = $1 WHERE retired_at IS NULL")
            .bind(now.cast_signed())
            .execute(&mut **tx)
            .await
            .map_err(|e| db_error("retire the active signing key", &e))?;
        sqlx::query(
            "INSERT INTO auth.signing_key (kid, alg, public_jwk, private_key_enc, created_at) \
             VALUES ($1, 'ES256', $2, $3, $4)",
        )
        .bind(&key.kid)
        .bind(&key.jwk)
        .bind(sealed)
        .bind(now.cast_signed())
        .execute(&mut **tx)
        .await
        .map_err(|e| db_error("insert a signing key", &e))?;
        Ok(key.kid)
    }

    /// The ring at `now`: the active key, opened, and every key to publish. A retired key
    /// is published while `retired_at + publish_window_secs` is still ahead of `now`;
    /// `publish_window_secs` is the longest a token it signed can still validate (the
    /// longest an access token is valid for, plus the validator's leeway).
    ///
    /// # Errors
    ///
    /// [`AuthError::ConfigError`] when the ring has no active key (call
    /// [`Self::ensure_active`] first), [`AuthError::DatabaseError`] when the read fails,
    /// and the [`KeyWrap`]'s error when the active key does not open.
    pub async fn load(&self, now: u64, publish_window_secs: u64) -> Result<KeyRing> {
        let rows = sqlx::query(
            "SELECT kid, public_jwk, private_key_enc, retired_at FROM auth.signing_key \
             WHERE retired_at IS NULL OR retired_at > $1 \
             ORDER BY retired_at DESC NULLS FIRST, created_at DESC",
        )
        .bind(now.saturating_sub(publish_window_secs).cast_signed())
        .fetch_all(&self.db)
        .await
        .map_err(|e| db_error("read the signing keys", &e))?;

        let mut active = None;
        let mut published = Vec::with_capacity(rows.len());
        for row in rows {
            let kid: String = row.get("kid");
            if row.get::<Option<i64>, _>("retired_at").is_none() {
                let sealed: Vec<u8> = row.get("private_key_enc");
                let pem = Zeroizing::new(self.wrap.open(&sealed, &aad(&kid))?);
                active = Some(SigningKey::from_pem(kid, &pem)?);
            }
            published.push(row.get::<Value, _>("public_jwk"));
        }
        let active = active.ok_or_else(|| AuthError::ConfigError {
            message: "auth.signing_key has no active key: the issuer must create one before \
                      it signs"
                .to_string(),
        })?;
        Ok(KeyRing { active, published })
    }

    /// Delete every retired key whose publish window has ended at `now`. Returns how
    /// many it deleted.
    ///
    /// # Errors
    ///
    /// [`AuthError::DatabaseError`] when the delete fails.
    pub async fn sweep(&self, now: u64, publish_window_secs: u64) -> Result<u64> {
        let swept = sqlx::query(
            "DELETE FROM auth.signing_key WHERE retired_at IS NOT NULL AND retired_at <= $1",
        )
        .bind(now.saturating_sub(publish_window_secs).cast_signed())
        .execute(&self.db)
        .await
        .map_err(|e| db_error("sweep the signing keys", &e))?
        .rows_affected();
        Ok(swept)
    }
}

/// Take the ring's lock for the rest of `tx`.
async fn lock_ring(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(RING_LOCK)
        .execute(&mut **tx)
        .await
        .map_err(|e| db_error("lock the signing keys", &e))?;
    Ok(())
}

#[cfg(test)]
mod tests;
