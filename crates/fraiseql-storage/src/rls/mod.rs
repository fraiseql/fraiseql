//! Row-level security enforcement for storage operations.
//!
//! Evaluates access control policies against bucket configuration and
//! object ownership. Follows the "RLS always wins" principle — deny-by-default.

#[cfg(test)]
mod tests;

use crate::{
    config::{BucketAccess, BucketConfig},
    metadata::StorageMetadataRow,
};

/// The admin role name that bypasses all access checks.
const ADMIN_ROLE: &str = "admin";

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
        _bucket: &BucketConfig,
    ) -> bool {
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
            Some(object) => is_admin(roles) || is_owner(user_id, object),
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
        _bucket: &BucketConfig,
        object: &StorageMetadataRow,
    ) -> bool {
        is_admin(roles) || is_owner(user_id, object)
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

/// Check if the roles contain the admin role.
fn is_admin(roles: &[String]) -> bool {
    roles.iter().any(|r| r == ADMIN_ROLE)
}

/// Check if the user owns the object.
fn is_owner(user_id: Option<&str>, object: &StorageMetadataRow) -> bool {
    match (user_id, &object.owner_id) {
        (Some(uid), Some(owner)) => uid == owner,
        _ => false,
    }
}
