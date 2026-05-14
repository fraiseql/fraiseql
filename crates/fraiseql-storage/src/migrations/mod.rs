//! Database migrations for storage metadata tables.
//!
//! Exposes DDL that `fraiseql-cli migrate up` can execute to create the
//! `_fraiseql_storage_objects` table and its indexes.

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
/// | `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | Row creation |
/// | `updated_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | Last modification |
///
/// # Example
///
/// ```
/// let sql = fraiseql_storage::migrations::storage_migration_sql();
/// assert!(sql.contains("_fraiseql_storage_objects"));
/// ```
pub fn storage_migration_sql() -> &'static str {
    "\
CREATE TABLE IF NOT EXISTS _fraiseql_storage_objects (
    pk_storage_object BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    bucket            TEXT        NOT NULL,
    key               TEXT        NOT NULL,
    content_type      TEXT        NOT NULL,
    size_bytes        BIGINT      NOT NULL,
    etag              TEXT,
    owner_id          TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (bucket, key)
);

CREATE INDEX IF NOT EXISTS idx_storage_objects_bucket_key
    ON _fraiseql_storage_objects (bucket, key);

CREATE INDEX IF NOT EXISTS idx_storage_objects_owner
    ON _fraiseql_storage_objects (owner_id)
    WHERE owner_id IS NOT NULL;
"
}
