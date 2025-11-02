# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 39
# src/fraiseql/enterprise/rbac/types.py (continued)


@strawberry.type
class RBACMutation:
    """GraphQL mutations for RBAC management."""

    @strawberry.mutation
    @requires_permission(resource="role", action="create")
    async def create_role(self, info: Info, input: CreateRoleInput) -> CreateRoleResponse:
        """Create a new role."""
        repo = info.context["repo"]
        tenant_id = info.context.get("tenant_id")
        user_id = info.context["user_id"]

        # Create role
        role_id = uuid4()
        await repo.run(
            DatabaseQuery(
                statement="""
                INSERT INTO roles (id, name, description, parent_role_id, tenant_id)
                VALUES (%s, %s, %s, %s, %s)
            """,
                params={
                    "id": role_id,
                    "name": input.name,
                    "description": input.description,
                    "parent_role_id": str(input.parent_role_id) if input.parent_role_id else None,
                    "tenant_id": str(tenant_id) if tenant_id else None,
                },
                fetch_result=False,
            )
        )

        # Assign permissions to role
        if input.permission_ids:
            for perm_id in input.permission_ids:
                await repo.run(
                    DatabaseQuery(
                        statement="""
                        INSERT INTO role_permissions (role_id, permission_id)
                        VALUES (%s, %s)
                    """,
                        params={"role_id": role_id, "permission_id": str(perm_id)},
                        fetch_result=False,
                    )
                )

        # Log to audit trail
        audit_logger = info.context.get("audit_logger")
        if audit_logger:
            await audit_logger.log_event(
                event_type="rbac.role.created",
                event_data={"role_id": str(role_id), "name": input.name},
                user_id=str(user_id),
                tenant_id=str(tenant_id) if tenant_id else None,
            )

        # Fetch created role
        role = await repo.run(
            DatabaseQuery(
                statement="SELECT * FROM roles WHERE id = %s",
                params={"id": role_id},
                fetch_result=True,
            )
        )

        return CreateRoleResponse(role=Role(**role[0]))

    @strawberry.mutation
    @requires_permission(resource="role", action="assign")
    async def assign_role_to_user(
        self, info: Info, user_id: UUID, role_id: UUID, expires_at: Optional[datetime] = None
    ) -> AssignRoleResponse:
        """Assign a role to a user."""
        repo = info.context["repo"]
        tenant_id = info.context.get("tenant_id")
        granted_by = info.context["user_id"]

        # Check if role exists
        role_exists = await repo.run(
            DatabaseQuery(
                statement="SELECT 1 FROM roles WHERE id = %s",
                params={"role_id": str(role_id)},
                fetch_result=True,
            )
        )
        if not role_exists:
            raise ValueError(f"Role {role_id} not found")

        # Assign role
        await repo.run(
            DatabaseQuery(
                statement="""
                INSERT INTO user_roles (user_id, role_id, tenant_id, granted_by, expires_at)
                VALUES (%s, %s, %s, %s, %s)
                ON CONFLICT (user_id, role_id, tenant_id) DO NOTHING
            """,
                params={
                    "user_id": str(user_id),
                    "role_id": str(role_id),
                    "tenant_id": str(tenant_id) if tenant_id else None,
                    "granted_by": str(granted_by),
                    "expires_at": expires_at,
                },
                fetch_result=False,
            )
        )

        # Invalidate permission cache for user
        cache = info.context.get("permission_cache")
        if cache:
            await cache.invalidate_user(user_id, tenant_id)

        # Log to audit trail
        audit_logger = info.context.get("audit_logger")
        if audit_logger:
            await audit_logger.log_event(
                event_type="rbac.role.assigned",
                event_data={
                    "user_id": str(user_id),
                    "role_id": str(role_id),
                    "granted_by": str(granted_by),
                },
                user_id=str(granted_by),
                tenant_id=str(tenant_id) if tenant_id else None,
            )

        return AssignRoleResponse(success=True)


@strawberry.type
class CreateRoleResponse:
    role: Role


@strawberry.type
class AssignRoleResponse:
    success: bool
