//! Per-bucket storage access policies over the Admin API (#974).
//!
//! ```text
//! GET    /api/v1/admin/storage/{bucket}/policies   (read token)
//! PUT    /api/v1/admin/storage/{bucket}/policies   (write token)
//! DELETE /api/v1/admin/storage/{bucket}/policies   (write token)
//! ```
//!
//! # Why this surface and not `/admin/v1/storage/*`
//!
//! The storage *browser* lives at `/admin/v1/storage/*`, which is the Studio UI
//! surface behind a single token. `/api/v1/admin/*` is the operational Admin
//! API, where reads and writes are separately tokenable
//! (`admin_readonly_token` / `admin_token`). A bucket's access policy is a
//! security control: an operator must be able to hand out the ability to
//! *inspect* what governs a bucket without handing out the ability to *change*
//! it. That is only possible on the split-token surface.
//!
//! # A push cannot refuse to boot, so it refuses at the request
//!
//! #371 made an unparseable policy a startup error, which is what stops a typo
//! from silently becoming a rule that no longer narrows. A policy arriving over
//! HTTP has no boot to refuse, so the equivalent guarantee is enforced here:
//! the policy is parsed **before** anything is written or swapped, and a
//! failure answers `400` naming the offending rule while the policy already in
//! force keeps serving, untouched. The parse is
//! [`fraiseql_storage::parse_policy`] — literally the function the config file
//! goes through — so the two doors cannot come to accept different policies.
//!
//! Order is load-bearing: parse, then persist, then swap. A database failure
//! therefore leaves the running policy alone *and* leaves the store agreeing
//! with it, so a restart converges on what is being enforced.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use fraiseql_storage::{
    PolicyRuleSpec, PolicySource, StorageState, parse_policy, policy_source, policy_to_specs,
};
use serde::{Deserialize, Serialize};

/// What governs a bucket's access, and where it came from.
#[derive(Debug, Serialize)]
pub struct BucketPolicyResponse {
    /// The bucket this describes.
    pub bucket:     String,
    /// Which of the two sources is governing — or `access_mode` when neither
    /// has a policy for this bucket.
    pub source:     PolicySource,
    /// The coarse access mode. It governs only when `source` is `access_mode`;
    /// a policy replaces it entirely.
    pub access:     &'static str,
    /// The rules in force, as they would be written in configuration. `null`
    /// when no policy governs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules:      Option<Vec<PolicyRuleSpec>>,
    /// When the stored policy was last written. Absent unless `source` is
    /// `store`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

/// Body of a `PUT`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PutBucketPolicyRequest {
    /// The complete rule list. It **replaces** whatever governs the bucket
    /// today; there is no merge, and no way to add a single rule.
    ///
    /// An empty list is accepted and is a deliberate lock-down: it permits
    /// nothing. To hand the bucket back to its configured policy, `DELETE`.
    pub rules: Vec<PolicyRuleSpec>,
}

/// Why a policy request was refused.
#[derive(Debug)]
pub enum PolicyApiError {
    /// No such bucket is configured on this server.
    UnknownBucket(String),
    /// The submitted policy is not a policy this server would accept at boot.
    /// The running one was not touched.
    InvalidPolicy {
        /// Which rule is at fault, when the fault is attributable to one.
        rule_index: Option<usize>,
        /// What is wrong with it.
        message:    String,
    },
    /// The policy store could not be read or written.
    Store(String),
}

impl IntoResponse for PolicyApiError {
    fn into_response(self) -> Response {
        let (status, code, body) = match self {
            Self::UnknownBucket(bucket) => (
                StatusCode::NOT_FOUND,
                "bucket_not_found",
                serde_json::json!({
                    "message": format!("no bucket named {bucket:?} is configured"),
                }),
            ),
            Self::InvalidPolicy {
                rule_index,
                message,
            } => (
                StatusCode::BAD_REQUEST,
                "invalid_policy",
                serde_json::json!({
                    "message": message,
                    "rule_index": rule_index,
                    // Stated in the response, not just in the docs: an operator
                    // whose push was refused needs to know the deployment is
                    // still enforcing what it was enforcing a moment ago.
                    "policy_in_force": "unchanged",
                }),
            ),
            Self::Store(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "policy_store_unavailable",
                serde_json::json!({
                    "message": message,
                    "policy_in_force": "unchanged",
                }),
            ),
        };
        let mut payload = body;
        if let Some(object) = payload.as_object_mut() {
            object.insert("error".to_string(), serde_json::Value::from(code));
        }
        (status, Json(payload)).into_response()
    }
}

/// Report what governs `bucket` right now, reading the live bucket map rather
/// than the store: an operator asking "what is enforced" must be told what is
/// enforced.
///
/// # Errors
///
/// [`PolicyApiError::UnknownBucket`] for a bucket this server does not
/// configure; [`PolicyApiError::Store`] if the store cannot be read.
pub async fn get_bucket_policy_handler(
    State(state): State<StorageState>,
    Path(bucket): Path<String>,
) -> Result<Json<BucketPolicyResponse>, PolicyApiError> {
    let stored = state
        .policy_store
        .get(&bucket)
        .await
        .map_err(|e| PolicyApiError::Store(e.to_string()))?;
    describe(&state, &bucket, stored.map(|row| row.updated_at))
        .ok_or(PolicyApiError::UnknownBucket(bucket))
}

