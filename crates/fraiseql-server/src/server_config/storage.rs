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

    // #973: a bucket name becomes the first segment of every object key, so a
    // bucket named after one of FraiseQL's own namespaces would place caller
    // objects inside the upload staging area or the render cache. Refuse it
    // here, where the name is chosen, rather than guarding every write.
    if fraiseql_storage::config::RESERVED_BUCKET_NAMES.contains(&name.as_str()) {
        return Err(format!(
            "[storage.{name}] uses a bucket name FraiseQL reserves for its own namespaces \
             ({}); objects would land inside the upload staging area or the render cache. \
             Rename the bucket.",
            fraiseql_storage::config::RESERVED_BUCKET_NAMES.join(", ")
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

    // #370: presets configured without the feature that serves them must be a
    // startup error — a config key that silently does nothing is the P06
    // defect.
    #[cfg(not(feature = "storage-transforms"))]
    if section.transform_presets.is_some() {
        return Err(format!(
            "[storage.{name}] declares transform_presets, but this server binary was built              without the `storage-transforms` feature — the render endpoint does not exist, so              the presets could never be served. Rebuild with the feature or remove the key.",
        ));
    }
    // #973's render keys are the same class: accepted here, servable only by a
    // binary that carries the render endpoint.
    #[cfg(not(feature = "storage-transforms"))]
    for (key, present) in [
        ("default_resize_mode", section.default_resize_mode.is_some()),
        ("watermark_font", section.watermark_font.is_some()),
    ] {
        if present {
            return Err(format!(
                "[storage.{name}] declares {key}, but this server binary was built without the \
                 `storage-transforms` feature — the render endpoint does not exist, so it could \
                 never take effect. Rebuild with the feature or remove the key."
            ));
        }
    }

    #[cfg(not(feature = "storage-transforms"))]
    let (transform_presets, watermark_font) = (
        section.transform_presets.as_ref().map(|_| Vec::new()),
        Option::<std::sync::Arc<Vec<u8>>>::None,
    );

    // #973: every preset spelling is validated HERE, at boot. A misspelt mode
    // or gravity would otherwise render something the operator did not ask for
    // on every request, and a quality paired with a losslessly-encoded format
    // is a parameter that can never take effect — the accepted-and-unconsumed
    // shape rule 2 forbids.
    #[cfg(feature = "storage-transforms")]
    let transform_presets = parse_transform_presets(name, section)?;

    #[cfg(feature = "storage-transforms")]
    if let Some(ref mode) = section.default_resize_mode {
        if fraiseql_storage::ResizeMode::parse(mode).is_none() {
            return Err(format!(
                "[storage.{name}] default_resize_mode = '{mode}' is not one of contain, stretch, \
                 fit, fill, cover-blur, cover-mirror"
            ));
        }
    }

    // The font is read and parsed at boot for the same reason the policies are:
    // a watermark that fails at render time fails once per request, forever.
    #[cfg(feature = "storage-transforms")]
    let watermark_font = match section.watermark_font {
        None => None,
        Some(ref path) => {
            let bytes = std::fs::read(path).map_err(|e| {
                format!("[storage.{name}] watermark_font '{path}' could not be read: {e}")
            })?;
            fraiseql_storage::transforms::text::parse_font(bytes.clone())
                .map_err(|e| format!("[storage.{name}] watermark_font '{path}': {e}"))?;
            Some(std::sync::Arc::new(bytes))
        },
    };

    // #371: parse policies at BOOT. An unknown method or principal spelling —
    // or, since #974, a malformed time bound — refuses to start rather than
    // becoming a rule that silently denies, or that quietly loses its narrowing
    // condition and permits more than it reads as permitting.
    //
    // The parse itself lives in fraiseql-storage so that the admin endpoint
    // pushing a policy at runtime accepts exactly this set of policies.
    let policies = match section.policies {
        None => None,
        Some(ref rules) => Some(
            fraiseql_storage::parse_policy(rules).map_err(|e| format!("[storage.{name}] {e}"))?,
        ),
    };

    let bucket = BucketConfig {
        name: name.clone(),
        max_object_bytes: section.max_object_bytes,
        allowed_mime_types: section.allowed_mime_types.clone(),
        access,
        transform_presets,
        policies,
        serve_inline: section.serve_inline.unwrap_or(false),
        upload_ttl_secs: section.upload_ttl_secs,
        default_resize_mode: section.default_resize_mode.clone(),
        watermark_font,
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

    let state = StorageState::new(
        Arc::new(backend),
        Arc::new(StorageMetadataRepo::new(pool.clone())),
        StorageRlsEvaluator::new(),
        buckets,
        Arc::new(fraiseql_storage::UploadSessionRepo::new(pool.clone())),
        Arc::new(fraiseql_storage::StoragePolicyStore::new(pool)),
    );

    apply_stored_policies(&state).await?;

    Ok(Some(state))
}

/// Overlay the durable policies onto the freshly-configured buckets, and say in
/// the log which source ends up governing each one (#974).
///
/// Three readings are load-bearing here:
///
/// - **A stored policy replaces the configured one wholesale.** Never merged — see
///   `fraiseql_storage::policy::store` for why.
/// - **An unparseable stored row refuses the boot**, exactly as a bad `[[storage.*.policies]]`
///   entry does. The alternative — skip the row and fall back to the configured policy — reverts a
///   deliberate narrowing without anyone asking, and the other alternative — deny everything for
///   that bucket — is the silent 3am denial #371 exists to prevent. A row can only get here by
///   being hand-edited in SQL or written by a different version, since the admin endpoint parses
///   before it persists.
/// - **A row for a bucket that is not configured is a warning, not an error.** It governs nothing,
///   but an operator who renamed a bucket needs to know their policy stopped applying.
///
/// # Errors
///
/// Returns an error message when the store cannot be read, or when a stored
/// policy fails to parse.
async fn apply_stored_policies(state: &StorageState) -> Result<(), String> {
    let report = state
        .reload_policies()
        .await
        .map_err(|e| format!("storage: failed to read stored bucket policies: {e}"))?;

    // A row this server cannot parse is a row written outside the admin API or
    // by a different version. `reload_policies` left the bucket alone — correct
    // once serving, but at boot "alone" means the configured policy, which
    // would silently undo whatever narrowing the stored one expressed. Refuse
    // to start instead, exactly as a malformed [[storage.*.policies]] entry
    // does.
    if let Some((bucket, error)) = report.invalid.first() {
        return Err(format!(
            "storage: the stored policy for bucket '{bucket}' is not valid ({error}). It was \
             written outside the admin API or by a different version; fix or delete the \
             _fraiseql_storage_policies row before starting."
        ));
    }
    report.log_problems();

    for (bucket, source) in &report.sources {
        tracing::info!(
            bucket = %bucket,
            policy_source = source.as_str(),
            "storage bucket access is governed by this policy source"
        );
    }
    Ok(())
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

/// Validate and map a section's `transform_presets` (#973).
///
/// Every spelling is checked HERE, at boot: a misspelt mode or gravity would
/// otherwise render something the operator did not ask for on every request,
/// and a quality paired with a losslessly-encoded format is a parameter that
/// can never take effect — the accepted-and-unconsumed shape rule 2 forbids.
#[cfg(feature = "storage-transforms")]
fn parse_transform_presets(
    name: &str,
    section: &StorageSectionConfig,
) -> Result<Option<Vec<fraiseql_storage::config::TransformPreset>>, String> {
    let Some(ref presets) = section.transform_presets else {
        return Ok(None);
    };
    let mut parsed = Vec::with_capacity(presets.len());
    for preset in presets {
        if let Some(ref mode) = preset.resize_mode {
            if fraiseql_storage::ResizeMode::parse(mode).is_none() {
                return Err(format!(
                    "[storage.{name}] preset '{}' declares resize_mode = '{mode}', which is not \
                     one of contain, stretch, fit, fill, cover-blur, cover-mirror",
                    preset.name
                ));
            }
        }
        if let Some(ref gravity) = preset.gravity {
            if fraiseql_storage::Gravity::parse(gravity).is_none() {
                return Err(format!(
                    "[storage.{name}] preset '{}' declares gravity = '{gravity}', which is not a \
                     compass point, center, or smart",
                    preset.name
                ));
            }
        }
        if let (Some(quality), Some(format)) = (preset.quality, preset.format.as_deref()) {
            if matches!(format.to_ascii_lowercase().as_str(), "png" | "webp") {
                return Err(format!(
                    "[storage.{name}] preset '{}' declares quality = {quality} with format = \
                     '{format}', which this server encodes losslessly — the quality could never \
                     take effect. Use jpeg or avif, or drop the quality.",
                    preset.name
                ));
            }
        }
        parsed.push(fraiseql_storage::config::TransformPreset {
            name:        preset.name.clone(),
            width:       preset.width,
            height:      preset.height,
            format:      preset.format.clone(),
            quality:     preset.quality,
            resize_mode: preset.resize_mode.clone(),
            gravity:     preset.gravity.clone(),
        });
    }
    Ok(Some(parsed))
}
