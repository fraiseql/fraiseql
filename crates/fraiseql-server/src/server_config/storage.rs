//! Resolution of `[storage.<name>]` config sections into the storage runtime
//! types.
//!
//! [`resolve_storage_section`] validates the configured storage section(s) and
//! maps the single supported section into the
//! [`fraiseql_storage::config::StorageConfig`] (backend connection) and
//! [`fraiseql_storage::config::BucketConfig`] (logical-bucket policy) the storage
//! runtime needs. It is pure and IO-free so it can be unit-tested without a
//! database; the actual backend construction and metadata wiring happen in the
//! binary boot path.

use std::{collections::HashMap, sync::Arc};

use fraiseql_storage::{
    StorageMetadataRepo, StorageRlsEvaluator, StorageState,
    config::{BucketAccess, BucketConfig, StorageConfig},
};
use sqlx::postgres::PgPoolOptions;

use super::{ServerConfig, StorageSectionConfig};

/// Maximum size of the dedicated connection pool used for storage object
/// metadata. Storage is metadata-light (one row per object operation), so a
/// small pool is sufficient and keeps startup cheap.
const STORAGE_METADATA_POOL_MAX: u32 = 5;

/// A `[storage.<name>]` section resolved into the types the storage runtime
/// needs.
#[derive(Debug, Clone)]
pub struct ResolvedStorage {
    /// Backend connection config passed to `fraiseql_storage::create_backend`.
    pub backend: StorageConfig,
    /// Logical-bucket access policy. `bucket.name` is the section key and the
    /// bucket name used in the URL path.
    pub bucket:  BucketConfig,
}

/// Resolve the configured storage section into a [`ResolvedStorage`].
///
/// Returns `Ok(None)` when no `[storage.<name>]` section is configured.
///
/// # Errors
///
/// Returns an error message when:
/// - more than one `[storage.<name>]` section is configured (the binary currently supports a single
///   backend), or
/// - a section's `access` value is not `"private"` or `"public_read"`.
pub fn resolve_storage_section(config: &ServerConfig) -> Result<Option<ResolvedStorage>, String> {
    resolve_from_map(&config.storage)
}

fn resolve_from_map(
    storage: &HashMap<String, StorageSectionConfig>,
) -> Result<Option<ResolvedStorage>, String> {
    // 0 sections → None; exactly 1 → resolve; >1 → error. Iterating once handles
    // all three without an `unwrap`/`expect` on the single-element case.
    let mut iter = storage.iter();
    let Some((name, section)) = iter.next() else {
        return Ok(None);
    };
    if iter.next().is_some() {
        let mut names: Vec<&str> = storage.keys().map(String::as_str).collect();
        names.sort_unstable();
        return Err(format!(
            "multiple [storage.<name>] sections configured ({}); the fraiseql-server binary \
             currently supports a single storage backend — configure exactly one [storage.<name>].",
            names.join(", "),
        ));
    }

    let access = parse_access(section.access.as_deref())?;

    let backend = StorageConfig {
        backend:      section.backend.clone(),
        path:         section.path.clone(),
        bucket:       section.bucket.clone(),
        region:       section.region.clone(),
        endpoint:     section.endpoint.clone(),
        project_id:   section.project_id.clone(),
        account_name: section.account_name.clone(),
    };

    let bucket = BucketConfig {
        name: name.clone(),
        max_object_bytes: section.max_object_bytes,
        allowed_mime_types: section.allowed_mime_types.clone(),
        access,
        transform_presets: None,
        serve_inline: section.serve_inline.unwrap_or(false),
    };

    Ok(Some(ResolvedStorage { backend, bucket }))
}

/// Build the storage runtime [`StorageState`] from the configured
/// `[storage.<name>]` section, or `Ok(None)` when storage is not configured.
///
/// Connects a small dedicated PostgreSQL pool from `config.database_url`,
/// ensures the object-metadata table exists (idempotent DDL), constructs the
/// backend, and assembles the state. Object storage via the binary is
/// PostgreSQL-only because [`StorageMetadataRepo`] requires a `sqlx::PgPool`.
///
/// # Errors
///
/// Returns an error message when the storage section is invalid (see
/// [`resolve_storage_section`]), the metadata database cannot be reached, the
/// metadata table cannot be created, or the backend cannot be constructed (for
/// example, a backend whose Cargo feature is not compiled in).
pub async fn build_storage_state(config: &ServerConfig) -> Result<Option<StorageState>, String> {
    let Some(resolved) = resolve_storage_section(config)? else {
        return Ok(None);
    };
    let bucket_name = resolved.bucket.name.clone();

    let pool = PgPoolOptions::new()
        .max_connections(STORAGE_METADATA_POOL_MAX)
        .connect(&config.database_url)
        .await
        .map_err(|e| {
            format!("storage: failed to connect to PostgreSQL for object metadata: {e}")
        })?;

    sqlx::raw_sql(fraiseql_storage::migrations::storage_migration_sql())
        .execute(&pool)
        .await
        .map_err(|e| format!("storage: failed to ensure the object-metadata table exists: {e}"))?;

    let backend = fraiseql_storage::create_backend(&resolved.backend).await.map_err(|e| {
        format!("storage: failed to create backend for bucket '{bucket_name}': {e}")
    })?;

    let mut buckets = HashMap::new();
    buckets.insert(bucket_name, resolved.bucket);

    Ok(Some(StorageState {
        backend:  Arc::new(backend),
        metadata: Arc::new(StorageMetadataRepo::new(pool)),
        rls:      StorageRlsEvaluator::new(),
        buckets:  Arc::new(buckets),
    }))
}

/// Parse the optional per-bucket `access` policy. Defaults to the secure
/// [`BucketAccess::Private`] policy when unset.
fn parse_access(access: Option<&str>) -> Result<BucketAccess, String> {
    let Some(value) = access else {
        return Ok(BucketAccess::Private);
    };
    match value.to_ascii_lowercase().as_str() {
        "private" => Ok(BucketAccess::Private),
        "public_read" | "public-read" => Ok(BucketAccess::PublicRead),
        other => Err(format!(
            "invalid storage access policy {other:?}; expected \"private\" or \"public_read\""
        )),
    }
}
