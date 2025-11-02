# Extracted from: docs/enterprise/RBAC_POSTGRESQL_REFACTORED.md
# Block number: 4
# tests/integration/enterprise/rbac/test_permission_cache.py


async def test_permission_cache_stores_and_retrieves():
    """Verify permissions can be cached and retrieved from PostgreSQL."""
    from fraiseql.enterprise.rbac.cache import PermissionCache
    from fraiseql.enterprise.rbac.models import Permission

    cache = PermissionCache(db_pool)

    # Mock permissions
    permissions = [
        Permission(id=uuid4(), resource="user", action="read", description="Read users"),
        Permission(id=uuid4(), resource="user", action="write", description="Write users"),
    ]

    user_id = uuid4()
    tenant_id = uuid4()

    # Store in cache
    await cache.set(user_id, tenant_id, permissions)

    # Retrieve from cache
    cached = await cache.get(user_id, tenant_id)

    assert cached is not None
    assert len(cached) == 2
    assert cached[0].resource == "user"
    # Expected failure: PermissionCache not implemented
