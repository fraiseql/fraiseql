//! Bucket configuration and validation.

use serde::{Deserialize, Serialize};

/// Access control policy for a bucket.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub enum BucketAccess {
    /// All operations require authentication. The default: an unspecified
    /// access mode must never widen a bucket.
    #[default]
    Private,
    /// Read operations are public; write operations require authentication.
    PublicRead,
}

/// Image transform preset for predefined transformations.
///
/// Allows defining common image transformations (e.g., thumbnails, previews)
/// that can be applied by name via the render endpoint.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransformPreset {
    /// Name of the preset (e.g., "thumbnail", "medium", "preview")
    pub name: String,

    /// Target width in pixels
    pub width: Option<u32>,

    /// Target height in pixels
    pub height: Option<u32>,

    /// Output format (e.g., "webp", "jpeg", "png")
    pub format: Option<String>,

    /// Quality for lossy formats (1-100)
    pub quality: Option<u8>,

    /// How the resize fills the box: `contain` (the default), `stretch`,
    /// `fit`, `fill`, `cover-blur`, `cover-mirror` (#973).
    pub resize_mode: Option<String>,

    /// Where a `fill` keeps its content, or a crop is taken from.
    pub gravity: Option<String>,
}

/// Bucket names FraiseQL reserves for its own namespaces.
///
/// A logical bucket becomes the first path segment of every object key
/// (`{bucket}/{key}`), so a bucket carrying one of these names would put caller
/// objects inside the resumable-upload staging area (#369) or the render cache
/// (#973) — where they would be read back as staged chunks or served as
/// somebody else's rendering. The server refuses such a bucket at boot; there
/// is no runtime path that could produce one.
pub const RESERVED_BUCKET_NAMES: [&str; 2] = [".fraiseql-uploads", ".fraiseql-transforms"];

/// Bucket configuration.
///
/// Defines size limits, allowed MIME types, access policies, and transform presets for a bucket.
#[derive(Debug, Clone, Default)]
pub struct BucketConfig {
    /// Name of the bucket.
    pub name: String,

    /// Maximum object size in bytes (None = unlimited).
    pub max_object_bytes: Option<u64>,

    /// Allowed MIME types (`None` = any; `Some([])` = none allowed).
    ///
    /// Entries may be an exact media type (`application/pdf`), a subtype
    /// wildcard (`image/*`) or `*/*`. Matching is case-insensitive and ignores
    /// `Content-Type` parameters — see [`BucketConfig::allows_mime`].
    pub allowed_mime_types: Option<Vec<String>>,

    /// Access control policy.
    pub access: BucketAccess,

    /// Predefined image transform presets
    pub transform_presets: Option<Vec<TransformPreset>>,

    /// Resize mode applied when a render names none (#973). `None` means
    /// `contain`, the behaviour that shipped with the render endpoint.
    pub default_resize_mode: Option<String>,

    /// Font file backing text watermarks, read at boot. `None` means text
    /// watermarks are refused by name on this bucket rather than rendered in
    /// some fallback typeface.
    pub watermark_font: Option<std::sync::Arc<Vec<u8>>>,

    /// Serve downloads from this bucket with `Content-Disposition: inline`
    /// (rendered in-browser) instead of the default `attachment`
    /// (force-download).
    ///
    /// Defaults to `false` (force-download), which — together with the
    /// always-on `X-Content-Type-Options: nosniff` header — neutralises the
    /// stored-XSS surface for buckets that accept untrusted content (#337).
    /// Even when set to `true`, content types that browsers can execute as
    /// active content (`text/html`, `image/svg+xml`, …) are still served as
    /// `attachment`.
    pub serve_inline: bool,

    /// Per-bucket access policy (#371). When `Some`, it REPLACES the coarse
    /// [`access`](Self::access) mode for this bucket: every request is denied
    /// unless a rule permits it (the storage-admin role still bypasses). When
    /// `None`, the `access` mode governs as before.
    pub policies: Option<crate::policy::BucketPolicy>,

