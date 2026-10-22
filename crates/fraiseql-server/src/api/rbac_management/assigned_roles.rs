//! The roles an end user holds through `fraiseql_user_roles` (#1632).
//!
//! The RBAC management API and SCIM group sync both write role assignments. This is
//! their one reader on the request path: every transport adds these roles to the claim
//! roles of a human principal before dispatch (`identity::resolve_request_identity`).
//!
//! An assignment is keyed by `user_id`, the subject (`sub`) of the principal's token,
//! whichever trusted issuer signed it. A global assignment (no tenant) applies in every
//! tenant; a tenant's assignment only to a principal in that tenant.
//!
//! Lookups are cached per `(sub, tenant)` for
//! [`AssignedRoles::TTL`](crate::api::rbac_management::assigned_roles::AssignedRoles::TTL).
//! Every write through
//! [`RbacDbBackend`](crate::api::rbac_management::db_backend::RbacDbBackend) flushes the cache it
//! shares with this reader, so a change applies on the next request of the replica that
//! made it. Other replicas converge within the TTL.

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use dashmap::DashMap;
use sqlx::PgPool;

use super::db_backend::RbacDbError;

/// One cached lookup.
struct Entry {
    roles:      Vec<String>,
    expires_at: Instant,
}

/// Reads the roles assigned to a subject, through a per-subject TTL cache.
pub struct AssignedRoles {
    pool:       PgPool,
    /// Keyed by `(sub, tenant)`.
    cache:      DashMap<(String, Option<String>), Entry>,
    /// Bumped by every flush. A lookup that started before a flush does not cache its
    /// answer, which may predate the change the flush was for.
    generation: AtomicU64,
}

impl AssignedRoles {
    /// How long a lookup is served from the cache: the bound on how stale a replica
    /// that did not make a change can be.
    pub const TTL: Duration = Duration::from_secs(30);

    /// A reader over `pool`, the database the RBAC tables live in.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            cache: DashMap::new(),
            generation: AtomicU64::new(0),
        }
    }

    /// The names of the roles assigned to `sub`, globally or in `tenant`.
    ///
    /// # Errors
    ///
    /// Returns `RbacDbError::QueryError` if the lookup fails. A failure is not cached.
    pub async fn of(&self, sub: &str, tenant: Option<&str>) -> Result<Vec<String>, RbacDbError> {
        let key = (sub.to_owned(), tenant.map(str::to_owned));
        if let Some(entry) = self.cache.get(&key) {
            if Instant::now() < entry.expires_at {
                return Ok(entry.roles.clone());
            }
        }
        let generation = self.generation.load(Ordering::Acquire);
        // `tenant_id` is a UUID column; comparing its text form keeps a tenant claim that
        // is not a UUID from failing the request: such a principal holds global roles only.
        let roles: Vec<String> = sqlx::query_scalar(
            "SELECT r.name FROM fraiseql_user_roles ur
             JOIN fraiseql_roles r ON r.id = ur.role_id
             WHERE ur.user_id = $1 AND (ur.tenant_id IS NULL OR ur.tenant_id::text = lower($2))",
        )
        .bind(sub)
        .bind(tenant)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RbacDbError::QueryError(e.to_string()))?;
        if self.generation.load(Ordering::Acquire) == generation {
            self.cache.insert(
                key.clone(),
                Entry {
                    roles:      roles.clone(),
                    expires_at: Instant::now() + Self::TTL,
                },
            );
            // A flush that landed between the check and the insert has already swept, so
            // take the entry back out ourselves.
            if self.generation.load(Ordering::Acquire) != generation {
                self.cache.remove(&key);
            }
        }
        Ok(roles)
    }

    /// Forget every cached lookup for `sub`, after an assignment to it changes.
    pub fn flush(&self, sub: &str) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.cache.retain(|(cached, _), _| cached != sub);
    }

    /// Forget every cached lookup, after a role itself changes (renamed or deleted).
    pub fn flush_all(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.cache.clear();
    }
}
