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
//!
//! `_fraiseql_storage_policies` (#974) holds per-bucket access policies pushed
//! over the admin API — one row per bucket, replacing that bucket's configured
//! policy wholesale. See [`crate::policy::store`] for the precedence rule.

/// The advisory-lock key the storage migration serializes on.
///
/// A fixed `i64`, chosen once: the ASCII bytes of `FSTORAGE`. It must never be derived from a
/// version string, a table name or anything else a later migration might edit — two runners
/// queue behind each other only if they pick the **same** number, so a computed key would
/// silently stop serializing on the day its input changed.
///
/// The value is deliberately below `i64::MAX` rather than a pattern with the high bit set:
/// `pg_advisory_xact_lock` takes a signed `bigint`, and a literal that needs wrapping to fit
/// is a literal someone will later "correct".
const STORAGE_MIGRATION_LOCK_KEY: i64 = 0x_4653_544f_5241_4745;

/// Run the storage migration, serialized against every concurrent runner (#1286).
///
/// [`storage_migration_sql`] is idempotent under repetition; it is **not** safe to run
/// concurrently. PostgreSQL evaluates `IF NOT EXISTS` and the create as separate steps, so two
/// sessions running the DDL at once against a database that does not yet carry the objects both
/// observe "absent" and both create. The loser gets a raw catalogue error — `23505` on
/// `pg_type_typname_nsp_index` or `pg_class_relname_nsp_index`, or `42P07 relation already
/// exists`.
///
/// Measured, not hypothesised: on a cold database this failed 5 of the 7
/// `storage_policy_admin_tests`, which each ran the migration in parallel. The failure then
/// **self-heals** — the run that fails leaves the objects behind, so every later run against
/// that database passes — which is why it read as flaky infrastructure for as long as it did.
/// The same DDL runs at server boot (`fraiseql_server::server_config::storage`), where a lost
/// race is not a flaky test but a server that does not start.
///
/// `pg_advisory_xact_lock` taken in the same transaction as the DDL makes concurrent runners
/// queue. Transaction-scoped deliberately: the commit or rollback releases it, so there is no
/// unlock to leak on an error path, and no way for a panicking caller to wedge every future
/// boot behind a lock nobody holds a handle to.
///
/// # Errors
///
/// Returns the underlying [`sqlx::Error`] if the transaction cannot be opened, the lock cannot
/// be taken, or the DDL fails.
pub async fn run_storage_migration(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(STORAGE_MIGRATION_LOCK_KEY)
        .execute(&mut *tx)
        .await?;
    sqlx::raw_sql(storage_migration_sql()).execute(&mut *tx).await?;
    tx.commit().await
}

#[cfg(test)]
mod tests;

/// Returns the SQL DDL to create the storage metadata table and indexes.
///
/// The DDL uses `IF NOT EXISTS`, which makes it idempotent under **repetition**: running it
/// again, after it has finished, is safe and produces no errors.
///
/// That is not the same as being safe under **concurrency**, and this function does nothing
/// about the latter — use [`run_storage_migration`] to execute it, which serializes runners
/// against each other (#1286). This returns the text, for `fraiseql-cli migrate up` and for
/// tests that assert on the DDL rather than run it.
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
/// | `expires_at` | `TIMESTAMPTZ` | Object expiry, read by `require_unexpired` (#974) |
/// | `metadata` | `JSONB NOT NULL DEFAULT '{}'` | User-defined metadata, read by `require_metadata` (#1099) |
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

-- #1099: user-defined object metadata, matchable by the require_metadata policy
-- condition. Defaults to an empty object, so adding the column cannot widen
-- access on existing rows: require_metadata fails against a key that is absent,
-- and every key is absent here.
--
-- Writing this column is its own permission (PolicyMethod::SetMetadata), never
-- implied by write or overwrite. That split is what lets require_metadata mean
-- a value the gated caller could not have written, without a reserved key
-- namespace to police.
ALTER TABLE _fraiseql_storage_objects
    ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}'::jsonb;

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

-- #974: per-bucket policies written over the admin API. The rules are held as
-- the operator wrote them (a JSON list of policy rule specs) rather than in
-- parsed form, so a boot-time load re-validates through the same door the
-- write came through. A row here REPLACES the bucket config policy wholesale;
-- the two are never merged.
CREATE TABLE IF NOT EXISTS _fraiseql_storage_policies (
    bucket     TEXT        PRIMARY KEY,
    rules      JSONB       NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
"
}
