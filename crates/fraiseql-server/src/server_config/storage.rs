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

    // #371: parse policies at BOOT. An unknown method or principal spelling
    // refuses to start rather than becoming a rule that silently denies (or,
    // in a multi-rule policy, silently drops the narrowing rule).
    let policies = match section.policies {
        None => None,
        Some(ref rules) => {
            let mut parsed = Vec::with_capacity(rules.len());
            for (index, rule) in rules.iter().enumerate() {
                let methods = rule
                    .methods
                    .iter()
                    .map(|m| fraiseql_storage::PolicyMethod::parse(m))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| format!("[storage.{name}] policy rule {index}: {e}"))?;
                if methods.is_empty() {
                    return Err(format!(
                        "[storage.{name}] policy rule {index} lists no methods; a rule that                          permits nothing is a configuration mistake, not a denial"
                    ));
                }
                let principal = fraiseql_storage::PolicyPrincipal::parse(&rule.principal)
                    .map_err(|e| format!("[storage.{name}] policy rule {index}: {e}"))?;
                parsed.push(fraiseql_storage::PolicyRule {
                    methods,
                    principal,
                    key_prefix: rule.key_prefix.clone(),
                });
            }
            Some(fraiseql_storage::BucketPolicy { rules: parsed })
        },
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

    Ok(Some(StorageState {
        backend:  Arc::new(backend),
        metadata: Arc::new(StorageMetadataRepo::new(pool.clone())),
        rls:      StorageRlsEvaluator::new(),
        buckets:  Arc::new(buckets),
        uploads:  Arc::new(fraiseql_storage::UploadSessionRepo::new(pool)),
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
