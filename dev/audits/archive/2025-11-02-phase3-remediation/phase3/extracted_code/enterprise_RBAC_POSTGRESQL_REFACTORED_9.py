# Extracted from: docs/enterprise/RBAC_POSTGRESQL_REFACTORED.md
# Block number: 9
async def test_cascade_invalidation_on_role_permission_change():
    """Verify CASCADE rule invalidates user permissions when role permissions change."""
    from fraiseql.enterprise.rbac.cache import PermissionCache
    from fraiseql.enterprise.rbac.resolver import PermissionResolver

    if not (await get_cache()).has_domain_versioning:
        pytest.skip("Requires pg_fraiseql_cache extension")

    cache = PermissionCache(db_pool)
    resolver = PermissionResolver(db_repo, cache)

    user_id = uuid4()
    role_id = uuid4()
    permission_id = uuid4()
    tenant_id = uuid4()

    # Setup: user has role
    await db.execute(
        """
        INSERT INTO user_roles (user_id, role_id, tenant_id)
        VALUES (%s, %s, %s)
    """,
        (user_id, role_id, tenant_id),
    )

    # Get initial permissions (caches result)
    permissions1 = await resolver.get_user_permissions(user_id, tenant_id)

    # Add permission to role
    await db.execute(
        """
        INSERT INTO role_permissions (role_id, permission_id)
        VALUES (%s, %s)
    """,
        (role_id, permission_id),
    )

    # Domain version increments:
    # 1. role_permissions INSERT → role_permission domain version++
    # 2. CASCADE rule → user_permissions domain version++

    # Get permissions again
    permissions2 = await resolver.get_user_permissions(user_id, tenant_id)

    # Should include new permission
    assert len(permissions2) > len(permissions1)
