//! Multi-layer permission cache with TTL expiry and LRU eviction.

use lru::LruCache;
use std::sync::Mutex;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};
use uuid::Uuid;
use super::{errors::Result, models::Permission};

/// Permission cache with TTL expiry and LRU eviction
pub struct PermissionCache {
    cache: Mutex<LruCache<CacheKey, CacheEntry>>,
    default_ttl: Duration,
}

#[derive(Hash, Eq, PartialEq, Clone)]
struct CacheKey {
    user_id: Uuid,
    tenant_id: Option<Uuid>,
}

#[derive(Clone)]
struct CacheEntry {
    permissions: Vec<Permission>,
    expires_at: Instant,
}

impl PermissionCache {
    /// Create new cache with capacity and default TTL
    pub fn new(capacity: usize) -> Self {
        Self::with_ttl(capacity, Duration::from_secs(300)) // 5 minute default TTL
    }

    /// Create new cache with custom TTL
    pub fn with_ttl(capacity: usize, default_ttl: Duration) -> Self {
        Self {
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(capacity).unwrap())),
            default_ttl,
        }
    }

    /// Get cached permissions (with TTL check)
    pub fn get(&self, user_id: Uuid, tenant_id: Option<Uuid>) -> Option<Vec<Permission>> {
        let key = CacheKey { user_id, tenant_id };
        let mut cache = self.cache.lock().unwrap();

        if let Some(entry) = cache.get(&key) {
            if Instant::now() < entry.expires_at {
                return Some(entry.permissions.clone());
            } else {
                // Entry expired, remove it
                cache.pop(&key);
            }
        }
        None
    }

    /// Cache permissions with default TTL
    pub fn set(&self, user_id: Uuid, tenant_id: Option<Uuid>, permissions: Vec<Permission>) {
        self.set_with_ttl(user_id, tenant_id, permissions, self.default_ttl);
    }

    /// Cache permissions with custom TTL
    pub fn set_with_ttl(
        &self,
        user_id: Uuid,
        tenant_id: Option<Uuid>,
        permissions: Vec<Permission>,
        ttl: Duration,
    ) {
        let key = CacheKey { user_id, tenant_id };
        let entry = CacheEntry {
            permissions,
            expires_at: Instant::now() + ttl,
        };

        let mut cache = self.cache.lock().unwrap();
        cache.put(key, entry);
    }

    /// Invalidate specific user (all tenants)
    pub fn invalidate_user(&self, user_id: Uuid) {
        let mut cache = self.cache.lock().unwrap();

        let keys_to_remove: Vec<CacheKey> = cache
            .iter()
            .filter(|(k, _)| k.user_id == user_id)
            .map(|(k, _)| k.clone())
            .collect();

        for key in keys_to_remove {
            cache.pop(&key);
        }
    }

    /// Invalidate specific tenant (all users)
    pub fn invalidate_tenant(&self, tenant_id: Uuid) {
        let mut cache = self.cache.lock().unwrap();

        let keys_to_remove: Vec<CacheKey> = cache
            .iter()
            .filter(|(k, _)| k.tenant_id == Some(tenant_id))
            .map(|(k, _)| k.clone())
            .collect();

        for key in keys_to_remove {
            cache.pop(&key);
        }
    }

    /// Invalidate specific role (affects all users with this role)
    pub fn invalidate_role(&self, _role_id: Uuid) {
        // Since we don't store role info in cache keys, we need to clear
        // potentially affected entries. For now, clear entire cache.
        // Phase 12 could optimize this with reverse index.
        self.clear();
    }

    /// Invalidate specific permission (affects all users with this permission)
    pub fn invalidate_permission(&self, _permission_id: Uuid) {
        // Similar to role invalidation - clear entire cache for safety
        // Phase 12 could optimize with permission-based invalidation
        self.clear();
    }

    /// Clear entire cache
    pub fn clear(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
    }

    /// Clean expired entries (maintenance operation)
    pub fn cleanup_expired(&self) {
        let mut cache = self.cache.lock().unwrap();
        let now = Instant::now();

        // Remove expired entries
        let keys_to_remove: Vec<CacheKey> = cache
            .iter()
            .filter(|(_, entry)| now >= entry.expires_at)
            .map(|(k, _)| k.clone())
            .collect();

        for key in keys_to_remove {
            cache.pop(&key);
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let cache = self.cache.lock().unwrap();
        let now = Instant::now();

        let expired_count = cache
            .iter()
            .filter(|(_, entry)| now >= entry.expires_at)
            .count();

        CacheStats {
            capacity: cache.cap().get(),
            size: cache.len(),
            expired_count,
        }
    }
}

#[derive(Debug)]
pub struct CacheStats {
    pub capacity: usize,
    pub size: usize,
    pub expired_count: usize,
}

/// Cache invalidation strategies for RBAC changes
pub struct CacheInvalidation;

impl CacheInvalidation {
    /// Invalidate cache when user role is assigned/revoked
    pub fn on_user_role_change(cache: &PermissionCache, user_id: Uuid) {
        cache.invalidate_user(user_id);
    }

    /// Invalidate cache when role permissions change
    pub fn on_role_permission_change(cache: &PermissionCache, role_id: Uuid) {
        cache.invalidate_role(role_id);
    }

    /// Invalidate cache when user is deleted
    pub fn on_user_deleted(cache: &PermissionCache, user_id: Uuid) {
        cache.invalidate_user(user_id);
    }

    /// Invalidate cache when tenant is deleted
    pub fn on_tenant_deleted(cache: &PermissionCache, tenant_id: Uuid) {
        cache.invalidate_tenant(tenant_id);
    }

    /// Invalidate entire cache (for major RBAC changes)
    pub fn on_major_rbac_change(cache: &PermissionCache) {
        cache.clear();
    }
}
