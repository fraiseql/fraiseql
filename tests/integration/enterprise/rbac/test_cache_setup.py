from pathlib import Path

import pytest

pytestmark = pytest.mark.integration


@pytest.mark.asyncio
async def test_rbac_cache_setup_migration_exists() -> None:
    """Verify RBAC cache setup migration file exists and has correct content."""
    cache_migration_path = Path("src/fraiseql/enterprise/migrations/003_rbac_cache_setup.sql")

    assert cache_migration_path.exists(), "RBAC cache setup migration should exist"

    # Verify the migration contains expected content
    migration_content = cache_migration_path.read_text()

    # Should contain domain setup calls
    assert "setup_table_invalidation" in migration_content
    assert "fraiseql_cache.setup_table_invalidation" in migration_content

    # Should contain CASCADE rules
    assert "cascade_rules" in migration_content
    assert "INSERT INTO fraiseql_cache.cascade_rules" in migration_content

    # Should reference RBAC domains
    assert "'role'" in migration_content
    assert "'permission'" in migration_content
    assert "'role_permission'" in migration_content
    assert "'user_role'" in migration_content

    # Should reference user_permissions as target domain
    assert "'user_permissions'" in migration_content
