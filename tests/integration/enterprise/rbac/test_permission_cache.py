from datetime import datetime
from unittest.mock import AsyncMock, MagicMock, patch
from uuid import uuid4

import pytest

from fraiseql.enterprise.rbac.models import Permission

pytestmark = pytest.mark.enterprise


@pytest.mark.asyncio
async def test_permission_cache_stores_and_retrieves() -> None:
    """Verify permissions can be cached and retrieved from PostgreSQL."""
    from fraiseql.enterprise.rbac.cache import PermissionCache

    # Mock database pool
    db_pool = MagicMock()

    # Mock PostgresCache
    with patch("fraiseql.enterprise.rbac.cache.PostgresCache") as mock_pg_cache_class:
        mock_pg_cache = MagicMock()
        mock_pg_cache.has_domain_versioning = False  # Disable for this test
        mock_pg_cache.get_with_metadata = AsyncMock(return_value=(None, None))  # Cache miss
        mock_pg_cache.set = AsyncMock()
        mock_pg_cache_class.return_value = mock_pg_cache

        cache = PermissionCache(db_pool)

        # Mock permissions
        now = datetime.now()
        permissions = [
            Permission(
                id=uuid4(), resource="user", action="read", description="Read users", created_at=now
            ),
            Permission(
                id=uuid4(),
                resource="user",
                action="write",
                description="Write users",
                created_at=now,
            ),
        ]

        user_id = uuid4()
        tenant_id = uuid4()

        # Store in cache
        await cache.set(user_id, tenant_id, permissions)

        # Verify set was called on PostgresCache
        mock_pg_cache.set.assert_called_once()

        # Mock cache hit for retrieval
        serialized_permissions = [
            {
                "id": str(p.id),
                "resource": p.resource,
                "action": p.action,
                "description": p.description,
                "constraints": p.constraints,
            }
            for p in permissions
        ]
        mock_pg_cache.get_with_metadata = AsyncMock(return_value=(serialized_permissions, None))

        # Retrieve from cache
        cached = await cache.get(user_id, tenant_id)

        assert cached is not None
        assert len(cached) == 2
        assert cached[0].resource == "user"
        assert cached[0].action == "read"
        assert cached[1].resource == "user"
        assert cached[1].action == "write"
