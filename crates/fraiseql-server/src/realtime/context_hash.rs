//! Security context hashing for RLS group coalescing.
//!
//! Clients with identical security contexts (same user, roles, tenant, and
//! scopes) share a single RLS evaluation per event rather than one per
//! connection. This module provides the hash function that groups them.
//!
//! # In-memory only
//!
//! Hashes produced by [`security_context_hash`] are **never persisted or
//! compared across processes**. `ahash` is not stable across versions or
//! platforms, which is acceptable here. If hashes ever need to be persisted,
//! switch to a stable algorithm (SipHash-2-4 or BLAKE3).

use std::hash::{Hash, Hasher};

use ahash::AHasher;

/// Borrowed view of the identity fields used for context hashing.
///
/// All fields that determine *who* a user is from an RLS perspective
/// are included. Fields that are per-request metadata (`request_id`,
/// `ip_address`) are intentionally excluded so that two requests from
/// the same principal share the same hash.
pub struct SecurityContextHashInput<'a> {
    /// User identifier (from JWT `sub` claim).
    pub user_id: &'a str,
    /// User's roles.
    pub roles: &'a [&'a str],
    /// Tenant/organisation identifier.
    pub tenant_id: Option<&'a str>,
    /// OAuth/permission scopes.
    pub scopes: &'a [&'a str],
}

/// Compute a stable in-memory hash for a security context.
///
/// The hash is order-independent for roles and scopes: two inputs with the
/// same elements in a different order produce the same hash.
///
/// # In-memory only
///
/// This hash is suitable for runtime grouping only. Do **not** persist it
/// or compare values across process restarts.
#[must_use]
pub fn security_context_hash(ctx: &SecurityContextHashInput<'_>) -> u64 {
    let mut hasher = AHasher::default();

    ctx.user_id.hash(&mut hasher);

    // Sort roles so hash is order-independent, then feed each element to the hasher.
    let mut roles: Vec<&str> = ctx.roles.to_vec();
    roles.sort_unstable();
    for role in &roles {
        role.hash(&mut hasher);
    }

    ctx.tenant_id.hash(&mut hasher);

    // Sort scopes so hash is order-independent, then feed each element to the hasher.
    let mut scopes: Vec<&str> = ctx.scopes.to_vec();
    scopes.sort_unstable();
    for scope in &scopes {
        scope.hash(&mut hasher);
    }

    hasher.finish()
}
