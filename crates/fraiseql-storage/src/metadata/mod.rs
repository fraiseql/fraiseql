//! Object metadata storage and retrieval.
//!
//! Tracks uploaded objects in a PostgreSQL table (`_fraiseql_storage_objects`)
//! for RLS enforcement, listing, and lifecycle management.

#[cfg(test)]
mod tests;

use chrono::{DateTime, Utc};
use fraiseql_error::FraiseQLError;
use sqlx::PgPool;

use crate::backend::types::ObjectInfo;

/// A row from the `_fraiseql_storage_objects` table.
#[derive(Debug, Clone)]
pub struct StorageMetadataRow {
    /// Primary key.
    pub pk_storage_object: i64,
    /// Bucket name.
    pub bucket: String,
    /// Object key (path within bucket).
    pub key: String,
    /// MIME content type.
    pub content_type: String,
    /// Object size in bytes.
    pub size_bytes: i64,
    /// Entity tag for integrity verification.
    pub etag: Option<String>,
    /// Owner identifier (user sub claim).
    pub owner_id: Option<String>,
    /// Row creation time.
    pub created_at: DateTime<Utc>,
    /// Last update time.
    pub updated_at: DateTime<Utc>,
}

/// Data required to insert a new storage object record.
#[derive(Debug, Clone)]
pub struct NewStorageObject {
    /// Bucket name.
    pub bucket: String,
    /// Object key (path within bucket).
    pub key: String,
    /// MIME content type.
    pub content_type: String,
    /// Object size in bytes.
    pub size_bytes: i64,
    /// Entity tag for integrity verification.
    pub etag: Option<String>,
    /// Owner identifier (user sub claim).
    pub owner_id: Option<String>,
}

/// Storage metadata repository backed by PostgreSQL.
pub struct StorageMetadataRepo {
    pool: PgPool,
}

impl StorageMetadataRepo {
    /// Create a new repository wrapping the given connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Insert a new object metadata row, returning the generated primary key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` if the database query fails
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
        .map_err(|e| FraiseQLError::Storage {
            message: e.to_string(),
            code: None,
        })?;

        Ok(pk)
    }

    /// Look up an object by bucket and key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` if the database query fails.
    pub async fn get(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<Option<StorageMetadataRow>, FraiseQLError> {
        let row = sqlx::query_as::<_, MetadataQueryRow>(
            "SELECT pk_storage_object, bucket, key, content_type, \
                    size_bytes, etag, owner_id, created_at, updated_at \
             FROM _fraiseql_storage_objects \
             WHERE bucket = $1 AND key = $2",
        )
        .bind(bucket)
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| FraiseQLError::Storage {
            message: e.to_string(),
            code: None,
        })?;

        Ok(row.map(Into::into))
    }

    /// Delete an object metadata row by bucket and key.
    ///
    /// Returns `true` if a row was actually deleted, `false` if no matching row existed.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` if the database query fails.
    pub async fn delete(&self, bucket: &str, key: &str) -> Result<bool, FraiseQLError> {
        let result = sqlx::query(
            "DELETE FROM _fraiseql_storage_objects WHERE bucket = $1 AND key = $2",
        )
        .bind(bucket)
        .bind(key)
        .execute(&self.pool)
        .await
        .map_err(|e| FraiseQLError::Storage {
            message: e.to_string(),
            code: None,
        })?;

        Ok(result.rows_affected() > 0)
    }

    /// List objects in a bucket, optionally filtered by key prefix.
    ///
    /// Results are ordered by key ascending. Use `limit` and `offset` for pagination.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` if the database query fails.
    pub async fn list(
        &self,
        bucket: &str,
        prefix: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<StorageMetadataRow>, FraiseQLError> {
        let rows = match prefix {
            Some(pfx) => {
                sqlx::query_as::<_, MetadataQueryRow>(
                    "SELECT pk_storage_object, bucket, key, content_type, \
                            size_bytes, etag, owner_id, created_at, updated_at \
                     FROM _fraiseql_storage_objects \
                     WHERE bucket = $1 AND key LIKE $2 \
                     ORDER BY key ASC \
                     LIMIT $3 OFFSET $4",
                )
                .bind(bucket)
                .bind(format!("{pfx}%"))
                .bind(i64::from(limit))
                .bind(i64::from(offset))
                .fetch_all(&self.pool)
                .await
            }
            None => {
                sqlx::query_as::<_, MetadataQueryRow>(
                    "SELECT pk_storage_object, bucket, key, content_type, \
                            size_bytes, etag, owner_id, created_at, updated_at \
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
            }
        }
        .map_err(|e| FraiseQLError::Storage {
            message: e.to_string(),
            code: None,
        })?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Insert or update an object metadata row (upsert on `(bucket, key)`).
    ///
    /// On conflict, updates `content_type`, `size_bytes`, `etag`, and `updated_at`.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` if the database query fails.
    pub async fn upsert(&self, row: &NewStorageObject) -> Result<i64, FraiseQLError> {
        let (pk,): (i64,) = sqlx::query_as(
            "INSERT INTO _fraiseql_storage_objects \
                 (bucket, key, content_type, size_bytes, etag, owner_id) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (bucket, key) DO UPDATE SET \
                 content_type = EXCLUDED.content_type, \
                 size_bytes   = EXCLUDED.size_bytes, \
                 etag         = EXCLUDED.etag, \
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
        .map_err(|e| FraiseQLError::Storage {
            message: e.to_string(),
            code: None,
        })?;

        Ok(pk)
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
    bucket: String,
    key: String,
    content_type: String,
    size_bytes: i64,
    etag: Option<String>,
    owner_id: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<MetadataQueryRow> for StorageMetadataRow {
    fn from(row: MetadataQueryRow) -> Self {
        Self {
            pk_storage_object: row.pk_storage_object,
            bucket: row.bucket,
            key: row.key,
            content_type: row.content_type,
            size_bytes: row.size_bytes,
            etag: row.etag,
            owner_id: row.owner_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Public conversions
// ---------------------------------------------------------------------------

impl From<&StorageMetadataRow> for ObjectInfo {
    fn from(row: &StorageMetadataRow) -> Self {
        Self {
            key: row.key.clone(),
            size: row.size_bytes.max(0) as u64,
            content_type: row.content_type.clone(),
            etag: row.etag.clone().unwrap_or_default(),
            last_modified: row.updated_at.to_rfc3339(),
        }
    }
}
