# Extracted from: docs/enterprise/RBAC_POSTGRESQL_REFACTORED.md
# Block number: 8
# No code changes needed - domain versioning handles this automatically
# This test validates that the cache setup in Phase 1.2 is working

# However, add helper to manually trigger invalidation for testing
async def test_manual_invalidation():
    """Verify manual invalidation works."""
    from fraiseql.enterprise.rbac.cache import PermissionCache

    cache = PermissionCache(db_pool)
    user_id = uuid4()
    tenant_id = uuid4()

    # Cache some permissions
    await cache.set(user_id, tenant_id, [mock_permission()])

    # Verify cached
    assert await cache.get(user_id, tenant_id) is not None

    # Manually invalidate
    await cache.invalidate_user(user_id, tenant_id)

    # Verify invalidated
    assert await cache.get(user_id, tenant_id) is None
