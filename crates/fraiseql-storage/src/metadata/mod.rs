//! Object metadata storage and retrieval.
//!
//! Tracks uploaded objects in a PostgreSQL table (`_fraiseql_storage_objects`)
//! for RLS enforcement, listing, and lifecycle management.

#[cfg(test)]
mod tests;

use chrono::{DateTime, Utc};
use fraiseql_error::{FileError, FraiseQLError};
use sqlx::PgPool;

use crate::backend::types::ObjectInfo;

/// Escape the PostgreSQL `LIKE` metacharacters (`%`, `_`) and the escape
/// character (`\`) in a client-supplied key prefix so it is matched as a
/// literal string.
///
/// Intended for use with an explicit `ESCAPE '\'` clause. The backslash is
/// replaced first so it cannot accidentally escape a metacharacter introduced
/// by a later replacement.
fn escape_like_prefix(prefix: &str) -> String {
    prefix.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

/// A row from the `_fraiseql_storage_objects` table.
#[derive(Debug, Clone)]
pub struct StorageMetadataRow {
    /// Primary key.
    pub pk_storage_object: i64,
    /// Bucket name.
    pub bucket:            String,
    /// Object key (path within bucket).
    pub key:               String,
    /// MIME content type.
    pub content_type:      String,
    /// Object size in bytes.
    pub size_bytes:        i64,
    /// Entity tag for integrity verification.
    pub etag:              Option<String>,
    /// Owner identifier (user sub claim).
    pub owner_id:          Option<String>,
    /// An upload is in flight for this key and has not been confirmed.
    ///
    /// Set when a presigned upload URL is signed (#866): the client writes the
    /// object straight to the backend, so the server records ownership up front
    /// and leaves `size_bytes` / `etag` as placeholders until a read reconciles
    /// them against the stored object.
    pub pending:           bool,
    /// Row creation time.
    pub created_at:        DateTime<Utc>,
    /// Last update time.
    pub updated_at:        DateTime<Utc>,
    /// When this object stops being servable, if ever (#974).
    ///
    /// Read by the [`require_unexpired`](crate::PolicyRule::require_unexpired)
    /// policy condition. `None` means no expiry was set, which that condition
    /// treats as a denial rather than as "never expires".
    pub expires_at:        Option<DateTime<Utc>>,
    /// User-defined metadata, read by the
    /// [`require_metadata`](crate::PolicyRule::require_metadata) policy
    /// condition (#1099).
    ///
    /// Written only by a caller holding
    /// [`PolicyMethod::SetMetadata`](crate::PolicyMethod::SetMetadata) — never
    /// by `write` or `overwrite`. That is what lets a policy match on it
    /// without the matched value being something the gated caller could have
    /// chosen.
    ///
    /// Empty for every object that has never had metadata set, and an absent
    /// key fails `require_metadata`, so the column cannot widen access on rows
    /// that predate it.
    pub metadata:          crate::policy::MetadataValues,
}

/// Data required to insert a new storage object record.
#[derive(Debug, Clone)]
pub struct NewStorageObject {
    /// Bucket name.
    pub bucket:       String,
    /// Object key (path within bucket).
    pub key:          String,
    /// MIME content type.
    pub content_type: String,
    /// Object size in bytes.
    pub size_bytes:   i64,
    /// Entity tag for integrity verification.
    pub etag:         Option<String>,
    /// Owner identifier (user sub claim).
    pub owner_id:     Option<String>,
}

/// Storage metadata repository backed by PostgreSQL.
pub struct StorageMetadataRepo {
    pool: PgPool,
}

impl StorageMetadataRepo {
    /// Create a new repository wrapping the given connection pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Insert a new object metadata row, returning the generated primary key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if the database query fails
    /// (e.g. duplicate `(bucket, key)` pair).
    pub async fn insert(&self, row: &NewStorageObject) -> Result<i64, FraiseQLError> {
        let (pk,): (i64,) = sqlx::query_as(
            "INSERT INTO _fraiseql_storage_objects \
                 (bucket, key, content_type, size_bytes, etag, owner_id) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             RETURNING pk_storage_object",
        )
        .bind(&row.bucket)
        .bind(&row.key)
        .bind(&row.content_type)
        .bind(row.size_bytes)
        .bind(&row.etag)
        .bind(&row.owner_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            FraiseQLError::File(FileError::Backend {
                message: e.to_string(),
                source:  Some(Box::new(e)),
            })
        })?;

        Ok(pk)
    }

    /// Look up an object by bucket and key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if the database query fails.
    pub async fn get(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<Option<StorageMetadataRow>, FraiseQLError> {
        let row = sqlx::query_as::<_, MetadataQueryRow>(
            "SELECT pk_storage_object, bucket, key, content_type, \
                    size_bytes, etag, owner_id, pending, created_at, updated_at, expires_at, \
                    metadata \
             FROM _fraiseql_storage_objects \
             WHERE bucket = $1 AND key = $2",
        )
        .bind(bucket)
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            FraiseQLError::File(FileError::Backend {
                message: e.to_string(),
                source:  Some(Box::new(e)),
            })
        })?;

        Ok(row.map(Into::into))
    }

    /// Delete an object metadata row by bucket and key.
    ///
    /// Returns `true` if a row was actually deleted, `false` if no matching row existed.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if the database query fails.
    pub async fn delete(&self, bucket: &str, key: &str) -> Result<bool, FraiseQLError> {
        let result =
            sqlx::query("DELETE FROM _fraiseql_storage_objects WHERE bucket = $1 AND key = $2")
                .bind(bucket)
                .bind(key)
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    FraiseQLError::File(FileError::Backend {
                        message: e.to_string(),
                        source:  Some(Box::new(e)),
                    })
                })?;

        Ok(result.rows_affected() > 0)
    }

    /// List objects in a bucket, optionally filtered by key prefix.
    ///
    /// Results are ordered by key ascending. Use `limit` and `offset` for pagination.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if the database query fails.
    pub async fn list(
        &self,
        bucket: &str,
        prefix: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<StorageMetadataRow>, FraiseQLError> {
        let rows = match prefix {
            Some(pfx) => {
                // #339: `prefix` is a literal key prefix, not a LIKE pattern.
                // Escape the LIKE metacharacters in the client-supplied value
                // and pin the escape character so `%` / `_` / `\` match
                // literally and cannot be used to widen the match.
                sqlx::query_as::<_, MetadataQueryRow>(
                    "SELECT pk_storage_object, bucket, key, content_type, \
                            size_bytes, etag, owner_id, pending, created_at, updated_at, expires_at, \
                    metadata \
                     FROM _fraiseql_storage_objects \
                     WHERE bucket = $1 AND key LIKE $2 ESCAPE '\\' \
                     ORDER BY key ASC \
                     LIMIT $3 OFFSET $4",
                )
                .bind(bucket)
                .bind(format!("{}%", escape_like_prefix(pfx)))
                .bind(i64::from(limit))
                .bind(i64::from(offset))
                .fetch_all(&self.pool)
                .await
            },
            None => {
                sqlx::query_as::<_, MetadataQueryRow>(
                    "SELECT pk_storage_object, bucket, key, content_type, \
                            size_bytes, etag, owner_id, pending, created_at, updated_at, expires_at, \
                    metadata \
                     FROM _fraiseql_storage_objects \
                     WHERE bucket = $1 \
                     ORDER BY key ASC \
                     LIMIT $2 OFFSET $3",
                )
                .bind(bucket)
                .bind(i64::from(limit))
                .bind(i64::from(offset))
                .fetch_all(&self.pool)
                .await
            },
        }
        .map_err(|e| {
            FraiseQLError::File(FileError::Backend {
                message: e.to_string(),
                source:  Some(Box::new(e)),
            })
        })?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Insert or update an object metadata row (upsert on `(bucket, key)`).
    ///
    /// On conflict, updates `content_type`, `size_bytes`, `etag`, and `updated_at`.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if the database query fails.
    pub async fn upsert(&self, row: &NewStorageObject) -> Result<i64, FraiseQLError> {
        let (pk,): (i64,) = sqlx::query_as(
            "INSERT INTO _fraiseql_storage_objects \
                 (bucket, key, content_type, size_bytes, etag, owner_id) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (bucket, key) DO UPDATE SET \
                 content_type = EXCLUDED.content_type, \
                 size_bytes   = EXCLUDED.size_bytes, \
                 etag         = EXCLUDED.etag, \
                 pending      = FALSE, \
                 updated_at   = now() \
             RETURNING pk_storage_object",
        )
        .bind(&row.bucket)
        .bind(&row.key)
        .bind(&row.content_type)
        .bind(row.size_bytes)
        .bind(&row.etag)
        .bind(&row.owner_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            FraiseQLError::File(FileError::Backend {
                message: e.to_string(),
                source:  Some(Box::new(e)),
            })
        })?;

        Ok(pk)
    }

    /// Claim `(bucket, key)` for an upload that will bypass the server.
    ///
    /// A presigned PUT sends the bytes straight to the object store, so the
    /// server's only chance to record who owns the resulting object is *before*
    /// it signs the URL (#866). Without this the object had no metadata row at
    /// all: it was unreadable through `GET`, invisible to `list`, and — because
    /// `can_write_object` reads a missing row as "create" — any authenticated
    /// user could take it over.
    ///
    /// `expected_pk` carries the caller's view of the current row, so the claim
    /// is safe against a concurrent writer:
    /// - `None` — the caller saw no row. Inserts, and returns `Ok(None)` if a row appeared in the
    ///   meantime rather than overwriting whoever won.
    /// - `Some(pk)` — the caller saw that row and its authorization has already been checked
    ///   against it. Re-marks it pending, and returns `Ok(None)` if it has since changed identity.
    ///
    /// Returns the claimed row's primary key, or `Ok(None)` when the claim lost
    /// a race and the caller must re-authorize.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if the database query fails.
    pub async fn reserve(
        &self,
        row: &NewStorageObject,
        expected_pk: Option<i64>,
    ) -> Result<Option<i64>, FraiseQLError> {
        let query = match expected_pk {
            // Create: lose the race rather than clobber the winner.
            None => sqlx::query_as::<_, (i64,)>(
                "INSERT INTO _fraiseql_storage_objects \
                     (bucket, key, content_type, size_bytes, etag, owner_id, pending) \
                 VALUES ($1, $2, $3, 0, NULL, $4, TRUE) \
                 ON CONFLICT (bucket, key) DO NOTHING \
                 RETURNING pk_storage_object",
            )
            .bind(&row.bucket)
            .bind(&row.key)
            .bind(&row.content_type)
            .bind(&row.owner_id),
            // Overwrite: the caller proved owner-or-admin against exactly this
            // row, so pin the update to it. `owner_id` is deliberately left
            // alone — an admin re-signing does not become the owner.
            Some(pk) => sqlx::query_as::<_, (i64,)>(
                "UPDATE _fraiseql_storage_objects SET \
                     content_type = $3, \
                     pending      = TRUE, \
                     updated_at   = now() \
                 WHERE bucket = $1 AND key = $2 AND pk_storage_object = $4 \
                 RETURNING pk_storage_object",
            )
            .bind(&row.bucket)
            .bind(&row.key)
            .bind(&row.content_type)
            .bind(pk),
        };

        let claimed = query.fetch_optional(&self.pool).await.map_err(|e| {
            FraiseQLError::File(FileError::Backend {
                message: e.to_string(),
                source:  Some(Box::new(e)),
            })
        })?;

        Ok(claimed.map(|(pk,)| pk))
    }

    /// Release a reservation that was never used.
    ///
    /// Called when signing fails after [`reserve`](Self::reserve) claimed a
    /// *new* row: leaving it behind would hold the key against its owner
    /// forever for an upload that can never happen. Scoped to the primary key
    /// and to `pending`, so it can never delete a confirmed object.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if the database query fails.
    pub async fn release_reservation(&self, pk: i64) -> Result<bool, FraiseQLError> {
        let result = sqlx::query(
            "DELETE FROM _fraiseql_storage_objects WHERE pk_storage_object = $1 AND pending",
        )
        .bind(pk)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            FraiseQLError::File(FileError::Backend {
                message: e.to_string(),
                source:  Some(Box::new(e)),
            })
        })?;

        Ok(result.rows_affected() > 0)
    }

    /// Reconcile a claimed row against the object that actually landed.
    ///
    /// A reservation records ownership but cannot record size or etag, because
    /// the bytes never passed through the server. The first successful read
    /// has them, so it settles the row. Scoped to the primary key so a
    /// concurrent server-side upload's own metadata is never overwritten by a
    /// stale read.
    ///
    /// # Errors
    ///
    /// Replace an object's user-defined metadata (#1099).
    ///
    /// Wholesale, like a policy push: the supplied map *is* the object's
    /// metadata afterwards. There is no per-key merge, because a merge makes
    /// "what metadata does this object carry" a question you answer by
    /// replaying history rather than by reading one value — and that value
    /// decides access.
    ///
    /// Authorization is the caller's business and happens before this: the
    /// route consults [`can_set_metadata`](crate::StorageRlsEvaluator::can_set_metadata),
    /// which is a grant no `write` or `overwrite` rule implies. The map must
    /// already have been through
    /// [`validate_metadata`](crate::policy::validate_metadata).
    ///
    /// Returns `false` when no such object exists, so a caller can answer `404`
    /// rather than reporting a write that touched nothing.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if the database query fails.
    pub async fn set_metadata(
        &self,
        bucket: &str,
        key: &str,
        metadata: &crate::policy::MetadataValues,
    ) -> Result<bool, FraiseQLError> {
        let result = sqlx::query(
            "UPDATE _fraiseql_storage_objects SET \
                 metadata   = $3, \
                 updated_at = now() \
             WHERE bucket = $1 AND key = $2",
        )
        .bind(bucket)
        .bind(key)
        .bind(sqlx::types::Json(metadata))
        .execute(&self.pool)
        .await
        .map_err(|e| {
            FraiseQLError::File(FileError::Backend {
                message: e.to_string(),
                source:  Some(Box::new(e)),
            })
        })?;
        Ok(result.rows_affected() > 0)
    }

    /// Reconcile a claimed row against the object that actually landed.
    ///
    /// A reservation records ownership but cannot record size or etag, because
    /// the bytes never passed through the server. The first successful read
    /// has them, so it settles the row. Scoped to the primary key so a
    /// concurrent server-side upload's own metadata is never overwritten by a
    /// stale read.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if the database query fails.
    pub async fn confirm(
        &self,
        pk: i64,
        size_bytes: i64,
        etag: Option<&str>,
    ) -> Result<(), FraiseQLError> {
        sqlx::query(
            "UPDATE _fraiseql_storage_objects SET \
                 size_bytes = $2, \
                 etag       = $3, \
                 pending    = FALSE, \
                 updated_at = now() \
             WHERE pk_storage_object = $1 AND pending",
        )
        .bind(pk)
        .bind(size_bytes)
        .bind(etag)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            FraiseQLError::File(FileError::Backend {
                message: e.to_string(),
                source:  Some(Box::new(e)),
            })
        })?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Internal query row type for sqlx::FromRow derive