    /// Lifetime of a resumable-upload session in seconds (#369). `None` uses
    /// the built-in default of 24 hours. An expired session is refused (`410`)
    /// and reaped: its staged bytes are discarded and, when creation reserved
    /// the key, the reservation is released.
    pub upload_ttl_secs: Option<u64>,
}

/// Storage configuration (from fraiseql-server config).
///
/// This struct represents the storage backend configuration that specifies
/// which storage provider to use and its settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Storage backend type: "local", "s3", "gcs", "azure"
    pub backend: String,

    /// Path for local filesystem backend
    pub path: Option<String>,

    /// Bucket name for S3, GCS, and Azure backends
    pub bucket: Option<String>,

    /// AWS region for S3 backend
    pub region: Option<String>,

    /// Custom endpoint URL for S3-compatible services (`MinIO`, etc.) and the
    /// Azure Blob / GCS local-development emulators (Azurite, fake-gcs-server).
    pub endpoint: Option<String>,

    /// GCP project ID for GCS backend
    pub project_id: Option<String>,

    /// Azure account name
    pub account_name: Option<String>,
}

impl BucketConfig {
    /// Whether *any* unauthenticated read is possible in this bucket.
    ///
    /// Used only to choose between `401` and `404` when refusing: a caller who
    /// could never see anything without credentials is told to authenticate,
    /// while in a bucket with public content a refusal is indistinguishable
    /// from a missing object (#876 — no existence oracle).
    ///
    /// Under a policy (#371) this is "some rule permits an anonymous read",
    /// not the coarse `access` mode, which the policy has replaced.
    #[must_use]
    pub fn allows_anonymous_read(&self) -> bool {
        match self.policies {
            Some(ref policy) => policy.rules.iter().any(|rule| {
                rule.methods.contains(&crate::policy::PolicyMethod::Read)
                    && rule.principal == crate::policy::PolicyPrincipal::Anonymous
            }),
            None => matches!(self.access, BucketAccess::PublicRead),
        }
    }

    /// Whether this bucket accepts an upload with the given `Content-Type`.
    ///
    /// The single enforcement point for `allowed_mime_types`. It used to be two
    /// (#876): `put_handler` matched the *raw* header value, so an allow-list
    /// entry `text/plain` rejected the browser-standard
    /// `text/plain;charset=UTF-8`, and it read `Some([])` as "no restriction"
    /// — the opposite of the documented meaning — while `BucketService::upload`
    /// read `Some([])` correctly but ignored `image/*` wildcards entirely. Two
    /// implementations of one policy, disagreeing in both directions.
    ///
    /// Rules:
    /// - `None` — no restriction.
    /// - `Some(list)` — the media type must match an entry. An empty list therefore allows nothing,
    ///   as documented.
    /// - Parameters are stripped (`text/plain; charset=utf-8` matches `text/plain`) and comparison
    ///   is ASCII-case-insensitive, per RFC 9110 §8.3.
    /// - An entry of `*/*` matches anything; `type/*` matches that type.
    #[must_use]
    pub fn allows_mime(&self, content_type: &str) -> bool {
        let Some(ref allowed) = self.allowed_mime_types else {
            return true;
        };
        let actual = normalize_media_type(content_type);
        allowed.iter().any(|pattern| mime_pattern_matches(pattern, &actual))
    }
}

/// Reduce a `Content-Type` header value to its lowercase media type.
///
/// `text/plain; charset=UTF-8` → `text/plain`.
fn normalize_media_type(content_type: &str) -> String {
    content_type.split(';').next().unwrap_or("").trim().to_ascii_lowercase()
}

/// Match one allow-list entry against an already-normalised media type.
fn mime_pattern_matches(pattern: &str, actual: &str) -> bool {
    let pattern = normalize_media_type(pattern);
    if pattern == "*/*" || pattern == actual {
        return true;
    }
    match pattern.strip_suffix("/*") {
        Some(prefix) => {
            actual.starts_with(prefix) && actual.as_bytes().get(prefix.len()) == Some(&b'/')
        },
        None => false,
    }
}

#[cfg(test)]
mod mime_policy_tests;