/// Replace `bucket`'s policy, wholesale.
///
/// # Errors
///
/// [`PolicyApiError::UnknownBucket`] for an unconfigured bucket;
/// [`PolicyApiError::InvalidPolicy`] when the rules would not survive a boot —
/// in which case nothing is written and nothing is swapped;
/// [`PolicyApiError::Store`] if the write fails, likewise leaving the running
/// policy in place.
pub async fn put_bucket_policy_handler(
    State(state): State<StorageState>,
    Path(bucket): Path<String>,
    Json(raw): Json<serde_json::Value>,
) -> Result<Json<BucketPolicyResponse>, PolicyApiError> {
    if !state.buckets.load().contains_key(&bucket) {
        return Err(PolicyApiError::UnknownBucket(bucket));
    }

    // Deserialized here rather than by an extractor so that a rule carrying a
    // field this server does not know — `require_unexpird`, say — is refused as
    // an invalid *policy*, with the same shape as every other refusal, instead
    // of as an opaque 422 from the JSON layer. A rule quietly missing the
    // condition its author wrote permits more than it reads as permitting,
    // which is the failure the closed vocabulary exists to prevent.
    let body: PutBucketPolicyRequest =
        serde_json::from_value(raw).map_err(|e| PolicyApiError::InvalidPolicy {
            rule_index: None,
            message:    e.to_string(),
        })?;

    // Parse FIRST. Everything after this line changes something; nothing before
    // it does.
    // Parse FIRST. Everything after this line changes something; nothing before
    // it does.
    let policy = parse_policy(&body.rules).map_err(|e| PolicyApiError::InvalidPolicy {
        rule_index: e.rule_index,
        message:    e.message,
    })?;

    let row = state
        .policy_store
        .put(&bucket, &body.rules)
        .await
        .map_err(|e| PolicyApiError::Store(e.to_string()))?;

    if !state.set_bucket_policies(&bucket, &Some(policy)) {
        // Buckets are fixed at boot and the existence check above passed, so
        // reaching here means the map changed underneath us. The row is
        // written, so a restart will pick it up; say so rather than report a
        // success that is not in force.
        return Err(PolicyApiError::Store(format!(
            "bucket {bucket:?} disappeared while the policy was being applied; it is stored and \
             will take effect on restart"
        )));
    }
    tracing::info!(bucket = %bucket, rules = body.rules.len(), "storage bucket policy replaced");

    describe(&state, &bucket, Some(row.updated_at)).ok_or(PolicyApiError::UnknownBucket(bucket))
}

/// Drop `bucket`'s stored policy, handing it back to its configured one — or,
/// with none configured, to the coarse `access` mode.
///
/// This can **widen** access, which is why it is a write-token operation and
/// why the response states the source that now governs.
///
/// # Errors
///
/// [`PolicyApiError::UnknownBucket`] for an unconfigured bucket;
/// [`PolicyApiError::Store`] if the delete fails, in which case the stored
/// policy keeps governing.
pub async fn delete_bucket_policy_handler(
    State(state): State<StorageState>,
    Path(bucket): Path<String>,
) -> Result<Json<BucketPolicyResponse>, PolicyApiError> {
    if !state.buckets.load().contains_key(&bucket) {
        return Err(PolicyApiError::UnknownBucket(bucket));
    }

    let existed = state
        .policy_store
        .delete(&bucket)
        .await
        .map_err(|e| PolicyApiError::Store(e.to_string()))?;

    let reverted = state.config_policy(&bucket).cloned();
    let _applied = state.set_bucket_policies(&bucket, &reverted);
    tracing::info!(
        bucket = %bucket,
        had_stored_policy = existed,
        "storage bucket policy reverted to its configured source"
    );

    describe(&state, &bucket, None).ok_or(PolicyApiError::UnknownBucket(bucket))
}

/// Describe a bucket from the live map. `None` when the bucket is unknown.
fn describe(
    state: &StorageState,
    bucket: &str,
    updated_at: Option<DateTime<Utc>>,
) -> Option<Json<BucketPolicyResponse>> {
    let buckets = state.buckets.load();
    let config = buckets.get(bucket)?;
    let source = policy_source(updated_at.is_some(), state.config_policy(bucket).is_some());
    Some(Json(BucketPolicyResponse {
        bucket: bucket.to_string(),
        source,
        access: config.access.as_str(),
        rules: config.policies.as_ref().map(policy_to_specs),
        updated_at: match source {
            PolicySource::Store => updated_at,
            PolicySource::ConfigFile | PolicySource::AccessMode => None,
        },
    }))
}
