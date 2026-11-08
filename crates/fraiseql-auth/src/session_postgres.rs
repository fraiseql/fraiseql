//! PostgreSQL-backed [`SessionStore`] implementation.
use async_trait::async_trait;
use sqlx::{Row, postgres::PgPool};

use crate::{
    error::{AuthError, Result},
    session::{SessionData, SessionStore, TokenPair, generate_refresh_token, hash_token, unix_now},
};

/// How long after a refresh token is rotated a second use of it is still taken for the
/// client's own race (two tabs refreshing at once) rather than for a second holder (#1853).
///
/// Measured against the server-side `rotated_at`, never a client clock. A reuse at most this
/// many seconds after the rotation answers [`Refresh::AlreadyRotated`] and ends nothing; a
/// later one ends the whole session.
pub const ROTATION_GRACE_SECS: i64 = 10;

/// Lifetime of an access token minted by a refresh when `[auth.session] ttl_secs` is
/// unset: the hour local sign-in has always minted.
pub const DEFAULT_ACCESS_TTL_SECS: u64 = 3_600;

/// How long the sessions a [`PostgresSessionStore`] mints last (`[auth.session]`, #1853).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionLifetime {
    /// Lifetime of an access token. `None` keeps the expiry each sign-in path asks for.
    pub access_ttl_secs:   Option<u64>,
    /// How long a session can be refreshed, counted from sign-in. `None` mints sessions
    /// that end with their access token, as before #1853.
    pub absolute_ttl_secs: Option<u64>,
}

/// The outcome of [`PostgresSessionStore::refresh`].
#[derive(Debug)]
pub enum Refresh {
    /// The token was current: here is the session's next pair.
    Rotated(TokenPair),
    /// The token was rotated at most [`ROTATION_GRACE_SECS`] ago. Nothing is minted and
    /// nothing is ended: the client already holds the pair that rotation produced.
    AlreadyRotated,
    /// Refused: unknown, ended, past the session's limit, reused after the grace window
    /// (which ends the session), or the account is deactivated (which ends it too).
    Refused,
}

/// Default `iss` claim for minted access tokens (see [`PostgresSessionStore::with_token_claims`]).
pub const DEFAULT_TOKEN_ISSUER: &str = "fraiseql";
/// Default `aud` claim for minted access tokens (see [`PostgresSessionStore::with_token_claims`]).
pub const DEFAULT_TOKEN_AUDIENCE: &str = "fraiseql-api";
/// Default claim a minted access token carries the account's tenant under (#1450): the
/// default of `[fraiseql.tenancy] tenant_claim`. See [`PostgresSessionStore::with_tenant_claim`].
pub const DEFAULT_TENANT_CLAIM: &str = "tenant_id";

/// PostgreSQL-backed session store
pub struct PostgresSessionStore {
    db:             PgPool,
    /// Shared HMAC secret access tokens are signed with (HS256). The same secret must
    /// be given to the validating side (e.g. `Hs256AuthState`). `None` means signing is
    /// not configured and [`SessionStore::create_session`] will fail rather than mint
    /// an unverifiable token.
    hs256_secret:   Option<Vec<u8>>,
    /// `iss` claim minted into access tokens. Must match what the validating
    /// side expects — see [`Self::with_token_claims`].
    token_issuer:   String,
    /// `aud` claim minted into access tokens. Must match what the validating
    /// side expects — see [`Self::with_token_claims`].
    token_audience: String,
    /// The claim the account's tenant is minted under — see [`Self::with_tenant_claim`].
    tenant_claim:   String,
    /// `[auth.session]` — see [`Self::with_lifetime`].
    lifetime:       SessionLifetime,
}

