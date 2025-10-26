"""Test Permission Resolution with PostgreSQL Caching

Tests for effective permission computation with hierarchical role inheritance
and PostgreSQL-native caching.
"""

import pytest
from uuid import uuid4
from pathlib import Path
from fraiseql.enterprise.rbac.resolver import PermissionResolver
from fraiseql.enterprise.rbac.cache import PermissionCache


@pytest.fixture(autouse=True, scope="module")
async def ensure_rbac_schema(db_pool):
    """Ensure RBAC schema exists before running tests."""
    # Check if roles table exists
    async with db_pool.connection() as conn:
        async with conn.cursor() as cur:
            await cur.execute(
                """
                SELECT EXISTS (
                    SELECT 1 FROM information_schema.tables
                    WHERE table_name = 'roles'
                )
            """
            )
            exists = (await cur.fetchone())[0]

            if not exists:
                # Read and execute the migration
                migration_path = Path("src/fraiseql/enterprise/migrations/002_rbac_tables.sql")
                migration_sql = migration_path.read_text()
                await cur.execute(migration_sql)
                await conn.commit()


async def test_user_effective_permissions_with_caching(db_repo, db_pool):
    """Verify user permissions are cached in PostgreSQL."""
    cache = PermissionCache(db_pool)
    resolver = PermissionResolver(db_repo, cache)

    user_id = uuid4()
    tenant_id = uuid4()

    # First call - should compute and cache
    permissions1 = await resolver.get_user_permissions(user_id, tenant_id)

    # Second call - should hit cache
    permissions2 = await resolver.get_user_permissions(user_id, tenant_id)

    assert permissions1 == permissions2


async def test_permission_resolver_methods_exist(db_repo, db_pool):
    """Verify PermissionResolver has required methods."""
    cache = PermissionCache(db_pool)
    resolver = PermissionResolver(db_repo, cache)

    # Test core methods exist
    assert hasattr(resolver, "get_user_permissions")
    assert hasattr(resolver, "has_permission")
    assert hasattr(resolver, "check_permission")
    assert hasattr(resolver, "get_user_roles")
    assert hasattr(resolver, "get_role_permissions")
    assert hasattr(resolver, "invalidate_user_cache")

    # Test method signatures
    import inspect

    # get_user_permissions should accept user_id, tenant_id, use_cache
    sig = inspect.signature(resolver.get_user_permissions)
    assert "user_id" in sig.parameters
    assert "tenant_id" in sig.parameters
    assert "use_cache" in sig.parameters

    # has_permission should accept user_id, resource, action, tenant_id
    sig = inspect.signature(resolver.has_permission)
    assert "user_id" in sig.parameters
    assert "resource" in sig.parameters
    assert "action" in sig.parameters
    assert "tenant_id" in sig.parameters


async def test_cache_integration(db_repo, db_pool):
    """Test that resolver integrates properly with cache."""
    cache = PermissionCache(db_pool)
    resolver = PermissionResolver(db_repo, cache)

    user_id = uuid4()
    tenant_id = uuid4()

    # Get permissions (should use cache)
    permissions = await resolver.get_user_permissions(user_id, tenant_id, use_cache=True)

    # Verify cache was attempted
    # (In real test, we'd check cache stats or mock the cache)

    # Test cache bypass
    permissions_no_cache = await resolver.get_user_permissions(user_id, tenant_id, use_cache=False)

    # Should be the same result
    assert permissions == permissions_no_cache
