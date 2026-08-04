//! Resumable-upload session tracking (#369).
//!
//! One row in `_fraiseql_storage_uploads` per in-flight Tus upload. The session
//! is the durable half of a resumable upload: it records who owns the upload,
//! how many bytes have been accepted, which reserved metadata row the completed
//! object will confirm, and the opaque backend continuation state (S3 multipart
//! upload id + part etags, or the local temp-file marker). The bytes themselves
//! live in the backend's staging area until completion.
//!
//! Every state change is a conditional `UPDATE` pinned to the offset the caller
//! proved knowledge of, so two concurrent `PATCH`es cannot both append at the
//! same offset — one wins, the other gets the Tus `409 Conflict`.

#[cfg(test)]
mod tests;

use chrono::{DateTime, Utc};
use fraiseql_error::{FileError, FraiseQLError};
use sqlx::PgPool;

fn db_err(e: sqlx::Error) -> FraiseQLError {
    FraiseQLError::File(FileError::Backend {
        message: e.to_string(),
        source:  Some(Box::new(e)),
    })
}

/// A row from `_fraiseql_storage_uploads`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UploadSession {
    /// Session id — the value in the Tus `Location` URL.
    pub upload_id:           uuid::Uuid,
    /// Bucket the completed object lands in.
    pub bucket:              String,
    /// Object key the completed object lands under.
    pub key:                 String,
    /// MIME type declared at creation, validated against the bucket policy.
    pub content_type:        String,
    /// Total upload length declared at creation (Tus `Upload-Length`).
    pub declared_bytes:      i64,
    /// Bytes accepted so far (the Tus `Upload-Offset`).
    pub received_bytes:      i64,
    /// The creator; `PATCH`/`HEAD`/`DELETE` are scoped to this identity.
    pub owner_id:            Option<String>,
    /// The reserved `_fraiseql_storage_objects` row confirmed on completion.
    pub pk_storage_object:   i64,
    /// Whether creation reserved a NEW metadata row (a cancelled/expired
    /// session must release it) or claimed an existing object for overwrite
    /// (the row must survive the session).
    pub created_reservation: bool,
    /// Opaque per-backend continuation state.
    pub backend_state:       serde_json::Value,
    /// Session creation time.
    pub created_at:          DateTime<Utc>,
    /// After this instant the session is refused and reaped.
    pub expires_at:          DateTime<Utc>,
}

impl UploadSession {
    /// Whether the session's deadline has passed.
    #[must_use]
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at <= now
    }
}

/// Fields for creating a new upload session.
#[derive(Debug, Clone)]
pub struct NewUploadSession {
    /// Bucket the completed object lands in.
    pub bucket:              String,
    /// Object key the completed object lands under.
    pub key:                 String,
    /// Declared MIME type.
    pub content_type:        String,
    /// Declared total length (Tus `Upload-Length`).
    pub declared_bytes:      i64,
    /// Creating identity.
    pub owner_id:            Option<String>,
    /// The reserved metadata row.
    pub pk_storage_object:   i64,
    /// Whether the metadata row was newly reserved (vs an overwrite claim).
    pub created_reservation: bool,
    /// Initial backend continuation state.
    pub backend_state:       serde_json::Value,
    /// Session deadline.
    pub expires_at:          DateTime<Utc>,
}

/// Repository over `_fraiseql_storage_uploads`.
pub struct UploadSessionRepo {
    pool: PgPool,
}

impl UploadSessionRepo {
    /// Wrap a connection pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a session. Returns `Ok(None)` when an in-flight session already
    /// holds this `(bucket, key)` — the caller answers Tus' `409`, it does not
    /// clobber the existing upload.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` on database failure.
    pub async fn create(
        &self,
        row: &NewUploadSession,
    ) -> Result<Option<uuid::Uuid>, FraiseQLError> {
        let created: Option<(uuid::Uuid,)> = sqlx::query_as(
            "INSERT INTO _fraiseql_storage_uploads \
                 (bucket, key, content_type, declared_bytes, owner_id, pk_storage_object, \
                  created_reservation, backend_state, expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
             ON CONFLICT (bucket, key) DO NOTHING \
             RETURNING upload_id",
        )
        .bind(&row.bucket)
        .bind(&row.key)
        .bind(&row.content_type)
        .bind(row.declared_bytes)
        .bind(&row.owner_id)
        .bind(row.pk_storage_object)
        .bind(row.created_reservation)
        .bind(&row.backend_state)
        .bind(row.expires_at)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(created.map(|(id,)| id))
    }

    /// Load a session by id.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` on database failure.
    pub async fn get(&self, upload_id: uuid::Uuid) -> Result<Option<UploadSession>, FraiseQLError> {
        sqlx::query_as("SELECT * FROM _fraiseql_storage_uploads WHERE upload_id = $1")
            .bind(upload_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_err)
    }

    /// Advance a session past an accepted chunk, pinned to the offset the
    /// caller appended at. Returns `false` when the pinned offset no longer
    /// matches — a concurrent `PATCH` won the append and this one must answer
    /// the Tus `409`.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` on database failure.
    pub async fn advance(
        &self,
        upload_id: uuid::Uuid,
        from_offset: i64,
        to_offset: i64,
        backend_state: &serde_json::Value,
    ) -> Result<bool, FraiseQLError> {
        let result = sqlx::query(
            "UPDATE _fraiseql_storage_uploads SET \
                 received_bytes = $3, \
                 backend_state  = $4 \
             WHERE upload_id = $1 AND received_bytes = $2",
        )
        .bind(upload_id)
        .bind(from_offset)
        .bind(to_offset)
        .bind(backend_state)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(result.rows_affected() > 0)
    }

    /// Remove a session (completion, cancellation, or expiry reaping).
    ///
    /// (Reads do not lock: the [`advance`](Self::advance) conditional update is
    /// what serialises concurrent appends.)
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` on database failure.
    pub async fn delete(&self, upload_id: uuid::Uuid) -> Result<bool, FraiseQLError> {
        let result = sqlx::query("DELETE FROM _fraiseql_storage_uploads WHERE upload_id = $1")
            .bind(upload_id)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(result.rows_affected() > 0)
    }
}