/// Idempotent DDL for `auth.sessions`.
///
/// [`PostgresSessionStore::init`] applies it on its own connection, which is not safe
/// against a concurrent boot; `fraiseql-server` applies this text under its migration
/// lock instead.
pub const SESSIONS_SCHEMA_SQL: &str = concat!(
    claim_auth_schema!(),
    r"

-- One row per refresh token. `expires_at` is when that refresh token stops working, and
-- what the sweep deletes by. It means one of two things:
--   * a session that cannot be refreshed (no `[auth.session] absolute_ttl_secs`): the
--     access token's expiry, as it always has;
--   * a refreshable session (#1853): the session's absolute limit, counted from sign-in
--     and copied unchanged to every row its refreshes create. The access token's own
--     expiry is in the token, not here.
CREATE TABLE IF NOT EXISTS auth.sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id TEXT NOT NULL,
    refresh_token_hash TEXT NOT NULL UNIQUE,
    issued_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    revoked_at TIMESTAMPTZ
);

-- The account's tenant when the session was minted (#1450), for revocation and
-- audit. NULL for a platform account or a principal with no account row.
ALTER TABLE auth.sessions ADD COLUMN IF NOT EXISTS tenant_id UUID;

-- A refreshable session (#1853): every row a sign-in and its refreshes create shares a
-- family. NULL for a session that cannot be refreshed.
ALTER TABLE auth.sessions ADD COLUMN IF NOT EXISTS family_id UUID;
-- When this row's refresh token was exchanged for the next one, in Unix seconds on the
-- server's clock. A rotated token used again is answered by how long ago this was.
ALTER TABLE auth.sessions ADD COLUMN IF NOT EXISTS rotated_at BIGINT;
-- The `jti` of the access token minted with this row, so logout can deny it where
-- `[security.token_revocation]` is configured.
ALTER TABLE auth.sessions ADD COLUMN IF NOT EXISTS access_jti TEXT;
CREATE INDEX IF NOT EXISTS idx_sessions_family_id ON auth.sessions(family_id);

CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON auth.sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON auth.sessions(expires_at);
CREATE INDEX IF NOT EXISTS idx_sessions_revoked_at ON auth.sessions(revoked_at);
"
);

impl PostgresSessionStore {
    /// Create a new PostgreSQL session store **without** JWT signing configured.
    ///
    /// Refresh-token bookkeeping ([`SessionStore::get_session`],
    /// [`SessionStore::revoke_session`], [`SessionStore::revoke_all_sessions`]) works,
    /// but [`SessionStore::create_session`] will return
    /// [`AuthError::ConfigError`] because there is no key to sign the access token
    /// with. Use [`Self::with_hs256_secret`] for a store that can issue sessions.
    #[must_use]
    pub fn new(db: PgPool) -> Self {
        Self {
            db,
            hs256_secret: None,
            token_issuer: DEFAULT_TOKEN_ISSUER.to_string(),
            token_audience: DEFAULT_TOKEN_AUDIENCE.to_string(),
            tenant_claim: DEFAULT_TENANT_CLAIM.to_string(),
            lifetime: SessionLifetime::default(),
        }
    }

    /// Create a new PostgreSQL session store with HS256 (HMAC) JWT signing.
    ///
    /// The secret is retained for the life of the store and **must** be the same
    /// secret configured on the validating side, otherwise the issued tokens will
    /// not verify.
    ///
    /// # Arguments
    /// * `db` - PostgreSQL connection pool
    /// * `secret` - Shared HMAC secret (use at least 32 bytes of entropy)
    #[must_use]
    pub fn with_hs256_secret(db: PgPool, secret: Vec<u8>) -> Self {
        Self {
            hs256_secret: Some(secret),
            ..Self::new(db)
        }
    }

    /// Mint the account's tenant under `claim`: the schema's
    /// `[fraiseql.tenancy] tenant_claim`, so a FraiseQL-minted token's tenant reaches
    /// `SecurityContext::tenant_id` exactly as an external IdP's does (#1450).
    #[must_use]
    pub fn with_tenant_claim(mut self, claim: impl Into<String>) -> Self {
        self.tenant_claim = claim.into();
        self
    }

    /// Mint sessions with `lifetime` (`[auth.session]`, #1853).
    ///
    /// Its `access_ttl_secs` replaces the expiry a sign-in path passes to
    /// [`SessionStore::create_session`]. Its `absolute_ttl_secs` makes the sessions
    /// refreshable through [`Self::refresh`]. A store minting anonymous guest sessions is
    /// built without it, so they keep their own lifetime.
    #[must_use]
    pub const fn with_lifetime(mut self, lifetime: SessionLifetime) -> Self {
        self.lifetime = lifetime;
        self
    }

    /// Set the `iss` / `aud` claims minted into access tokens.
    ///
    /// The defaults (`fraiseql` / `fraiseql-api`) only validate against a
    /// validator configured with exactly those values. A deployment whose
    /// `[auth_hs256]` declares its own issuer/audience **must** mint matching
    /// claims, or every login "succeeds" and then 401s on the first validated
    /// request — the mint/validate drift #368's server mount surfaced.
    #[must_use]
    pub fn with_token_claims(
        mut self,
        issuer: impl Into<String>,
        audience: impl Into<String>,
    ) -> Self {
        self.token_issuer = issuer.into();
        self.token_audience = audience.into();
        self
    }

    /// Initialize the sessions table
    ///
    /// This should be called once during server startup to ensure the table exists.
    ///
    /// # Errors
    /// Returns error if table creation fails
    pub async fn init(&self) -> Result<()> {
        // raw_sql: this is a multi-statement DDL batch, which the prepared-
        // statement path (`sqlx::query`) refuses at the protocol level.
        sqlx::raw_sql(SESSIONS_SCHEMA_SQL).execute(&self.db).await.map_err(|e| {
            AuthError::DatabaseError {
                message: format!("Failed to initialize sessions table: {}", e),
            }
        })?;

        Ok(())
    }

    /// Generate a JWT access token with the configured signing key.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::ConfigError`] when no signing key is configured. Minting
    /// a token nobody can verify is worse than failing: it produces a login that
    /// "succeeds" and then 401s on every subsequent request.
    /// Returns the token and its `jti`.
    fn generate_access_token(
        &self,
        user_id: &str,
        tenant: Option<uuid::Uuid>,
        expires_in: u64,
    ) -> Result<(String, String)> {
        // SECURITY: Propagate clock errors; unwrap_or_default would produce iat=0.
        let now = unix_now()?;

        let exp = now + expires_in;

        let mut claims = crate::Claims {
            sub: user_id.to_string(),
            iat: now,
            exp,
            nbf: None,
            iss: self.token_issuer.clone(),
            aud: vec![self.token_audience.clone()],
            extra: std::collections::HashMap::new(),
        };

        // Add JTI (JWT ID) for uniqueness
        let jti = uuid::Uuid::new_v4().to_string();
        claims.extra.insert("jti".to_string(), serde_json::json!(jti));

        // The account's tenant, under the schema's tenant claim (#1450). The simple form
        // (32 hex digits) is the one rendering that is both a valid tenant key (registry,
        // `X-Tenant-ID`, schema mode) and a valid `::uuid` cast for the auth tables' RLS.
        if let Some(tenant) = tenant {
            claims.extra.insert(
                self.tenant_claim.clone(),
                serde_json::json!(tenant.as_simple().to_string()),
            );
        }

        match &self.hs256_secret {
            Some(secret) => Ok((crate::jwt::generate_hs256_token(&claims, secret)?, jti)),
            None => Err(AuthError::ConfigError {
                message: "JWT signing is not configured for this PostgresSessionStore — \
                          construct it with with_hs256_secret. Refusing to issue an access \
                          token that no validator could verify."
                    .to_string(),
            }),
        }
    }
}

/// The account a session is minted for: refused if SCIM deactivated it (#946), and its
/// tenant otherwise (#1450).
///
/// The tenant comes from the account row, which only a tenant authority can set (#1088),
/// and never from the request: no caller can choose the tenant a session carries. Every
/// credential path converges on `create_session`, so every one of them gets it.
///
/// # Why a missing table is allowed through
///
/// `auth.tb_user` belongs to the account store, which a session-only embedder need not
/// have. Treating its absence as a refusal would break those deployments closed for a
/// reason unrelated to offboarding. So the *specific* Postgres `undefined_table` code is
/// the one condition that passes; every other error — including a readable row that says
/// `active = false` — refuses. A deployment running SCIM always has the table (SCIM
/// provisions into it), so the tolerated branch is unreachable there.
///
/// A user with no row is allowed, in no tenant: anonymous sessions and JWT-only
/// principals live in a different identity space and were never provisioned.
async fn account_for_session(db: &PgPool, user_id: &str) -> Result<Option<uuid::Uuid>> {
    let row = sqlx::query("SELECT active, tenant_id FROM auth.tb_user WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(db)
        .await;

    let (active, tenant): (Option<bool>, Option<uuid::Uuid>) = match row {
        Ok(Some(row)) => (Some(row.get("active")), row.get("tenant_id")),
        Ok(None) => (None, None),
        Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("42P01") => {
            // No account store in this deployment; nothing was ever provisioned.
            return Ok(None);
        },
        Err(e) => {
            return Err(AuthError::DatabaseError {
                message: format!("Failed to check account activation: {e}"),
            });
        },
    };

    if active == Some(false) {
        tracing::warn!(
            user_id = %user_id,
            "refused a session for a deactivated account (SCIM active = false)"
        );
        return Err(AuthError::AccountDeactivated);
    }
    Ok(tenant)
}

impl PostgresSessionStore {
    /// Remove the session rows past their `expires_at`.
    ///
    /// A rotated row of a refreshable session carries the session's limit, so it stays
    /// until the session itself ends: reuse detection needs it that long.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] if the delete fails.
    pub async fn sweep_expired(&self) -> Result<u64> {
        let now = unix_now()?;
        let swept = sqlx::query("DELETE FROM auth.sessions WHERE expires_at < $1")
            .bind(now.cast_signed())
            .execute(&self.db)
            .await
            .map_err(|e| AuthError::DatabaseError {
                message: format!("Failed to sweep sessions: {e}"),
            })?
            .rows_affected();
        Ok(swept)
    }

    /// Exchange `refresh_token` for the session's next pair (#1853).
    ///
    /// One transaction, holding the token's row: concurrent uses of one token cannot both
    /// rotate it. The account is read again, so a deactivated account is refused and the
    /// session ended, and the new access token carries the account's tenant as it is now
    /// (#1450). The second factor is not asked again: the session passed it at sign-in.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] when the database fails, and the signing error
    /// when no key is configured.
    pub async fn refresh(&self, refresh_token: &str) -> Result<Refresh> {
        if self.lifetime.absolute_ttl_secs.is_none() {
            return Ok(Refresh::Refused);
        }
        let now = unix_now()?;
        let mut tx = self.db.begin().await.map_err(|e| db_error("begin a refresh", &e))?;
        let Some(row) = sqlx::query(
            r"
            SELECT id, user_id, family_id, expires_at, rotated_at, revoked_at IS NOT NULL AS revoked
            FROM auth.sessions
            WHERE refresh_token_hash = $1
            FOR UPDATE
            ",
        )
        .bind(hash_token(refresh_token))
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| db_error("read a session", &e))?
        else {
            return Ok(Refresh::Refused);
        };
        let id: uuid::Uuid = row.get("id");
        let user_id: String = row.get("user_id");
        let family: Option<uuid::Uuid> = row.get("family_id");
        let limit: i64 = row.get("expires_at");
        let rotated_at: Option<i64> = row.get("rotated_at");
        let revoked: bool = row.get("revoked");

        let Some(family) = family else {
            // Minted when the session could not be refreshed.
            return Ok(Refresh::Refused);
        };
        if revoked {
            return Ok(Refresh::Refused);
        }
        if let Some(rotated_at) = rotated_at {
            if now.cast_signed() - rotated_at <= ROTATION_GRACE_SECS {
                return Ok(Refresh::AlreadyRotated);
            }
            end_family(&mut tx, family).await?;
            tx.commit().await.map_err(|e| db_error("end a session", &e))?;
            tracing::warn!(
                user_id = %user_id,
                "a rotated refresh token was used again after the grace window; \
                 the session is ended"
            );
            return Ok(Refresh::Refused);
        }
        if now.cast_signed() >= limit {
            return Ok(Refresh::Refused);
        }
        let tenant = match account_for_session(&self.db, &user_id).await {
            Ok(tenant) => tenant,
            Err(AuthError::AccountDeactivated) => {
                end_family(&mut tx, family).await?;
                tx.commit().await.map_err(|e| db_error("end a session", &e))?;
                return Ok(Refresh::Refused);
            },
            Err(e) => return Err(e),
        };

        sqlx::query("UPDATE auth.sessions SET rotated_at = $2 WHERE id = $1")
            .bind(id)
            .bind(now.cast_signed())
            .execute(&mut *tx)
            .await
            .map_err(|e| db_error("rotate a session", &e))?;
        let pair = self
            .insert_session(
                &mut tx,
                &user_id,
                tenant,
                now,
                self.access_expiry(now, now),
                Some((family, limit.cast_unsigned())),
            )
            .await?;
        tx.commit().await.map_err(|e| db_error("commit a refresh", &e))?;
        Ok(Refresh::Rotated(pair))
    }

    /// End the session `refresh_token` belongs to: every row of its family (#1853).
    ///
    /// Returns the `jti` of every access token the session minted, for a caller that can
    /// deny them. Ending the rows alone does not end an access token already issued: the
    /// validator never reads this table. An unknown token ends nothing and returns none.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] when the database fails.
    pub async fn end_session(&self, refresh_token: &str) -> Result<Vec<String>> {
        let mut tx = self.db.begin().await.map_err(|e| db_error("begin a logout", &e))?;
        let Some(row) =
            sqlx::query("SELECT id, family_id FROM auth.sessions WHERE refresh_token_hash = $1")
                .bind(hash_token(refresh_token))
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| db_error("read a session", &e))?
        else {
            return Ok(Vec::new());
        };
        let id: uuid::Uuid = row.get("id");
        let family: Option<uuid::Uuid> = row.get("family_id");
        let jtis: Vec<Option<String>> = sqlx::query_scalar(
            r"
            UPDATE auth.sessions
            SET revoked_at = COALESCE(revoked_at, NOW())
            WHERE id = $1 OR family_id = $2
            RETURNING access_jti
            ",
        )
        .bind(id)
        .bind(family)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| db_error("end a session", &e))?;
        tx.commit().await.map_err(|e| db_error("commit a logout", &e))?;
        Ok(jtis.into_iter().flatten().collect())
    }

    /// How long an access token minted by a refreshable store lives: `ttl_secs`, else
    /// one hour. `None` when this store does not mint refreshable sessions.
    #[must_use]
    pub const fn refreshable_access_ttl_secs(&self) -> Option<u64> {
        match self.lifetime {
            SessionLifetime {
                absolute_ttl_secs: None,
                ..
            } => None,
            SessionLifetime {
                access_ttl_secs: Some(ttl),
                ..
            } => Some(ttl),
            SessionLifetime { .. } => Some(DEFAULT_ACCESS_TTL_SECS),
        }
    }

    /// The access token's expiry: the configured lifetime; an hour for a refreshable
    /// session that configures none; otherwise the expiry the sign-in path asked for.
    const fn access_expiry(&self, now: u64, requested: u64) -> u64 {
        match self.lifetime {
            SessionLifetime {
                access_ttl_secs: Some(ttl),
                ..
            } => now + ttl,
            SessionLifetime {
                absolute_ttl_secs: Some(_),
                ..
            } => now + DEFAULT_ACCESS_TTL_SECS,
            SessionLifetime { .. } => requested,
        }
    }

    /// Mint the session: sign the access token, then record the refresh token's hash.
    /// Reached only after the account check, and the second factor where one is owed.
    async fn mint(
        &self,
        user_id: &str,
        tenant: Option<uuid::Uuid>,
        expires_at: u64,
    ) -> Result<TokenPair> {
        // SECURITY: Propagate clock errors; unwrap_or_default would produce issued_at=0.
        let now = unix_now()?;
        let access_expires_at = self.access_expiry(now, expires_at);
        let family = self
            .lifetime
            .absolute_ttl_secs
            .map(|absolute| (uuid::Uuid::new_v4(), now + absolute));
        let mut conn = self.db.acquire().await.map_err(|e| db_error("acquire a connection", &e))?;
        self.insert_session(&mut conn, user_id, tenant, now, access_expires_at, family)
            .await
    }

    /// Sign an access token expiring at `access_expires_at` and record its session row.
    ///
    /// `family` is the session's family and limit when it can be refreshed; the row then
    /// expires at the limit. Without it the row expires with the access token.
    async fn insert_session(
        &self,
        conn: &mut sqlx::PgConnection,
        user_id: &str,
        tenant: Option<uuid::Uuid>,
        now: u64,
        access_expires_at: u64,
        family: Option<(uuid::Uuid, u64)>,
    ) -> Result<TokenPair> {
        let refresh_token = generate_refresh_token();
        let refresh_token_hash = hash_token(&refresh_token);
        let expires_in = access_expires_at.saturating_sub(now);

        // Mint the access token before writing the session row: if signing is not
        // configured this fails, and doing it first keeps an orphan row out of
        // auth.sessions for a session that was never handed to anyone.
        let (access_token, jti) = self.generate_access_token(user_id, tenant, expires_in)?;
        let row_expires_at = family.map_or(access_expires_at, |(_, limit)| limit);

        sqlx::query(
            r"
            INSERT INTO auth.sessions
            (user_id, refresh_token_hash, issued_at, expires_at, tenant_id, family_id, access_jti)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ",
        )
        .bind(user_id)
        .bind(&refresh_token_hash)
        .bind(now.cast_signed())
        .bind(row_expires_at.cast_signed())
        .bind(tenant)
        .bind(family.map(|(family, _)| family))
        .bind(&jti)
        .execute(&mut *conn)
        .await
        .map_err(|e| {
            if e.to_string().contains("duplicate key") {
                AuthError::SessionError {
                    message: "Refresh token already exists".to_string(),
                }
            } else {
                AuthError::DatabaseError {
                    message: format!("Failed to create session: {}", e),
                }
            }
        })?;

        Ok(TokenPair {
            access_token,
            refresh_token,
            expires_in,
        })
    }
}

