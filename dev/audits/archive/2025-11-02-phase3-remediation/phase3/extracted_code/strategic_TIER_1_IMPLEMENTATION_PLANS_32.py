# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 32
# src/fraiseql/enterprise/rbac/cache.py

import hashlib
import json
from datetime import timedelta
from typing import List, Optional
from uuid import UUID

from fraiseql.enterprise.rbac.models import Permission


class PermissionCache:
    """2-layer permission cache (request-level + Redis)."""

    def __init__(self, redis_client=None):
        self.redis = redis_client
        self._request_cache: dict[str, List[Permission]] = {}
        self._cache_ttl = timedelta(minutes=5)

    def _make_key(self, user_id: UUID, tenant_id: Optional[UUID]) -> str:
        """Generate cache key for user permissions."""
        data = f"{user_id}:{tenant_id or 'global'}"
        return f"rbac:permissions:{hashlib.md5(data.encode()).hexdigest()}"

    async def get(self, user_id: UUID, tenant_id: Optional[UUID]) -> Optional[List[Permission]]:
        """Get cached permissions."""
        key = self._make_key(user_id, tenant_id)

        # Try request-level cache first (fastest)
        if key in self._request_cache:
            return self._request_cache[key]

        # Try Redis cache
        if self.redis:
            cached_data = await self.redis.get(key)
            if cached_data:
                permissions = [Permission(**p) for p in json.loads(cached_data)]
                self._request_cache[key] = permissions
                return permissions

        return None

    async def set(self, user_id: UUID, tenant_id: Optional[UUID], permissions: List[Permission]):
        """Cache permissions."""
        key = self._make_key(user_id, tenant_id)

        # Store in request cache
        self._request_cache[key] = permissions

        # Store in Redis
        if self.redis:
            data = json.dumps(
                [
                    {
                        "id": str(p.id),
                        "resource": p.resource,
                        "action": p.action,
                        "constraints": p.constraints,
                    }
                    for p in permissions
                ]
            )
            await self.redis.setex(key, self._cache_ttl.total_seconds(), data)

    def clear_request_cache(self):
        """Clear request-level cache (called at end of request)."""
        self._request_cache.clear()

    async def invalidate_user(self, user_id: UUID, tenant_id: Optional[UUID] = None):
        """Invalidate cache for user (e.g., after role change)."""
        key = self._make_key(user_id, tenant_id)
        self._request_cache.pop(key, None)
        if self.redis:
            await self.redis.delete(key)


# Update PermissionResolver to use cache
class PermissionResolver:
    """Permission resolver with caching."""

    def __init__(self, repo: FraiseQLRepository, cache: PermissionCache = None):
        self.repo = repo
        self.hierarchy = RoleHierarchy(repo)
        self.cache = cache or PermissionCache()

    async def get_user_permissions(
        self, user_id: UUID, tenant_id: Optional[UUID] = None, use_cache: bool = True
    ) -> List[Permission]:
        """Get user permissions with caching."""
        if use_cache:
            cached = await self.cache.get(user_id, tenant_id)
            if cached is not None:
                return cached

        # Compute permissions (same as before)
        permissions = await self._compute_permissions(user_id, tenant_id)

        if use_cache:
            await self.cache.set(user_id, tenant_id, permissions)

        return permissions
