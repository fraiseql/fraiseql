# Extracted from: docs/enterprise/RBAC_POSTGRESQL_REFACTORED.md
# Block number: 12
# tests/integration/enterprise/rbac/test_permission_resolution.py


async def test_user_effective_permissions_with_caching():
    """Verify user permissions are cached in PostgreSQL."""
    from fraiseql.enterprise.rbac.cache import PermissionCache
    from fraiseql.enterprise.rbac.resolver import PermissionResolver

    cache = PermissionCache(db_pool)
    resolver = PermissionResolver(db_repo, cache)

    user_id = uuid4()
    tenant_id = uuid4()

    # First call - should compute and cache
    permissions1 = await resolver.get_user_permissions(user_id, tenant_id)

    # Second call - should hit cache
    permissions2 = await resolver.get_user_permissions(user_id, tenant_id)

    assert permissions1 == permissions2
    # Expected failure: not using cache
