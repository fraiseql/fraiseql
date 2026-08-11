//! Row-level security enforcement for storage operations.
//!
//! Evaluates access control policies against bucket configuration and
//! object ownership. Follows the "RLS always wins" principle — deny-by-default.

#[cfg(test)]
mod tests;

use chrono::{DateTime, Utc};

use crate::{
    config::{BucketAccess, BucketConfig},
    metadata::StorageMetadataRow,
    policy::{ClaimValues, PolicyMethod, PolicyRequest},
};

/// The storage admin role that bypasses all object-level access checks.
///
/// This is an explicit, storage-namespaced role rather than the generic `"admin"`.
/// The server maps an OIDC token's `scopes` verbatim into a [`StorageUser`](crate::StorageUser)'s
/// `roles`, so a generic role name like `"admin"` — a common scope in many `IdPs` and apps —
/// would otherwise grant *full* storage access (read/overwrite/delete any object in any bucket)
/// to any caller whose token happened to carry it (M-storage-scope). Requiring the explicit
/// `fraiseql:storage:admin` role makes the storage-admin grant intentional, not an accidental
/// collision with an unrelated application scope.
pub const STORAGE_ADMIN_ROLE: &str = "fraiseql:storage:admin";

/// Who is asking, and when.
///
/// Bundled rather than passed as loose arguments because #974 added three more
/// facts a decision depends on (the caller's claims, the deciding instant, and
/// whether the request came through a signed URL), and threading five
/// parameters through every `can_*` method is how one call site ends up
/// quietly passing the wrong one.
///
/// `now` is carried here rather than read from the clock inside the evaluator
/// so that a time-bounded grant is testable without sleeping, and so a single
/// request cannot straddle two different "now"s.
#[derive(Debug, Clone, Copy)]
pub struct StorageCaller<'a> {
    /// The authenticated caller, if any.
    pub user_id:        Option<&'a str>,
    /// The caller's roles (an OIDC token's scopes, verbatim).
    pub roles:          &'a [String],
    /// The caller's normalised token claims; empty when the auth path has none.
    pub claims:         &'a ClaimValues,
    /// The instant this decision is made at.
    pub now:            DateTime<Utc>,
    /// Whether the request arrived through the signed-URL path.
    pub via_signed_url: bool,
}

impl<'a> StorageCaller<'a> {
    /// A caller arriving through an ordinary (not signed-URL) request.
    #[must_use]
    pub const fn new(
        user_id: Option<&'a str>,
        roles: &'a [String],
        claims: &'a ClaimValues,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            user_id,
            roles,
            claims,
            now,
            via_signed_url: false,
        }
    }

    /// The same caller, arriving through the signed-URL path.
    ///
    /// Only the presign endpoint sets this: it is what distinguishes *"may this
    /// caller be handed a signed URL"* from *"may this caller download
    /// directly"*, which is the whole point of
    /// [`PolicyPrincipal::SignedUrl`](crate::PolicyPrincipal::SignedUrl).
    #[must_use]
    pub const fn through_signed_url(mut self) -> Self {
        self.via_signed_url = true;
        self
    }
}

/// Storage RLS evaluator.
///
/// Stateless evaluator that checks access policies:
/// - **Public buckets**: anonymous reads allowed; writes require authentication
/// - **Private buckets**: reads require owner match or admin role
/// - **Writes**: always require an authenticated user
/// - **Deletes**: require owner match or admin role
#[derive(Debug, Clone, Copy)]
pub struct StorageRlsEvaluator;

impl StorageRlsEvaluator {
    /// Create a new evaluator.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Check if the user can read the given object.
    ///
    /// Rules:
    /// - Public bucket → always allowed (even anonymous)
    /// - Private bucket → owner match or admin role required
    #[must_use]
    pub fn can_read(
        &self,
        caller: &StorageCaller<'_>,
        bucket: &BucketConfig,
        object: &StorageMetadataRow,
    ) -> bool {
        if let Some(decision) = policy_decision(
            PolicyMethod::Read,
            caller,
            bucket,
            &object.key,
            object.owner_id.as_deref(),
            object.expires_at,
        ) {
            return decision;
        }
        match bucket.access {
            BucketAccess::PublicRead => true,
            BucketAccess::Private => is_admin(caller.roles) || is_owner(caller.user_id, object),
        }
    }

    /// Check if the user can write (upload) to the bucket.
    ///
    /// Rules:
    /// - Must be authenticated (`user_id` present)
    /// - Admin role always allowed
    #[must_use]
    pub fn can_write(&self, caller: &StorageCaller<'_>, bucket: &BucketConfig) -> bool {
        self.can_write_key(caller, bucket, "")
    }

    /// [`can_write`](Self::can_write) for a known key, so a policy's
    /// `key_prefix` can narrow the grant. The key-less form is the whole-bucket
    /// question (a prefix-scoped rule does not answer it).
    #[must_use]
    pub fn can_write_key(
        &self,
        caller: &StorageCaller<'_>,
        bucket: &BucketConfig,
        key: &str,
    ) -> bool {
        if let Some(decision) =
            policy_decision(PolicyMethod::Write, caller, bucket, key, None, None)
        {
            return decision;
        }
        if is_admin(caller.roles) {
            return true;
        }
        caller.user_id.is_some()
    }

