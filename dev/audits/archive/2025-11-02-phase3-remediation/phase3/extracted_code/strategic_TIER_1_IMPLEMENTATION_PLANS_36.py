# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 36
# tests/integration/enterprise/rbac/test_row_level_security.py


async def test_tenant_scoped_rls():
    """Verify users can only see data from their tenant."""
    # Create data in multiple tenants
    await create_test_data(tenant_id="tenant-1", user_id="user-1")
    await create_test_data(tenant_id="tenant-2", user_id="user-2")

    # Query as tenant-1 user
    result = await execute_graphql(
        """
        query {
            users {
                id
                tenantId
            }
        }
    """,
        context={"user_id": "user-1", "tenant_id": "tenant-1"},
    )

    users = result["data"]["users"]
    # Should only see tenant-1 data
    assert all(u["tenantId"] == "tenant-1" for u in users)
    # Expected failure: RLS not configured
