# Extracted from: docs/enterprise/RBAC_POSTGRESQL_REFACTORED.md
# Block number: 2
# tests/integration/enterprise/rbac/test_cache_setup.py


async def test_rbac_cache_domains_registered():
    """Verify RBAC cache domains are registered with triggers."""
    from fraiseql.caching import get_cache

    cache = get_cache()

    # Check if pg_fraiseql_cache extension is available
    if not cache.has_domain_versioning:
        pytest.skip("pg_fraiseql_cache extension not installed")

    # Verify domains exist
    async with db.pool.connection() as conn, conn.cursor() as cur:
        await cur.execute("""
            SELECT domain
            FROM fraiseql_cache.domain_version
            WHERE domain IN ('role', 'permission', 'role_permission', 'user_role')
        """)
        domains = {row[0] for row in await cur.fetchall()}

    assert "role" in domains
    assert "permission" in domains
    assert "role_permission" in domains
    assert "user_role" in domains
    # Expected failure: domains not registered
