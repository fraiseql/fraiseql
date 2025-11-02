# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 40
@strawberry.mutation
@requires_permission(resource="role", action="delete")
async def delete_role(self, info: Info, role_id: UUID) -> DeleteRoleResponse:
    """Delete a role (if not system role)."""
    repo = info.context["repo"]

    # Check if system role
    role = await repo.run(
        DatabaseQuery(
            statement="SELECT is_system FROM roles WHERE id = %s",
            params={"role_id": str(role_id)},
            fetch_result=True,
        )
    )

    if not role:
        raise ValueError(f"Role {role_id} not found")

    if role[0]["is_system"]:
        raise PermissionError("Cannot delete system role")

    # Delete role (CASCADE will remove user_roles and role_permissions)
    await repo.run(
        DatabaseQuery(
            statement="DELETE FROM roles WHERE id = %s",
            params={"role_id": str(role_id)},
            fetch_result=False,
        )
    )

    return DeleteRoleResponse(success=True)


@strawberry.mutation
@requires_permission(resource="role", action="update")
async def add_permission_to_role(
    self, info: Info, role_id: UUID, permission_id: UUID
) -> AddPermissionResponse:
    """Add permission to role."""
    repo = info.context["repo"]

    await repo.run(
        DatabaseQuery(
            statement="""
            INSERT INTO role_permissions (role_id, permission_id, granted)
            VALUES (%s, %s, TRUE)
            ON CONFLICT (role_id, permission_id) DO UPDATE SET granted = TRUE
        """,
            params={"role_id": str(role_id), "permission_id": str(permission_id)},
            fetch_result=False,
        )
    )

    # Clear hierarchy cache (permissions changed)
    hierarchy = info.context.get("role_hierarchy")
    if hierarchy:
        hierarchy.clear_cache(role_id)

    return AddPermissionResponse(success=True)