    /// Check if the user can write (create or overwrite) the given object.
    ///
    /// Object-aware counterpart of [`can_write`](Self::can_write):
    /// - **Create** (no `existing` object): same as [`can_write`](Self::can_write) — admin or any
    ///   authenticated user may create a new object.
    /// - **Overwrite** (an `existing` object): owner match or admin role required, mirroring
    ///   [`can_delete`](Self::can_delete). Without this, any authenticated user could clobber
    ///   another user's object data by writing to its key — an overwrite IDOR (H9; and via the
    ///   presign-upload door, B4).
    #[must_use]
    pub fn can_write_object(
        &self,
        caller: &StorageCaller<'_>,
        bucket: &BucketConfig,
        existing: Option<&StorageMetadataRow>,
    ) -> bool {
        match existing {
            None => self.can_write(caller, bucket),
            Some(object) => {
                // #371: overwriting an EXISTING object is `overwrite`, never
                // `write`. Otherwise the natural rule "authenticated callers
                // may write" re-opens the H9/B4 overwrite IDOR by permitting
                // any authenticated caller to clobber another user's object.
                if let Some(decision) = policy_decision(
                    PolicyMethod::Overwrite,
                    caller,
                    bucket,
                    &object.key,
                    object.owner_id.as_deref(),
                    object.expires_at,
                ) {
                    return decision;
                }
                is_admin(caller.roles) || is_owner(caller.user_id, object)
            },
        }
    }

    /// Check if the user can delete the given object.
    ///
    /// Rules:
    /// - Owner match or admin role required
    #[must_use]
    pub fn can_delete(
        &self,
        caller: &StorageCaller<'_>,
        bucket: &BucketConfig,
        object: &StorageMetadataRow,
    ) -> bool {
        if let Some(decision) = policy_decision(
            PolicyMethod::Delete,
            caller,
            bucket,
            &object.key,
            object.owner_id.as_deref(),
            object.expires_at,
        ) {
            return decision;
        }
        is_admin(caller.roles) || is_owner(caller.user_id, object)
    }

    /// Whether the caller may list the bucket at `prefix` at all.
    ///
    /// Distinct from [`filter_visible`](Self::filter_visible), which decides
    /// *which rows* come back: this is the door. Without a policy the historical
    /// rule stands (a private bucket requires authentication; a public one is
    /// open, with per-row filtering behind it). With a policy, `list` is its own
    /// method and nothing is implied by the write grant.
    #[must_use]
    pub fn can_list(
        &self,
        caller: &StorageCaller<'_>,
        bucket: &BucketConfig,
        prefix: &str,
    ) -> bool {
        if let Some(decision) =
            policy_decision(PolicyMethod::List, caller, bucket, prefix, None, None)
        {
            return decision;
        }
        if is_admin(caller.roles) || caller.user_id.is_some() {
            return true;
        }
        !matches!(bucket.access, BucketAccess::Private)
    }

    /// Filter a list of objects to those visible to the user.
    ///
    /// For public buckets, all objects are visible.
    /// For private buckets, only owned objects (or all if admin).
    #[must_use]
    pub fn filter_visible(
        &self,
        caller: &StorageCaller<'_>,
        bucket: &BucketConfig,
        objects: Vec<StorageMetadataRow>,
    ) -> Vec<StorageMetadataRow> {
        if bucket.policies.is_some() {
            if is_admin(caller.roles) {
                return objects;
            }
            // Per-object, because a `key_prefix` rule makes visibility a
            // property of the key, not of the bucket.
            return objects
                .into_iter()
                .filter(|object| {
                    policy_decision(
                        PolicyMethod::Read,
                        caller,
                        bucket,
                        &object.key,
                        object.owner_id.as_deref(),
                        object.expires_at,
                    )
                    .unwrap_or(false)
                })
                .collect();
        }
        match bucket.access {
            BucketAccess::PublicRead => objects,
            BucketAccess::Private => {
                if is_admin(caller.roles) {
                    return objects;
                }
                objects.into_iter().filter(|obj| is_owner(caller.user_id, obj)).collect()
            },
        }
    }
}

impl Default for StorageRlsEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

/// The policy decision for a bucket, or `None` when the bucket has no policy
/// (the caller then falls back to the coarse `access` mode).
///
/// The storage-admin role bypasses policies exactly as it bypasses the access
/// mode — it is the deliberate global grant documented on
/// [`STORAGE_ADMIN_ROLE`], not an accidental hole; an operator who wants
/// no bypass must not grant that role.
fn policy_decision(
    method: PolicyMethod,
    caller: &StorageCaller<'_>,
    bucket: &BucketConfig,
    key: &str,
    owner_id: Option<&str>,
    expires_at: Option<DateTime<Utc>>,
) -> Option<bool> {
    let policy = bucket.policies.as_ref()?;
    if is_admin(caller.roles) {
        return Some(true);
    }
    Some(policy.permits(
        method,
        &PolicyRequest {
            user_id: caller.user_id,
            roles: caller.roles,
            key,
            owner_id,
            now: caller.now,
            expires_at,
            claims: caller.claims,
            via_signed_url: caller.via_signed_url,
        },
    ))
}

/// Check if the roles contain the storage admin role.
fn is_admin(roles: &[String]) -> bool {
    roles.iter().any(|r| r == STORAGE_ADMIN_ROLE)
}

/// Check if the user owns the object.
fn is_owner(user_id: Option<&str>, object: &StorageMetadataRow) -> bool {
    match (user_id, &object.owner_id) {
        (Some(uid), Some(owner)) => uid == owner,
        _ => false,
    }
}
