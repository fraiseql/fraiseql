//! Row-level security enforcement for storage operations.
//!
//! Evaluates access control policies against bucket configuration and
//! object ownership. Follows the "RLS always wins" principle — deny-by-default.

#[cfg(test)]
mod tests;

use crate::{
    config::{BucketAccess, BucketConfig},
    metadata::StorageMetadataRow,
    policy::{PolicyMethod, PolicyRequest},
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
        user_id: Option<&str>,
        roles: &[String],
        bucket: &BucketConfig,
        object: &StorageMetadataRow,
    ) -> bool {
        if let Some(decision) = policy_decision(
            PolicyMethod::Read,
            user_id,
            roles,
            bucket,
            &object.key,
            object.owner_id.as_deref(),
        ) {
            return decision;
        }
        match bucket.access {
            BucketAccess::PublicRead => true,
            BucketAccess::Private => is_admin(roles) || is_owner(user_id, object),
        }
    }

    /// Check if the user can write (upload) to the bucket.
    ///
    /// Rules:
    /// - Must be authenticated (`user_id` present)
    /// - Admin role always allowed
    #[must_use]
    pub fn can_write(
        &self,
        user_id: Option<&str>,
        roles: &[String],
        bucket: &BucketConfig,
    ) -> bool {
        self.can_write_key(user_id, roles, bucket, "")
    }

    /// [`can_write`](Self::can_write) for a known key, so a policy's
    /// `key_prefix` can narrow the grant. The key-less form is the whole-bucket
    /// question (a prefix-scoped rule does not answer it).
    #[must_use]
    pub fn can_write_key(
        &self,
        user_id: Option<&str>,
        roles: &[String],
        bucket: &BucketConfig,
        key: &str,
    ) -> bool {
        if let Some(decision) =
            policy_decision(PolicyMethod::Write, user_id, roles, bucket, key, None)
        {
            return decision;
        }
        if is_admin(roles) {
            return true;
        }
        user_id.is_some()
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
        user_id: Option<&str>,
        roles: &[String],
        bucket: &BucketConfig,
        existing: Option<&StorageMetadataRow>,
    ) -> bool {
        match existing {
            None => self.can_write(user_id, roles, bucket),
            Some(object) => {
                // #371: overwriting an EXISTING object is `overwrite`, never
                // `write`. Otherwise the natural rule "authenticated callers
                // may write" re-opens the H9/B4 overwrite IDOR by permitting
                // any authenticated caller to clobber another user's object.
                if let Some(decision) = policy_decision(
                    PolicyMethod::Overwrite,
                    user_id,
                    roles,
                    bucket,
                    &object.key,
                    object.owner_id.as_deref(),
                ) {
                    return decision;
                }
                is_admin(roles) || is_owner(user_id, object)
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
        user_id: Option<&str>,
        roles: &[String],
        bucket: &BucketConfig,
        object: &StorageMetadataRow,
    ) -> bool {
        if let Some(decision) = policy_decision(
            PolicyMethod::Delete,
            user_id,
            roles,
            bucket,
            &object.key,
            object.owner_id.as_deref(),
        ) {
            return decision;
        }
        is_admin(roles) || is_owner(user_id, object)
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
        user_id: Option<&str>,
        roles: &[String],
        bucket: &BucketConfig,
        prefix: &str,
    ) -> bool {
        if let Some(decision) =
            policy_decision(PolicyMethod::List, user_id, roles, bucket, prefix, None)
        {
            return decision;
        }
        if is_admin(roles) || user_id.is_some() {
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
        user_id: Option<&str>,
        roles: &[String],
        bucket: &BucketConfig,
        objects: Vec<StorageMetadataRow>,
    ) -> Vec<StorageMetadataRow> {
        if bucket.policies.is_some() {
            if is_admin(roles) {
                return objects;
            }
            // Per-object, because a `key_prefix` rule makes visibility a
            // property of the key, not of the bucket.
            return objects
                .into_iter()
                .filter(|object| {
                    policy_decision(
                        PolicyMethod::Read,
                        user_id,
                        roles,
                        bucket,
                        &object.key,
                        object.owner_id.as_deref(),
                    )
                    .unwrap_or(false)
                })
                .collect();
        }
        match bucket.access {
            BucketAccess::PublicRead => objects,
            BucketAccess::Private => {
                if is_admin(roles) {
                    return objects;
                }
                objects.into_iter().filter(|obj| is_owner(user_id, obj)).collect()
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
    user_id: Option<&str>,
    roles: &[String],
    bucket: &BucketConfig,
    key: &str,
    owner_id: Option<&str>,
) -> Option<bool> {
    let policy = bucket.policies.as_ref()?;
    if is_admin(roles) {
        return Some(true);
    }
    Some(policy.permits(
        method,
        &PolicyRequest {
            user_id,
            roles,
            key,
            owner_id,
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
