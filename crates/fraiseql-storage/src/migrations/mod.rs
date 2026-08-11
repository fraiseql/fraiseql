//! Database migrations for storage metadata tables.
//!
//! Exposes DDL that `fraiseql-cli migrate up` can execute to create the
//! `_fraiseql_storage_objects` table and its indexes, plus the
//! `_fraiseql_storage_uploads` table backing resumable (Tus) uploads (#369):
//! one row per in-flight upload session carrying the declared length, the
//! current offset, the owner, the reserved metadata row it will confirm on
//! completion, and opaque per-backend continuation state (the S3 multipart
//! upload id and part etags, or the local temp-file marker). `UNIQUE (bucket,
//! key)` allows at most one in-flight resumable upload per object key.

#[cfg(test)]
mod tests;

/// Returns the SQL DDL to create the storage metadata table and indexes.
///
/// The DDL uses `IF NOT EXISTS` for idempotency — running it multiple times
/// is safe and produces no errors.
///
/// # Table Schema
///
/// | Column | Type | Notes |
/// |--------|------|-------|
/// | `pk_storage_object` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Trinity-style PK |
/// | `bucket` | `TEXT NOT NULL` | Bucket name |
/// | `key` | `TEXT NOT NULL` | Object key (path) |
/// | `content_type` | `TEXT NOT NULL` | MIME type |
/// | `size_bytes` | `BIGINT NOT NULL` | Object size |
/// | `etag` | `TEXT` | Entity tag |
/// | `owner_id` | `TEXT` | Uploader's sub claim |
/// | `pending` | `BOOLEAN NOT NULL DEFAULT FALSE` | An upload is in flight for this key |
/// | `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | Row creation |
/// | `updated_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | Last modification |
///
/// `pending` exists because a presigned upload writes the object *directly to
/// the backend*, so the server never sees the bytes (#866). The row is claimed
/// when the URL is signed — which is what gives the object an owner and keeps
/// the H9/B4 overwrite gate applicable — and carries `size_bytes = 0` /
/// `etag IS NULL` until a read reconciles it against the stored object.
///
/// # Example
///
/// ```
/// let sql = fraiseql_storage::migrations::storage_migration_sql();
/// assert!(sql.contains("_fraiseql_storage_objects"));
/// ```
#[must_use]
pub const fn storage_migration_sql() -> &'static str {
    "\
CREATE TABLE IF NOT EXISTS _fraiseql_storage_objects (
    pk_storage_object BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    bucket            TEXT        NOT NULL,
    key               TEXT        NOT NULL,
    content_type      TEXT        NOT NULL,
    size_bytes        BIGINT      NOT NULL,
    etag              TEXT,
    owner_id          TEXT,
    pending           BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (bucket, key)
);

CREATE INDEX IF NOT EXISTS idx_storage_objects_bucket_key
    ON _fraiseql_storage_objects (bucket, key);

ALTER TABLE _fraiseql_storage_objects
    ADD COLUMN IF NOT EXISTS pending BOOLEAN NOT NULL DEFAULT FALSE;

CREATE INDEX IF NOT EXISTS idx_storage_objects_owner
    ON _fraiseql_storage_objects (owner_id)
    WHERE owner_id IS NOT NULL;

-- #974: the object's own expiry, read by the require_unexpired policy
-- condition. Nullable, and a NULL is a denial for that condition rather than a
-- never-expires — so adding the column cannot widen access on existing rows.
ALTER TABLE _fraiseql_storage_objects
    ADD COLUMN IF NOT EXISTS expires_at TIMESTAMPTZ;

CREATE TABLE IF NOT EXISTS _fraiseql_storage_uploads (
    upload_id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    bucket              TEXT        NOT NULL,
    key                 TEXT        NOT NULL,
    content_type        TEXT        NOT NULL,
    declared_bytes      BIGINT      NOT NULL,
    received_bytes      BIGINT      NOT NULL DEFAULT 0,
    owner_id            TEXT,
    pk_storage_object   BIGINT      NOT NULL,
    created_reservation BOOLEAN     NOT NULL,
    backend_state       JSONB       NOT NULL DEFAULT '{}'::jsonb,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at          TIMESTAMPTZ NOT NULL,
    UNIQUE (bucket, key)
);

CREATE INDEX IF NOT EXISTS idx_storage_uploads_expires
    ON _fraiseql_storage_uploads (expires_at);
"
}
