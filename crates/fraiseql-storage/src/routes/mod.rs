//! HTTP route handlers for storage operations.
//!
//! Provides a complete `axum::Router` for object storage:
//! - `PUT /storage/v1/object/{bucket}/{*key}` — upload
//! - `GET /storage/v1/object/{bucket}/{*key}` — download
//! - `DELETE /storage/v1/object/{bucket}/{*key}` — delete
//! - `GET /storage/v1/list/{bucket}` — list
//! - `POST /storage/v1/presign/{bucket}/{*key}` — presigned URL
//!
//! There is no transform/render route: `ImageTransformer` is a library-level
//! capability with no HTTP surface (#901).

#[cfg(feature = "transforms")]
mod render;
#[cfg(test)]
mod tests;
mod uploads;

use std::{collections::HashMap, sync::Arc};

use arc_swap::ArcSwap;
use axum::{
    Extension, Router,
    body::Body,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use bytes::Bytes;
use fraiseql_error::{FileError, FraiseQLError};
use serde::{Deserialize, Serialize};

#[cfg(feature = "aws-s3")]
use crate::PresignedUrl;
use crate::{
    backend::StorageBackend,
    config::BucketConfig,
    metadata::{NewStorageObject, StorageMetadataRepo, StorageMetadataRow},
    policy::{BucketPolicy, StoragePolicyStore},
    rls::StorageRlsEvaluator,
    uploads::UploadSessionRepo,
};

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Shared state for all storage route handlers.
#[derive(Clone)]
pub struct StorageState {
    /// Storage backend (shared across all buckets).
    pub backend:         Arc<StorageBackend>,
    /// Metadata repository for object tracking.
    pub metadata:        Arc<StorageMetadataRepo>,
    /// RLS evaluator for access control.
    pub rls:             StorageRlsEvaluator,
    /// Bucket configurations keyed by bucket name.
    ///
    /// Behind an [`ArcSwap`] because a bucket's
    /// [`policies`](BucketConfig::policies) can be replaced at runtime over the
    /// admin API (#974). A reader takes a snapshot with `.load()` and holds it
    /// for the whole request, so a policy pushed mid-request cannot decide half
    /// of it. Everything else about a bucket is fixed at boot — including
    /// `max_object_bytes`, which the router's body limit is sized from once.
    pub buckets:         Arc<ArcSwap<HashMap<String, BucketConfig>>>,
    /// Resumable-upload session repository (#369).
    pub uploads:         Arc<UploadSessionRepo>,
    /// Durable per-bucket policies (#974).
    pub policy_store:    Arc<StoragePolicyStore>,
    /// The policy each bucket was **configured** with, captured before any
    /// stored policy was applied.
    ///
    /// This is what a `DELETE` of a stored policy reverts a bucket to, so it
    /// has to survive the overlay that replaced it. Set once by
    /// [`StorageState::new`] and never mutated.
    pub config_policies: Arc<HashMap<String, Option<BucketPolicy>>>,
}

impl StorageState {
    /// Assemble the storage runtime state from its boot-time configuration.
    ///
    /// Snapshots each bucket's configured policy into
    /// [`config_policies`](Self::config_policies), so callers cannot get that
    /// invariant wrong: whatever a bucket's policy is at construction *is* its
    /// configured policy.
    #[must_use]
    pub fn new(
        backend: Arc<StorageBackend>,
        metadata: Arc<StorageMetadataRepo>,
        rls: StorageRlsEvaluator,
        buckets: HashMap<String, BucketConfig>,
        uploads: Arc<UploadSessionRepo>,
        policy_store: Arc<StoragePolicyStore>,
    ) -> Self {
        let config_policies = buckets
            .iter()
            .map(|(name, bucket)| (name.clone(), bucket.policies.clone()))
            .collect();
        Self {
            backend,
            metadata,
            rls,
            buckets: Arc::new(ArcSwap::from_pointee(buckets)),
            uploads,
            policy_store,
            config_policies: Arc::new(config_policies),
        }
    }

    /// Replace one bucket's access policy, effective for every request that
    /// starts after this returns.
    ///
    /// Only the [`policies`](BucketConfig::policies) field is touched — of that
    /// bucket, and of no other. That is what makes #371's *"a stored policy
    /// replaces the configured one wholesale, the two are never merged"* true
    /// at the type level rather than by convention: there is no code path here
    /// that could combine two rule lists, because the new list is moved into
    /// the field and the old one is dropped.
    ///
    /// `None` restores the bucket to having no policy at all, which hands it
    /// back to the coarse `access` mode.
    ///
    /// Returns `false` — changing nothing — when the bucket is not configured.
    #[must_use]
    pub fn set_bucket_policies(&self, bucket: &str, policy: &Option<BucketPolicy>) -> bool {
        if !self.buckets.load().contains_key(bucket) {
            return false;
        }
        // `rcu` re-runs its closure if another writer wins the race, so two
        // concurrent pushes to different buckets cannot lose one another.
        self.buckets.rcu(|current| {
            let mut next = HashMap::clone(current);
            if let Some(config) = next.get_mut(bucket) {
                config.policies.clone_from(policy);
            }
            Arc::new(next)
        });
        true
    }

    /// The policy a bucket was configured with, ignoring anything pushed since.
    #[must_use]
    pub fn config_policy(&self, bucket: &str) -> Option<&BucketPolicy> {
        self.config_policies.get(bucket).and_then(Option::as_ref)
    }

    /// Reconcile every configured bucket's policy against the store.
    ///
    /// This is the whole precedence rule executed: a bucket with a stored row
    /// gets that policy, a bucket without one is returned to its configured
    /// policy (which is how a `DELETE` on another replica propagates here), and
    /// nothing is ever merged.
    ///
    /// A row that fails to parse leaves its bucket **exactly as it is** — the
    /// last good generation keeps serving. That is deliberately different from
    /// the boot path, which refuses to start on the same row: at boot there is
    /// no running policy to preserve, and refusing is how #371 keeps a typo
    /// from silently becoming a deny-all. Once serving, throwing away a working
    /// policy because someone hand-edited a row is the worse of the two.
    /// The offending rows come back in the report so the caller can decide.
    ///
    /// # Errors
    ///
    /// `FraiseQLError::File` if the store cannot be read. Nothing is changed in
    /// that case; a database blip must not drop every bucket back to its
    /// configured policy.
    pub async fn reload_policies(&self) -> Result<PolicyReloadReport, FraiseQLError> {
        let rows = self.policy_store.list().await?;
        let known = self.buckets.load();

        let mut stored: HashMap<String, BucketPolicy> = HashMap::new();
        let mut report = PolicyReloadReport::default();
        for row in rows {
            if !known.contains_key(&row.bucket) {
                report.unknown_buckets.push(row.bucket);
                continue;
            }
            match row.parse() {
                Ok(policy) => {
                    stored.insert(row.bucket, policy);
                },
                Err(e) => report.invalid.push((row.bucket, e)),
            }
        }

        for name in known.keys() {
            if report.invalid.iter().any(|(bucket, _)| bucket == name) {
                continue;
            }
            let from_store = stored.remove(name);
            let source = crate::policy::policy_source(
                from_store.is_some(),
                self.config_policy(name).is_some(),
            );
            let effective = from_store.or_else(|| self.config_policy(name).cloned());
            // Every `name` came from the same snapshot, so the bucket is known
            // by construction and the return value carries no new information.
            let _applied = self.set_bucket_policies(name, &effective);
            report.sources.insert(name.clone(), source);
        }
        Ok(report)
    }

    /// Reload from the store every `interval`, forever. The caller owns the
    /// task.
    ///
    /// A write made *through this replica* takes effect immediately, so this
    /// exists for the other direction: a policy pushed at another replica, and
    /// the bound on how long that replica's operator has to wait before the
    /// change is deployment-wide.
    pub async fn policy_refresh_loop(self, interval: std::time::Duration) {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            match self.reload_policies().await {
                Ok(report) => report.log_problems(),
                Err(e) => tracing::error!(
                    error = %e,
                    "storage policy refresh failed; the policies now in force keep serving"
                ),
            }
        }
    }

    /// The same backend, metadata and store, governing a different set of
    /// buckets — which become the new configured baseline.
    ///
    /// This is a re-*configuration*, not a policy push: it re-snapshots
    /// [`config_policies`](Self::config_policies) from the map it is handed.
    /// Use [`set_bucket_policies`](Self::set_bucket_policies) to push a policy
    /// over an existing configuration.
    #[must_use]
    pub fn with_buckets(&self, buckets: HashMap<String, BucketConfig>) -> Self {
        Self::new(
            self.backend.clone(),
            self.metadata.clone(),
            self.rls,
            buckets,
            self.uploads.clone(),
            self.policy_store.clone(),
        )
    }
}

