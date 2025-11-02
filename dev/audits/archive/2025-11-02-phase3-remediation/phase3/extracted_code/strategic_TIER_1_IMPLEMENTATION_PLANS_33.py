# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 33
# tests/integration/enterprise/rbac/test_directives.py


async def test_requires_permission_directive():
    """Verify @requires_permission blocks unauthorized access."""
    # User with 'viewer' role (only has read permissions)
    result = await execute_graphql(
        """
        mutation {
            deleteUser(id: "user-123") {
                success
            }
        }
    """,
        context={"user_id": "viewer-user", "tenant_id": "tenant-1"},
    )

    # Should be blocked - viewer doesn't have 'user.delete' permission
    assert result["errors"] is not None
    assert "permission denied" in result["errors"][0]["message"].lower()
    # Expected failure: directive not implemented