// ---------------------------------------------------------------------------

/// Internal row type that derives `sqlx::FromRow`.
///
/// Kept separate from the public `StorageMetadataRow` to avoid leaking the
/// sqlx dependency into the public API.
#[derive(sqlx::FromRow)]
struct MetadataQueryRow {
    pk_storage_object: i64,
    bucket:            String,
    key:               String,
    content_type:      String,
    size_bytes:        i64,
    etag:              Option<String>,
    owner_id:          Option<String>,
    pending:           bool,
    created_at:        DateTime<Utc>,
    updated_at:        DateTime<Utc>,
    expires_at:        Option<DateTime<Utc>>,
    metadata:          sqlx::types::Json<crate::policy::MetadataValues>,
}

impl From<MetadataQueryRow> for StorageMetadataRow {
    fn from(row: MetadataQueryRow) -> Self {
        Self {
            pk_storage_object: row.pk_storage_object,
            bucket:            row.bucket,
            key:               row.key,
            content_type:      row.content_type,
            size_bytes:        row.size_bytes,
            etag:              row.etag,
            owner_id:          row.owner_id,
            pending:           row.pending,
            created_at:        row.created_at,
            updated_at:        row.updated_at,
            expires_at:        row.expires_at,
            metadata:          row.metadata.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Public conversions
// ---------------------------------------------------------------------------

impl From<&StorageMetadataRow> for ObjectInfo {
    fn from(row: &StorageMetadataRow) -> Self {
        // Reason: size_bytes is non-negative (clamped above by .max(0)); cast to u64 is safe.
        #[allow(clippy::cast_sign_loss)]
        let size = row.size_bytes.max(0) as u64;
        Self {
            key: row.key.clone(),
            size,
            content_type: row.content_type.clone(),
            etag: row.etag.clone().unwrap_or_default(),
            last_modified: row.updated_at.to_rfc3339(),
        }
    }
}