/// How long a policy pushed at one replica takes to reach the others.
///
/// The replica that served the write applies it before answering, so this
/// bounds only cross-replica propagation.
pub const DEFAULT_POLICY_REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

/// What one [`StorageState::reload_policies`] pass did, and what it refused.
#[derive(Debug, Default)]
pub struct PolicyReloadReport {
    /// Which source governs each configured bucket after the pass.
    pub sources:         std::collections::BTreeMap<String, crate::policy::PolicySource>,
    /// Stored rows that could not be parsed. Their buckets are untouched.
    pub invalid:         Vec<(String, crate::policy::PolicySpecError)>,
    /// Stored rows naming a bucket this server does not configure. They govern
    /// nothing.
    pub unknown_buckets: Vec<String>,
}

impl PolicyReloadReport {
    /// Whether every stored row was applicable and valid.
    #[must_use]
    pub const fn is_clean(&self) -> bool {
        self.invalid.is_empty() && self.unknown_buckets.is_empty()
    }

    /// Log whatever the pass could not apply. Silence means everything applied.
    pub fn log_problems(&self) {
        for (bucket, error) in &self.invalid {
            tracing::error!(
                bucket = %bucket, error = %error,
                "a stored storage policy is not valid and was NOT applied; the policy already in \
                 force for this bucket keeps serving. Fix or delete the \
                 _fraiseql_storage_policies row."
            );
        }
        for bucket in &self.unknown_buckets {
            tracing::warn!(
                bucket = %bucket,
                "a stored storage policy names a bucket this server does not configure, so it \
                 governs nothing — delete the row or restore the [storage.<name>] section"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// Request body for presigned URL generation.
#[derive(Debug, Deserialize)]
pub struct PresignRequest {
    /// Operation: "upload" (PUT) or "download" (GET).
    pub operation:       String,
    /// MIME type (required for uploads, optional for downloads).
    #[serde(default)]
    pub content_type:    Option<String>,
    /// URL validity duration in seconds (default: 3600, max: 86400).
    #[serde(default = "default_expiry_secs")]
    pub expires_in_secs: u64,
}

const fn default_expiry_secs() -> u64 {
    3600
}

/// Response body for presigned URL generation.
#[derive(Debug, Serialize)]
pub struct PresignResponse {
    /// The presigned URL.
    pub url:        String,
    /// When the URL expires (RFC3339 format).
    pub expires_at: String,
    /// HTTP method this URL is valid for.
    pub method:     String,
}

#[cfg(feature = "aws-s3")]
impl From<PresignedUrl> for PresignResponse {
    fn from(url: PresignedUrl) -> Self {
        Self {
            url:        url.url,
            expires_at: url.expires_at.to_rfc3339(),
            method:     url.method,
        }
    }
}

/// Query parameters for list endpoint.
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// Filter by key prefix.
    pub prefix: Option<String>,
    /// Maximum results (default: 100, max: 1000).
    pub limit:  Option<u32>,
    /// Offset for pagination.
    pub offset: Option<u32>,
}

/// User identity extracted from request (populated by auth middleware).
#[derive(Debug, Clone, Default)]
pub struct StorageUser {
    /// User identifier (sub claim from JWT).
    pub user_id: Option<String>,
    /// User roles.
    pub roles:   Vec<String>,
    /// Normalised token claims, for the `require_claims` policy condition (#974).
    ///
    /// Populated only on the OIDC validation path. Static-token and API-key
    /// auth leave it empty, so a rule requiring a claim denies under those
    /// modes rather than silently ceasing to narrow.
    pub claims:  crate::policy::ClaimValues,
}

impl StorageUser {
    /// The access-decision context for this user at `now`.
    ///
    /// Built here rather than at each call site so that a route cannot forget
    /// to pass the claims along, which would turn every `require_claims` rule
    /// into a silent denial on that one path.
    #[must_use]
    pub fn caller(&self, now: chrono::DateTime<chrono::Utc>) -> crate::StorageCaller<'_> {
        crate::StorageCaller::new(self.user_id.as_deref(), self.roles.as_slice(), &self.claims, now)
    }
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Upload body limit applied when no configured bucket sets an explicit
/// `max_object_bytes` (or none are configured). Mirrors the legacy storage
/// route default; larger objects should use presigned direct-to-backend
/// uploads rather than the buffered server-side route.
const DEFAULT_STORAGE_BODY_LIMIT: usize = 100 * 1024 * 1024; // 100 MiB

/// Compute the per-route upload body limit from the configured buckets.
///
/// Returns the largest explicit `max_object_bytes` across all buckets, or
/// [`DEFAULT_STORAGE_BODY_LIMIT`] when any bucket is unlimited (or no buckets
/// are configured). Sizing the route limit to the largest bucket keeps each
/// bucket's own `max_object_bytes` check reachable — smaller buckets are still
/// enforced per-request by [`put_handler`].
fn storage_body_limit(buckets: &HashMap<String, BucketConfig>) -> usize {
    let mut max_explicit: u64 = 0;
    for bucket in buckets.values() {
        match bucket.max_object_bytes {
            Some(n) => max_explicit = max_explicit.max(n),
            None => return DEFAULT_STORAGE_BODY_LIMIT, // an unlimited bucket is present
        }
    }
    if max_explicit == 0 {
        DEFAULT_STORAGE_BODY_LIMIT // no buckets configured
    } else {
        usize::try_from(max_explicit).unwrap_or(DEFAULT_STORAGE_BODY_LIMIT)
    }
}

/// Build the storage HTTP router.
///
/// Returns an `axum::Router` that handles all storage endpoints.
/// The caller is responsible for applying authentication middleware
/// that populates `StorageUser` in request extensions.
pub fn storage_router(state: StorageState) -> Router {
    // #338: the storage handlers buffer the whole body into memory and enforce
    // per-bucket `max_object_bytes`. Apply a per-route body limit sized to the
    // largest configured bucket so those checks are reachable; being applied on
    // this router (inner) it overrides the server-wide `DefaultBodyLimit` (and
    // axum's 2 MiB default) for storage routes only.
    // Sized once, from the boot configuration: `max_object_bytes` is not among
    // the fields a policy push can change (#974).
    let body_limit = storage_body_limit(&state.buckets.load());
    Router::new()
        .route(
            "/storage/v1/object/{bucket}/{*key}",
            put(put_handler).get(get_handler).delete(delete_handler),
        )
        .route("/storage/v1/list/{bucket}", get(list_handler))
        .route("/storage/v1/presign/{bucket}/{*key}", post(presign_handler))
        // Image renders (#370): present only when the `transforms` feature is
        // compiled in; without it the path 404s like any unknown route.
        .merge(render_routes())
        // Resumable (Tus) uploads (#369). Creation addresses the object key;
        // the session endpoints address the upload id. The two shapes differ
        // in arity, so axum accepts both under the same prefix.
        .route("/storage/v1/uploads/{bucket}/{*key}", post(uploads::create_upload_handler))
        .route(
            "/storage/v1/uploads/{id}",
            axum::routing::patch(uploads::patch_upload_handler)
                .head(uploads::head_upload_handler)
                .delete(uploads::delete_upload_handler),
        )
        .layer(DefaultBodyLimit::max(body_limit))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Upload an object.
#[tracing::instrument(skip(state, user, headers, body), fields(bucket = %bucket_name, key = %key))]
async fn put_handler(
    State(state): State<StorageState>,
    user: Option<Extension<StorageUser>>,
    Path((bucket_name, key)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(rejection) = reject_unsafe_key(&key) {
        return rejection;
    }

    // Snapshot the bucket map for the whole request: a policy pushed over the
    // admin API mid-request must not decide half of it (#974).
    let buckets = state.buckets.load();
    let Some(bucket) = buckets.get(&bucket_name) else {
        return error_response(StatusCode::NOT_FOUND, "bucket_not_found", "Bucket not found");
    };

    let user = user.map(|Extension(u)| u).unwrap_or_default();

    // Load any existing object so an overwrite is gated on ownership, not just on the
    // bucket-level write permission (H9 overwrite IDOR). Done before any backend work.
    let existing = match state.metadata.get(&bucket_name, &key).await {
        Ok(existing) => existing,
        Err(e) => return storage_error_response(&e),
    };

    // RLS: create requires authentication; overwrite requires owner or admin.
    if !state.rls.can_write_object(
        &user.caller(chrono::Utc::now()),
        bucket,
        &key,
        existing.as_ref(),
    ) {
        tracing::warn!(
            bucket = %bucket_name,
            key = %key,
            user_id = ?user.user_id,
            overwrite = existing.is_some(),
            "Storage upload denied"
        );
        // Anonymous callers always get 401 (no existence oracle); an authenticated
        // non-owner attempting an overwrite gets 403.
        return if user.user_id.is_none() {
            error_response(StatusCode::UNAUTHORIZED, "unauthorized", "Authentication required")
        } else {
            error_response(StatusCode::FORBIDDEN, "forbidden", "Access denied")
        };
    }

    // Validate size
    if let Some(max_bytes) = bucket.max_object_bytes {
        if body.len() as u64 > max_bytes {
            tracing::warn!(
                bucket = %bucket_name,
                key = %key,
                size = body.len(),
                max_bytes = max_bytes,
                "Storage upload rejected: payload too large"
            );
            return error_response(
                StatusCode::PAYLOAD_TOO_LARGE,
                "payload_too_large",
                "Object exceeds maximum size",
            );
        }
    }

    // Determine content type
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream");

    // Validate MIME type against the bucket's single policy (#876).
    if !bucket.allows_mime(content_type) {
        tracing::warn!(
            bucket = %bucket_name,
            key = %key,
            content_type = %content_type,
            "Storage upload rejected: MIME type not allowed"
        );
        return error_response(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "mime_type_rejected",
            "Content type not allowed for this bucket",
        );
    }

    // Upload to backend. #336: scope the backend key by bucket so two buckets
    // cannot collide on the same object key.
    let object_key = backend_object_key(&bucket_name, &key);
    let etag = match state.backend.upload(&object_key, &body, content_type).await {
        Ok(etag) => etag,
        Err(e) => return storage_error_response(&e),
    };

    // Record metadata
    let new_obj = NewStorageObject {
        bucket: bucket_name,
        key,
        content_type: content_type.to_string(),
        // Reason: body length is bounded by max_object_bytes config (set elsewhere); i64
        // capacity is 9.2 EB so wrap is unreachable.
        #[allow(clippy::cast_possible_wrap)]
        size_bytes: body.len() as i64,
        etag: Some(etag.clone()),
        owner_id: user.user_id,
    };
    if let Err(e) = state.metadata.upsert(&new_obj).await {
        return storage_error_response(&e);
    }

    let mut headers = HeaderMap::new();
    if let Ok(val) = etag.parse() {
        headers.insert(header::ETAG, val);
    }
    (StatusCode::OK, headers).into_response()
}

/// Download an object.
#[tracing::instrument(skip(state, user), fields(bucket = %bucket_name, key = %key))]
async fn get_handler(
    State(state): State<StorageState>,
    user: Option<Extension<StorageUser>>,
    Path((bucket_name, key)): Path<(String, String)>,
) -> Response {
    if let Some(rejection) = reject_unsafe_key(&key) {
        return rejection;
    }

    // Snapshot the bucket map for the whole request: a policy pushed over the
    // admin API mid-request must not decide half of it (#974).
    let buckets = state.buckets.load();
    let Some(bucket) = buckets.get(&bucket_name) else {
        return error_response(StatusCode::NOT_FOUND, "bucket_not_found", "Bucket not found");
    };

    let user = user.map(|Extension(u)| u).unwrap_or_default();

    // Look up metadata for RLS check. Missing and not-yours are the same
    // answer (#876), so neither reveals which one it was.
    let row = match state.metadata.get(&bucket_name, &key).await {
        Ok(Some(row)) => row,
        Ok(None) => return object_not_visible(bucket, &user),
        Err(e) => return storage_error_response(&e),
    };

    if !state.rls.can_read(&user.caller(chrono::Utc::now()), bucket, &row) {
        tracing::warn!(
            bucket = %bucket_name,
            key = %key,
            user_id = ?user.user_id,
            "Storage download denied: access forbidden"
        );
        return object_not_visible(bucket, &user);
    }

    // Download from backend (#336: bucket-scoped key).
    match state.backend.download(&backend_object_key(&bucket_name, &key)).await {
        Ok(data) => {
            // #866: a presigned upload wrote these bytes straight to the
            // backend, so the reservation made at signing time carries
            // placeholder size/etag. This is the first point at which the
            // server has the object, so settle the row here rather than serve
            // metadata that contradicts the body.
            let etag = if row.pending {
                let etag = object_etag(&data);
                // Reason: `data.len()` is bounded by the bucket's max_object_bytes and by
                // the route body limit; i64 capacity is 9.2 EB, so the cast cannot wrap.
                #[allow(clippy::cast_possible_wrap)]
                let size = data.len() as i64;
                if let Err(e) =
                    state.metadata.confirm(row.pk_storage_object, size, Some(&etag)).await
                {
                    return storage_error_response(&e);
                }
                Some(etag)
            } else {
                row.etag.clone()
            };

            let mut headers = HeaderMap::new();
            if let Ok(ct) = row.content_type.parse() {
                headers.insert(header::CONTENT_TYPE, ct);
            }
            if let Some(ref etag) = etag {
                if let Ok(val) = etag.parse() {
                    headers.insert(header::ETAG, val);
                }
            }
            // #608: branch the cache directive on the bucket's access mode. A
            // `Private` object's per-request RLS decision (`can_read`, above) is
            // per-row, so a URL-keyed shared cache (CDN/reverse/forward proxy)
            // cannot represent the boundary — advertising `public` would let it
            // store the private object and serve it to unauthenticated third
            // parties, and would delay revocation for up to `max-age`. `no-store`
            // is the conservative directive; `PublicRead` stays publicly cacheable.
            let cache_control = match bucket.access {
                crate::config::BucketAccess::Private => "private, no-store",
                crate::config::BucketAccess::PublicRead => "public, max-age=3600",
            };
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache_control));
            // #337: defang stored content. `nosniff` stops browsers from
            // MIME-sniffing the body into an executable type. The default
            // `attachment` disposition forces a download rather than inline
            // rendering; a bucket may opt into inline rendering, but content
            // types that can execute as active content stay attachments.
            headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
            let disposition = if bucket.serve_inline && !is_inline_unsafe(&row.content_type) {
                "inline"
            } else {
                "attachment"
            };
            headers.insert(header::CONTENT_DISPOSITION, HeaderValue::from_static(disposition));
            (StatusCode::OK, headers, Body::from(data)).into_response()
        },
        Err(e) => storage_error_response(&e),
    }
}

/// Delete an object.
#[tracing::instrument(skip(state, user), fields(bucket = %bucket_name, key = %key))]
async fn delete_handler(
    State(state): State<StorageState>,
    user: Option<Extension<StorageUser>>,
    Path((bucket_name, key)): Path<(String, String)>,
) -> Response {
    if let Some(rejection) = reject_unsafe_key(&key) {
        return rejection;
    }

    // Snapshot the bucket map for the whole request: a policy pushed over the
    // admin API mid-request must not decide half of it (#974).
    let buckets = state.buckets.load();
    let Some(bucket) = buckets.get(&bucket_name) else {
        return error_response(StatusCode::NOT_FOUND, "bucket_not_found", "Bucket not found");
    };

    // Look up metadata for RLS check
    let row = match state.metadata.get(&bucket_name, &key).await {
        Ok(Some(row)) => row,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "not_found", "Object not found"),
        Err(e) => return storage_error_response(&e),
    };

    let user = user.map(|Extension(u)| u).unwrap_or_default();
    if !state.rls.can_delete(&user.caller(chrono::Utc::now()), bucket, &row) {
        tracing::warn!(
            bucket = %bucket_name,
            key = %key,
            user_id = ?user.user_id,
            "Storage delete denied: access forbidden"
        );
        return error_response(StatusCode::FORBIDDEN, "forbidden", "Access denied");
    }

    // Delete from backend (#336: bucket-scoped key).
    //
    // A `NotFound` from the backend is not a failure here. #866's reservations
    // record ownership *before* the bytes exist, so a claim whose upload never
    // happened is a metadata row with nothing behind it — propagating the
    // backend's 404 would leave that row in place and squat the key against its
    // own owner permanently. The caller has already been authorised to delete
    // this object; the outcome it asks for is "gone", which is satisfied. Every
    // other backend error still refuses, so a genuine failure cannot silently
    // orphan the bytes by dropping their metadata.
    if let Err(e) = state.backend.delete(&backend_object_key(&bucket_name, &key)).await {
        if matches!(e, FraiseQLError::File(FileError::NotFound { .. })) {
            tracing::debug!(
                bucket = %bucket_name,
                key = %key,
                pending = row.pending,
                "Storage delete: no object in the backing store; releasing the metadata row"
            );
        } else {
            return storage_error_response(&e);
        }
    }

    // Remove metadata
    if let Err(e) = state.metadata.delete(&bucket_name, &key).await {
        return storage_error_response(&e);
    }

    StatusCode::NO_CONTENT.into_response()
}

/// List objects in a bucket.
#[tracing::instrument(skip(state, user, query), fields(bucket = %bucket_name))]
async fn list_handler(
    State(state): State<StorageState>,
    user: Option<Extension<StorageUser>>,
    Path(bucket_name): Path<String>,
    Query(query): Query<ListQuery>,
) -> Response {
    // Snapshot the bucket map for the whole request: a policy pushed over the
    // admin API mid-request must not decide half of it (#974).
    let buckets = state.buckets.load();
    let Some(bucket) = buckets.get(&bucket_name) else {
        return error_response(StatusCode::NOT_FOUND, "bucket_not_found", "Bucket not found");
    };

    let user = user.map(|Extension(u)| u).unwrap_or_default();
    // The door: whether this caller may list at all. Which rows come back is
    // `filter_visible`'s decision, applied below. Under a bucket policy `list`
    // is its own method (#371) — it is no longer implied by write access.
    if !state.rls.can_list(
        &user.caller(chrono::Utc::now()),
        bucket,
        query.prefix.as_deref().unwrap_or(""),
    ) {
        return if user.user_id.is_none() {
            error_response(StatusCode::UNAUTHORIZED, "unauthorized", "Authentication required")
        } else {
            error_response(StatusCode::FORBIDDEN, "forbidden", "Access denied")
        };
    }

    let limit = query.limit.unwrap_or(100).min(1000);
    let offset = query.offset.unwrap_or(0);

    let rows = match state.metadata.list(&bucket_name, query.prefix.as_deref(), limit, offset).await
    {
        Ok(rows) => rows,
        Err(e) => return storage_error_response(&e),
    };

    // Apply RLS filtering
    let visible = state.rls.filter_visible(&user.caller(chrono::Utc::now()), bucket, rows);

    let items: Vec<ListItem> = visible.iter().map(ListItem::from).collect();
    axum::Json(items).into_response()
}

/// Generate a presigned URL.
///
/// Pre-v2.4.0 this handler bypassed [`StorageRlsEvaluator`] entirely: any
/// anonymous client could presign GET / PUT against any bucket+key,
/// returning a 24-hour-valid URL for objects in `BucketAccess::Private`
/// buckets owned by other users (#335).  The handler now mirrors the
/// access-control shape of [`put_handler`] / [`get_handler`]:
///
/// - For `operation = "download"`: the metadata row is loaded and `state.rls.can_read` is consulted
///   before signing. Missing objects yield `404`; objects the caller may not read yield `403`.
/// - For `operation = "upload"`: the metadata row is loaded and `state.rls.can_write_object` is
///   consulted before signing — creating a new object needs bucket-level write permission, but
///   overwriting an existing one needs owner or admin (B4: otherwise a presigned PUT is an
///   overwrite IDOR). A non-owner overwrite yields `403`.
///
/// # Caveat — bucket constraints are NOT enforced via S3 presigned PUT.
///
/// The S3 presigned PUT URL gives the holder the same effective
/// authority as the FraiseQL server for the bucket+key window: any
/// `Content-Type` and any body size accepted by S3 itself goes through.
/// FraiseQL's bucket-level `max_object_bytes` and `allowed_mime_types`
/// checks live in [`put_handler`] and cannot be encoded in a vanilla S3
/// presigned PUT.  Operators who need those constraints enforced for
/// presigned uploads must (a) restrict presigned uploads to trusted
/// users via RLS, (b) re-validate after the upload via metadata
/// inspection + cleanup, or (c) route uploads through `PUT /storage/v1/{bucket}/{*key}`
/// instead.  This is documented as a known limitation in CHANGELOG.
#[tracing::instrument(skip(state, user, request), fields(bucket = %bucket_name, key = %key))]
async fn presign_handler(
    State(state): State<StorageState>,
    user: Option<Extension<StorageUser>>,
    Path((bucket_name, key)): Path<(String, String)>,
    axum::Json(request): axum::Json<PresignRequest>,
) -> Response {
    if let Some(rejection) = reject_unsafe_key(&key) {
        return rejection;
    }

    // Snapshot the bucket map for the whole request: a policy pushed over the
    // admin API mid-request must not decide half of it (#974).
    let buckets = state.buckets.load();
    let Some(bucket) = buckets.get(&bucket_name) else {
        return error_response(StatusCode::NOT_FOUND, "bucket_not_found", "Bucket not found");
    };

    let user = user.map(|Extension(u)| u).unwrap_or_default();

    // Validate operation
    let operation = request.operation.to_lowercase();
    if operation != "upload" && operation != "download" {
        return error_response(
            StatusCode::BAD_REQUEST,
            "invalid_operation",
            "operation must be 'upload' or 'download'",
        );
    }

    if request.expires_in_secs == 0 || request.expires_in_secs > 86400 {
        return error_response(
            StatusCode::BAD_REQUEST,
            "invalid_expiry",
            "expires_in_secs must be between 1 and 86400",
        );
    }

    // RLS gate.  Mirrors put_handler / get_handler.  Done before any S3 work,
    // and answering identically whether the object is missing or merely not
    // the caller's, so unauthorised callers cannot observe which it is (#876).
    //
    // Yields the row the decision was made against — `None` meaning "no object
    // here", which is what makes the claim below a create rather than an
    // overwrite.
    let authorised_against: Option<i64> = if operation == "upload" {
        // B4: a presign(upload) that would overwrite an existing object must be gated
        // on ownership, exactly like put_handler — otherwise a leaked/guessed key lets
        // any authenticated user presign-overwrite another user's object.
        let existing = match state.metadata.get(&bucket_name, &key).await {
            Ok(existing) => existing,
            Err(e) => return storage_error_response(&e),
        };
        if !state.rls.can_write_object(
            &user.caller(chrono::Utc::now()).through_signed_url(),
            bucket,
            &key,
            existing.as_ref(),
        ) {
            tracing::warn!(
                bucket = %bucket_name,
                key = %key,
                user_id = ?user.user_id,
                overwrite = existing.is_some(),
                "Storage presign(upload) denied"
            );
            return if user.user_id.is_none() {
                error_response(StatusCode::UNAUTHORIZED, "unauthorized", "Authentication required")
            } else {
                error_response(StatusCode::FORBIDDEN, "forbidden", "Access denied")
            };
        }

        existing.as_ref().map(|row| row.pk_storage_object)
    } else {
        // download: look up metadata so can_read can apply per-row policy.
        // Missing and not-yours give the same answer (#876).
        let row = match state.metadata.get(&bucket_name, &key).await {
            Ok(Some(row)) => row,
            Ok(None) => return object_not_visible(bucket, &user),
            Err(e) => return storage_error_response(&e),
        };
        if !state
            .rls
            .can_read(&user.caller(chrono::Utc::now()).through_signed_url(), bucket, &row)
        {
            tracing::warn!(
                bucket = %bucket_name,
                key = %key,
                user_id = ?user.user_id,
                "Storage presign(download) denied by RLS"
            );
            return object_not_visible(bucket, &user);
        }
        None
    };

    #[cfg(feature = "aws-s3")]
    {
        use std::time::Duration;
        let expires_in = Duration::from_secs(request.expires_in_secs);
        // #336: scope the presigned key by bucket so the signed URL targets the
        // same backend object the PUT/GET handlers use.
        let object_key = backend_object_key(&bucket_name, &key);

        // The row claimed for this upload, and whether this request created it —
        // a signing failure must release a claim it made, but leave one that was
        // already there.
        let mut reservation: Option<(i64, bool)> = None;

        let result = if operation == "upload" {
            // Every remaining way to refuse comes first: nothing may be claimed
            // for a request that cannot be signed, or the key is squatted under
            // the caller's name for an upload that never happens.
            let Some(content_type) = request.content_type else {
                return error_response(
                    StatusCode::BAD_REQUEST,
                    "missing_content_type",
                    "content_type required for upload",
                );
            };

            // #866: claim the object BEFORE handing out the URL. The upload
            // bypasses the server entirely, so this is the last moment at which
            // the owner can be recorded — and without a row `can_write_object`
            // reads the next caller as a creator, voiding the H9/B4 overwrite
            // gate for exactly the door it is named after.
            let claim = NewStorageObject {
                bucket:       bucket_name.clone(),
                key:          key.clone(),
                content_type: content_type.clone(),
                size_bytes:   0,
                etag:         None,
                owner_id:     user.user_id.clone(),
            };
            match state.metadata.reserve(&claim, authorised_against).await {
                Ok(Some(pk)) => reservation = Some((pk, authorised_against.is_none())),
                Ok(None) => {
                    // Another writer changed the row between the gate and the
                    // claim. Refuse rather than sign against an authorization
                    // decision that is no longer true.
                    tracing::warn!(
                        bucket = %bucket_name,
                        key = %key,
                        user_id = ?user.user_id,
                        "Storage presign(upload) lost a race for the object; refusing"
                    );
                    return error_response(
                        StatusCode::CONFLICT,
                        "conflict",
                        "The object changed while the request was being authorized; retry",
                    );
                },
                Err(e) => return storage_error_response(&e),
            }

            state.backend.presign_put(&object_key, &content_type, expires_in).await
        } else {
            state.backend.presign_get(&object_key, expires_in).await
        };

        match result {
            Ok(url) => axum::Json(PresignResponse::from(url)).into_response(),
            Err(e) => {
                // Signing failed, so the upload can never happen. A claim we
                // *created* would hold the key against its owner forever;
                // release it. A claim over a pre-existing object stays put —
                // those bytes are still there, and the next read settles it.
                if let Some((pk, created)) = reservation {
                    if created {
                        if let Err(release) = state.metadata.release_reservation(pk).await {
                            tracing::error!(
                                bucket = %bucket_name,
                                key = %key,
                                error = %release,
                                "Failed to release a storage reservation after a signing error"
                            );
                        }
                    }
                }
                storage_error_response(&e)
            },
        }
    }

    #[cfg(not(feature = "aws-s3"))]
    {
        let _ = (key, operation, request, authorised_against);
        error_response(
            StatusCode::NOT_IMPLEMENTED,
            "not_supported",
            "Presigned URLs require S3 backend",
        )
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// List item returned in JSON array from list endpoint.
#[derive(Debug, Serialize)]
struct ListItem {
    key:          String,
    size:         i64,
    content_type: String,
    etag:         Option<String>,
    /// The key is claimed by an in-flight presigned upload (#866): it has an
    /// owner, but `size`/`etag` are placeholders until the object is first read.
    /// Reported rather than hidden — an owner who has just presigned an upload
    /// must be able to see the claim, and a caller must be able to tell a
    /// settled object from an unsettled one.
    pending:      bool,
    created_at:   String,
    updated_at:   String,
}

impl From<&StorageMetadataRow> for ListItem {
    fn from(row: &StorageMetadataRow) -> Self {
        Self {
            key:          row.key.clone(),
            size:         row.size_bytes,
            content_type: row.content_type.clone(),
            etag:         row.etag.clone(),
            pending:      row.pending,
            created_at:   row.created_at.to_rfc3339(),
            updated_at:   row.updated_at.to_rfc3339(),
        }
    }
}

/// The render route, when the `transforms` feature is compiled in.
#[cfg(feature = "transforms")]
fn render_routes() -> Router<StorageState> {
    Router::new().route("/storage/v1/render/{bucket}/{*key}", get(render::render_handler))
}

/// Without the `transforms` feature there is no render surface at all.
#[cfg(not(feature = "transforms"))]
fn render_routes() -> Router<StorageState> {
    Router::new()
}

/// Build a JSON error response.
fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    let body = serde_json::json!({
        "error": {
            "code": code,
            "message": message,
        }
    });
    (status, axum::Json(body)).into_response()
}

/// Convert a `FraiseQLError` to an appropriate HTTP response.
///
/// After F050 (typed `FileError` migration), backend storage failures arrive
/// as `FraiseQLError::File(FileError::*)` rather than `FraiseQLError::Storage`.
/// The routing here matches the previous behaviour of
/// `Storage { code: Some("...") }`:
///
/// - `FileError::NotFound` → 404
/// - `FileError::PermissionDenied` → 403
/// - other backend variants (`IoError`, `Backend`, `NotImplemented`, `Unsupported`,
///   `SizeLimitExceeded`, `MimeTypeNotAllowed`) → 500
/// - `FileError::InvalidKey` → 400
fn storage_error_response(err: &FraiseQLError) -> Response {
    if let FraiseQLError::File(file_err) = err {
        let (status, code) = match file_err {
            FileError::NotFound { .. } => (StatusCode::NOT_FOUND, "not_found"),
            FileError::PermissionDenied { .. } => (StatusCode::FORBIDDEN, "permission_denied"),
            FileError::InvalidKey { .. } => (StatusCode::BAD_REQUEST, "invalid_key"),
            FileError::IoError { .. } => {
                tracing::error!(error = %err, "Storage I/O error");
                (StatusCode::INTERNAL_SERVER_ERROR, "io_error")
            },
            FileError::NotImplemented { .. } => {
                (StatusCode::INTERNAL_SERVER_ERROR, "not_implemented")
            },
            FileError::Unsupported { .. } => (StatusCode::INTERNAL_SERVER_ERROR, "not_supported"),
            FileError::SizeLimitExceeded { .. } => {
                (StatusCode::INTERNAL_SERVER_ERROR, "size_limit_exceeded")
            },
            FileError::MimeTypeNotAllowed { .. } => {
                (StatusCode::INTERNAL_SERVER_ERROR, "mime_type_not_allowed")
            },
            FileError::Backend { .. } => {
                tracing::error!(error = %err, "Storage backend error");
                (StatusCode::INTERNAL_SERVER_ERROR, "storage_error")
            },
            // Pre-F050 FileError variants — unlikely to reach the storage
            // routes but handled for completeness.
            FileError::TooLarge { .. } => (StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large"),
            FileError::QuotaExceeded => (StatusCode::PAYLOAD_TOO_LARGE, "quota_exceeded"),
            FileError::InvalidType { .. } | FileError::MimeMismatch { .. } => {
                (StatusCode::UNSUPPORTED_MEDIA_TYPE, "invalid_type")
            },
            FileError::VirusDetected { .. } => (StatusCode::UNPROCESSABLE_ENTITY, "virus_detected"),
            FileError::Storage { .. } | FileError::Processing { .. } => {
                tracing::error!(error = %err, "Storage backend error");
                (StatusCode::INTERNAL_SERVER_ERROR, "storage_error")
            },
            // SECURITY: `FileError` is `#[non_exhaustive]`. Any future variant
            // added without updating this match falls through to a generic
            // 500 response rather than silently leaking the wrong status.
            _ => {
                tracing::error!(error = %err, "Unhandled FileError variant");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            },
        };
        error_response(status, code, &file_err.to_string())
    } else {
        tracing::error!(error = %err, "Unexpected storage error");
        error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", &err.to_string())
    }
}

/// The response for "you cannot see this object", whether it is missing or
/// merely not yours.
///
/// #876: `get_handler` answered `404` for a missing object and `403` for one
/// the caller may not read, so an unauthenticated attacker could enumerate a
/// private bucket's keys — often themselves sensitive (customer names, invoice
/// numbers) — with no credentials. `presign(download)` had the same split, and
/// its own comment claimed the opposite ("unauthorised callers cannot observe
/// whether the object exists"). Both now collapse the two cases, exactly as
/// `put_handler` already did.
///
/// A `PublicRead` bucket has no boundary to leak — `can_read` is unconditional
/// there — so it keeps the plain `404`.
fn object_not_visible(bucket: &BucketConfig, user: &StorageUser) -> Response {
    if !bucket.allows_anonymous_read() && user.user_id.is_none() {
        error_response(StatusCode::UNAUTHORIZED, "unauthorized", "Authentication required")
    } else {
        error_response(StatusCode::NOT_FOUND, "not_found", "Object not found")
    }
}

/// Reject a client-supplied object key that is unsafe or aliasing, before any
/// metadata or backend work.
///
/// #813: the backends validate the *composed* `"{bucket}/{key}"` deep inside
/// their I/O methods, which leaves the handlers' own metadata lookups — keyed
/// on the raw string — reachable with a key the backend would later refuse.
/// `GET`/`DELETE` answered `404` from the metadata probe and `presign` signed
/// (or 501'd) without ever consulting the validator. Validating the raw key
/// here means the metadata key and the backend key are the same canonical
/// string on every path, and an unusable key costs one 400 instead of a
/// partially-applied request.
fn reject_unsafe_key(key: &str) -> Option<Response> {
    match crate::backend::validate_key(key) {
        Ok(()) => None,
        Err(e) => {
            tracing::warn!(key = %key, error = %e, "Storage request rejected: unsafe object key");
            Some(storage_error_response(&e))
        },
    }
}

/// Compose the backend object key from the bucket name and the object key.
///
/// Buckets are an isolation boundary (#336): two objects with the same key in
/// different buckets must map to distinct backend keys. The metadata table
/// keys on `(bucket, key)`, but the object store is a single shared backing
/// store — prefixing the bucket name scopes the backend key so a key in one
/// bucket cannot overwrite or shadow the same key in another.
fn backend_object_key(bucket: &str, key: &str) -> String {
    format!("{bucket}/{key}")
}

/// Content hash used as the etag when reconciling a presign-uploaded object.
///
/// The bytes never passed through the server at upload time, so there is no
/// backend-reported etag to record; this is computed from the object the read
/// actually served, which is the thing the etag has to identify.
fn object_etag(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(data);
    // The first 16 bytes of the digest: plenty for an entity tag, and short
    // enough to read. `get` rather than a slice so the crate-wide
    // `indexing_slicing` deny holds; SHA-256 is always 32 bytes, so the
    // fallback is unreachable.
    let truncated = digest.get(..16).unwrap_or(&digest);
    format!("\"{}\"", hex::encode(truncated))
}

/// Content types a browser may execute as active content when rendered
/// inline. These are always served as `attachment` regardless of bucket
/// configuration, to neutralise stored XSS (#337).
fn is_inline_unsafe(content_type: &str) -> bool {
    // Compare only the media type, ignoring parameters like "; charset=utf-8".
    let base = content_type.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    matches!(
        base.as_str(),
        "text/html" | "application/xhtml+xml" | "image/svg+xml" | "application/xml" | "text/xml"
    )
}