/// End every row of a session family.
async fn end_family(conn: &mut sqlx::PgConnection, family: uuid::Uuid) -> Result<()> {
    sqlx::query(
        "UPDATE auth.sessions SET revoked_at = NOW() WHERE family_id = $1 AND revoked_at IS NULL",
    )
    .bind(family)
    .execute(&mut *conn)
    .await
    .map_err(|e| db_error("end a session", &e))?;
    Ok(())
}

fn db_error(doing: &str, e: &sqlx::Error) -> AuthError {
    AuthError::DatabaseError {
        message: format!("Failed to {doing}: {e}"),
    }
}

// Reason: SessionStore is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl SessionStore for PostgresSessionStore {
    async fn create_session(&self, user_id: &str, expires_at: u64) -> Result<TokenPair> {
        // Offboarding must block every credential, not just the one the IdP owns (#946).
        // This is the single place every credential path converges on — password login,
        // social callback, OTP, SAML ACS, an anonymous session's upgrade all end here —
        // which is why the check lives here rather than in each of them. The second factor
        // is required here for the same reason: a path that skipped it would mint a full
        // session from the first factor alone.
        let tenant = account_for_session(&self.db, user_id).await?;
        if let Some(challenge) = crate::totp_mfa::postgres::PgMfaStore::new(self.db.clone())
            .challenge_if_enrolled(user_id)
            .await?
        {
            return Err(AuthError::SecondFactorRequired { challenge });
        }
        self.mint(user_id, tenant, expires_at).await
    }

    async fn create_session_after_second_factor(
        &self,
        passed: crate::totp_mfa::SecondFactorPassed,
        expires_at: u64,
    ) -> Result<TokenPair> {
        let tenant = account_for_session(&self.db, passed.user_id()).await?;
        self.mint(passed.user_id(), tenant, expires_at).await
    }

    async fn get_session(&self, refresh_token_hash: &str) -> Result<SessionData> {
        let row = sqlx::query(
            r"
            SELECT user_id, issued_at, expires_at, refresh_token_hash
            FROM auth.sessions
            WHERE refresh_token_hash = $1 AND revoked_at IS NULL
            ",
        )
        .bind(refresh_token_hash)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AuthError::DatabaseError {
            message: format!("Failed to get session: {}", e),
        })?
        .ok_or(AuthError::TokenNotFound)?;

        let user_id: String = row.get("user_id");
        let issued_at: i64 = row.get("issued_at");
        let expires_at: i64 = row.get("expires_at");
        let refresh_token_hash: String = row.get("refresh_token_hash");

        Ok(SessionData {
            user_id,
            issued_at: issued_at.cast_unsigned(),
            expires_at: expires_at.cast_unsigned(),
            refresh_token_hash,
        })
    }

    async fn revoke_session(&self, refresh_token_hash: &str) -> Result<()> {
        let result = sqlx::query(
            r"
            UPDATE auth.sessions
            SET revoked_at = NOW()
            WHERE refresh_token_hash = $1 AND revoked_at IS NULL
            ",
        )
        .bind(refresh_token_hash)
        .execute(&self.db)
        .await
        .map_err(|e| AuthError::DatabaseError {
            message: format!("Failed to revoke session: {}", e),
        })?;

        if result.rows_affected() == 0 {
            return Err(AuthError::SessionError {
                message: "Session not found or already revoked".to_string(),
            });
        }

        Ok(())
    }

    async fn revoke_all_sessions(&self, user_id: &str) -> Result<()> {
        sqlx::query(
            r"
            UPDATE auth.sessions
            SET revoked_at = NOW()
            WHERE user_id = $1 AND revoked_at IS NULL
            ",
        )
        .bind(user_id)
        .execute(&self.db)
        .await
        .map_err(|e| AuthError::DatabaseError {
            message: format!("Failed to revoke all sessions: {}", e),
        })?;

        Ok(())
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
