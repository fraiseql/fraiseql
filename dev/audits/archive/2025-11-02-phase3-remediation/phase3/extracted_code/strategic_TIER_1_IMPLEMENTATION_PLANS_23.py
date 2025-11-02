# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 23
# tests/integration/enterprise/rbac/test_rbac_schema.py


async def test_rbac_tables_exist():
    """Verify RBAC tables exist with correct schema."""
    tables = ["roles", "permissions", "role_permissions", "user_roles"]

    for table in tables:
        result = await db.run(
            DatabaseQuery(
                statement=f"""
                SELECT column_name, data_type
                FROM information_schema.columns
                WHERE table_name = '{table}'
            """,
                params={},
                fetch_result=True,
            )
        )
        assert len(result) > 0, f"Table {table} should exist"

    # Verify roles table structure
    roles_columns = await get_table_columns("roles")
    assert "id" in roles_columns
    assert "name" in roles_columns
    assert "parent_role_id" in roles_columns  # For hierarchy
    assert "tenant_id" in roles_columns  # Multi-tenancy
    # Expected failure: tables don't exist
