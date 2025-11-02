# Extracted from: docs/enterprise/RBAC_POSTGRESQL_REFACTORED.md
# Block number: 7
# tests/integration/enterprise/rbac/test_cache_invalidation.py


async def test_permission_cache_invalidates_on_role_change():
    """Verify cache invalidates when user roles change."""
    from fraiseql.enterprise.rbac.cache import PermissionCache
    from fraiseql.enterprise.rbac.resolver import PermissionResolver

    cache = PermissionCache(db_pool)
    resolver = PermissionResolver(db_repo, cache)

    user_id = uuid4()
    tenant_id = uuid4()

    # Get initial permissions (should cache)
    permissions1 = await resolver.get_user_permissions(user_id, tenant_id)
    initial_count = len(permissions1)

    # Assign new role to user
    await db.execute(
        """
        INSERT INTO user_roles (user_id, role_id, tenant_id)
        VALUES (%s, %s, %s)
    """,
        (user_id, "some-new-role-id", tenant_id),
    )

    # Get permissions again (should recompute due to invalidation)
    permissions2 = await resolver.get_user_permissions(user_id, tenant_id)

    # Should have different permissions now
    assert len(permissions2) != initial_count
    # Expected failure: cache not invalidating
