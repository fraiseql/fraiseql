# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 38
# tests/integration/enterprise/rbac/test_management_api.py


async def test_create_role_mutation():
    """Verify role creation via GraphQL."""
    result = await execute_graphql(
        """
        mutation {
            createRole(input: {
                name: "data_scientist"
                description: "Data science team member"
                parentRoleId: "developer-role-id"
                permissionIds: ["perm-1", "perm-2"]
            }) {
                role {
                    id
                    name
                    permissions { resource action }
                }
            }
        }
    """,
        context={"user_id": "admin-user", "tenant_id": "tenant-1"},
    )

    assert result["data"]["createRole"]["role"]["name"] == "data_scientist"
    assert len(result["data"]["createRole"]["role"]["permissions"]) == 2
    # Expected failure: createRole mutation not implemented
