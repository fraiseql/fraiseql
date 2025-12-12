"""RBAC Management Mutations

GraphQL mutations for managing roles, permissions, and user assignments.
All mutations automatically invalidate caches via PostgreSQL domain versioning.
"""

from datetime import datetime
from typing import Optional
from uuid import UUID

from fraiseql.mutations.mutation_decorator import mutation
from fraiseql.strawberry_compat import strawberry


# Input types for role management
@strawberry.input
class CreateRoleInput:
    """Input for creating a new role."""

    name: str
    description: Optional[str] = None
    parent_role_id: Optional[UUID] = None
    tenant_id: Optional[UUID] = None


@strawberry.input
class UpdateRoleInput:
    """Input for updating an existing role."""

    role_id: UUID
    name: Optional[str] = None
    description: Optional[str] = None
    parent_role_id: Optional[UUID] = None


@strawberry.input
class DeleteRoleInput:
    """Input for deleting a role."""

    role_id: UUID


# Input types for permission management
@strawberry.input
class CreatePermissionInput:
    """Input for creating a new permission."""

    resource: str
    action: str
    description: Optional[str] = None
    constraints: Optional[dict] = None


@strawberry.input
class UpdatePermissionInput:
    """Input for updating an existing permission."""

    permission_id: UUID
    resource: Optional[str] = None
    action: Optional[str] = None
    description: Optional[str] = None
    constraints: Optional[dict] = None


@strawberry.input
class DeletePermissionInput:
    """Input for deleting a permission."""

    permission_id: UUID


# Input types for role-permission management
@strawberry.input
class GrantPermissionToRoleInput:
    """Input for granting a permission to a role."""

    role_id: UUID
    permission_id: UUID


@strawberry.input
class RevokePermissionFromRoleInput:
    """Input for revoking a permission from a role."""

    role_id: UUID
    permission_id: UUID


# Input types for user-role management
@strawberry.input
class AssignRoleToUserInput:
    """Input for assigning a role to a user."""

    user_id: UUID
    role_id: UUID
    tenant_id: Optional[UUID] = None
    expires_at: Optional[datetime] = None


@strawberry.input
class RevokeRoleFromUserInput:
    """Input for revoking a role from a user."""

    user_id: UUID
    role_id: UUID
    tenant_id: Optional[UUID] = None


# Result types
@strawberry.type
class RoleMutationResult:
    """Result of a role mutation."""

    success: bool
    role_id: Optional[UUID] = None
    message: str


@strawberry.type
class PermissionMutationResult:
    """Result of a permission mutation."""

    success: bool
    permission_id: Optional[UUID] = None
    message: str


@strawberry.type
class RolePermissionMutationResult:
    """Result of a role-permission mutation."""

    success: bool
    role_permission_id: Optional[UUID] = None
    message: str


@strawberry.type
class UserRoleMutationResult:
    """Result of a user-role mutation."""

    success: bool
    user_role_id: Optional[UUID] = None
    message: str


# Role Management Mutations
@mutation
class CreateRole:
    """Create a new role with optional hierarchy."""

    input: CreateRoleInput
    success: RoleMutationResult
    error: RoleMutationResult

    @staticmethod
    def sql(
        name: str,
        description: Optional[str] = None,
        parent_role_id: Optional[UUID] = None,
        tenant_id: Optional[UUID] = None,
    ) -> str:
        """Generate SQL to create a new role."""
        columns = ["name"]
        values = ["%s"]
        params = [name]

        if description is not None:
            columns.append("description")
            values.append("%s")
            params.append(description)

        if parent_role_id is not None:
            columns.append("parent_role_id")
            values.append("%s")
            params.append(str(parent_role_id))

        if tenant_id is not None:
            columns.append("tenant_id")
            values.append("%s")
            params.append(str(tenant_id))

        columns_str = ", ".join(columns)
        placeholders = ", ".join(["%s"] * len(params))

        return f"""
        INSERT INTO roles ({columns_str})
        VALUES ({placeholders})
        RETURNING id
        """

    @staticmethod
    def execute(
        name: str,
        description: Optional[str] = None,
        parent_role_id: Optional[UUID] = None,
        tenant_id: Optional[UUID] = None,
    ) -> dict:
        """Execute role creation."""
        return {
            "name": name,
            "description": description,
            "parent_role_id": parent_role_id,
            "tenant_id": tenant_id,
        }

    @classmethod
    def resolve(cls, _input: CreateRoleInput) -> RoleMutationResult:
        """GraphQL resolver for role creation."""
        # Note: The mutation decorator handles SQL execution
        # This resolver is called after successful execution
        return RoleMutationResult(
            success=True,
            role_id=None,  # Would be returned from INSERT RETURNING
            message=f"Role '{_input.name}' created successfully",
        )


@mutation
class UpdateRole:
    """Update an existing role."""

    input: UpdateRoleInput
    success: RoleMutationResult
    error: RoleMutationResult

    @staticmethod
    def sql(
        role_id: UUID,
        name: Optional[str] = None,
        description: Optional[str] = None,
        parent_role_id: Optional[UUID] = None,
    ) -> str:
        """Generate SQL to update a role."""
        updates = []
        params = []

        if name is not None:
            updates.append("name = %s")
            params.append(name)

        if description is not None:
            updates.append("description = %s")
            params.append(description)

        if parent_role_id is not None:
            updates.append("parent_role_id = %s")
            params.append(str(parent_role_id))

        if not updates:
            raise ValueError("At least one field must be provided for update")

        updates.append("updated_at = NOW()")
        updates_str = ", ".join(updates)
        params.append(str(role_id))

        return f"""
        UPDATE roles
        SET {updates_str}
        WHERE id = %s
        RETURNING id
        """

    @staticmethod
    def execute(
        role_id: UUID,
        name: Optional[str] = None,
        description: Optional[str] = None,
        parent_role_id: Optional[UUID] = None,
    ) -> dict:
        """Execute role update."""
        return {
            "role_id": role_id,
            "name": name,
            "description": description,
            "parent_role_id": parent_role_id,
        }

    @classmethod
    def resolve(cls, _input: UpdateRoleInput) -> RoleMutationResult:
        """GraphQL resolver for role update."""
        return RoleMutationResult(
            success=True,
            role_id=_input.role_id,
            message="Role updated successfully",
        )


@mutation
class DeleteRole:
    """Delete a role (only if not system role and not referenced)."""

    input: DeleteRoleInput
    success: RoleMutationResult
    error: RoleMutationResult

    @staticmethod
    def sql(role_id: UUID) -> str:
        """Generate SQL to delete a role."""
        return """
        DELETE FROM roles
        WHERE id = %s AND is_system = FALSE
        RETURNING id
        """

    @staticmethod
    def execute(role_id: UUID) -> dict:
        """Execute role deletion."""
        return {"role_id": role_id}

    @classmethod
    def resolve(cls, _input: DeleteRoleInput) -> RoleMutationResult:
        """GraphQL resolver for role deletion."""
        return RoleMutationResult(
            success=True,
            role_id=_input.role_id,
            message="Role deleted successfully",
        )


# Permission Management Mutations
@mutation
class CreatePermission:
    """Create a new permission."""

    input: CreatePermissionInput
    success: PermissionMutationResult
    error: PermissionMutationResult

    @staticmethod
    def sql(
        resource: str,
        action: str,
        description: Optional[str] = None,
        constraints: Optional[dict] = None,
    ) -> str:
        """Generate SQL to create a new permission."""
        columns = ["resource", "action"]
        values = ["%s", "%s"]
        params = [resource, action]

        if description is not None:
            columns.append("description")
            values.append("%s")
            params.append(description)

        if constraints is not None:
            columns.append("constraints")
            values.append("%s")
            params.append(constraints)

        columns_str = ", ".join(columns)
        placeholders = ", ".join(["%s"] * len(params))

        return f"""
        INSERT INTO permissions ({columns_str})
        VALUES ({placeholders})
        RETURNING id
        """

    @staticmethod
    def execute(
        resource: str,
        action: str,
        description: Optional[str] = None,
        constraints: Optional[dict] = None,
    ) -> dict:
        """Execute permission creation."""
        return {
            "resource": resource,
            "action": action,
            "description": description,
            "constraints": constraints,
        }

    @classmethod
    def resolve(cls, _input: CreatePermissionInput) -> PermissionMutationResult:
        """GraphQL resolver for permission creation."""
        return PermissionMutationResult(
            success=True,
            permission_id=None,  # Would be returned from INSERT RETURNING
            message=f"Permission '{_input.resource}:{_input.action}' created successfully",
        )


@mutation
class UpdatePermission:
    """Update an existing permission."""

    input: UpdatePermissionInput
    success: PermissionMutationResult
    error: PermissionMutationResult

    @staticmethod
    def sql(
        permission_id: UUID,
        resource: Optional[str] = None,
        action: Optional[str] = None,
        description: Optional[str] = None,
        constraints: Optional[dict] = None,
    ) -> str:
        """Generate SQL to update a permission."""
        updates = []
        params = []

        if resource is not None:
            updates.append("resource = %s")
            params.append(resource)

        if action is not None:
            updates.append("action = %s")
            params.append(action)

        if description is not None:
            updates.append("description = %s")
            params.append(description)

        if constraints is not None:
            updates.append("constraints = %s")
            params.append(constraints)

        if not updates:
            raise ValueError("At least one field must be provided for update")

        updates_str = ", ".join(updates)
        params.append(str(permission_id))

        return f"""
        UPDATE permissions
        SET {updates_str}
        WHERE id = %s
        RETURNING id
        """

    @staticmethod
    def execute(
        permission_id: UUID,
        resource: Optional[str] = None,
        action: Optional[str] = None,
        description: Optional[str] = None,
        constraints: Optional[dict] = None,
    ) -> dict:
        """Execute permission update."""
        return {
            "permission_id": permission_id,
            "resource": resource,
            "action": action,
            "description": description,
            "constraints": constraints,
        }

    @classmethod
    def resolve(cls, _input: UpdatePermissionInput) -> PermissionMutationResult:
        """GraphQL resolver for permission update."""
        return PermissionMutationResult(
            success=True,
            permission_id=_input.permission_id,
            message="Permission updated successfully",
        )


@mutation
class DeletePermission:
    """Delete a permission."""

    input: DeletePermissionInput
    success: PermissionMutationResult
    error: PermissionMutationResult

    @staticmethod
    def sql(permission_id: UUID) -> str:
        """Generate SQL to delete a permission."""
        return """
        DELETE FROM permissions
        WHERE id = %s
        RETURNING id
        """

    @staticmethod
    def execute(permission_id: UUID) -> dict:
        """Execute permission deletion."""
        return {"permission_id": permission_id}

    @classmethod
    def resolve(cls, _input: DeletePermissionInput) -> PermissionMutationResult:
        """GraphQL resolver for permission deletion."""
        return PermissionMutationResult(
            success=True,
            permission_id=_input.permission_id,
            message="Permission deleted successfully",
        )


# Role-Permission Management Mutations
@mutation
class GrantPermissionToRole:
    """Grant a permission to a role."""

    input: GrantPermissionToRoleInput
    success: RolePermissionMutationResult
    error: RolePermissionMutationResult

    @staticmethod
    def sql(role_id: UUID, permission_id: UUID) -> str:
        """Generate SQL to grant permission to role."""
        return """
        INSERT INTO role_permissions (role_id, permission_id, granted)
        VALUES (%s, %s, TRUE)
        ON CONFLICT (role_id, permission_id) DO UPDATE SET
            granted = TRUE
        RETURNING id
        """

    @staticmethod
    def execute(role_id: UUID, permission_id: UUID) -> dict:
        """Execute permission grant."""
        return {
            "role_id": role_id,
            "permission_id": permission_id,
        }

    @classmethod
    def resolve(cls, _input: GrantPermissionToRoleInput) -> RolePermissionMutationResult:
        """GraphQL resolver for granting permission."""
        return RolePermissionMutationResult(
            success=True,
            role_permission_id=None,  # Would be returned from INSERT RETURNING
            message="Permission granted to role successfully",
        )


@mutation
class RevokePermissionFromRole:
    """Revoke a permission from a role."""

    input: RevokePermissionFromRoleInput
    success: RolePermissionMutationResult
    error: RolePermissionMutationResult

    @staticmethod
    def sql(role_id: UUID, permission_id: UUID) -> str:
        """Generate SQL to revoke permission from role."""
        return """
        UPDATE role_permissions
        SET granted = FALSE
        WHERE role_id = %s AND permission_id = %s
        RETURNING id
        """

    @staticmethod
    def execute(role_id: UUID, permission_id: UUID) -> dict:
        """Execute permission revoke."""
        return {
            "role_id": role_id,
            "permission_id": permission_id,
        }

    @classmethod
    def resolve(cls, _input: RevokePermissionFromRoleInput) -> RolePermissionMutationResult:
        """GraphQL resolver for revoking permission."""
        return RolePermissionMutationResult(
            success=True,
            role_permission_id=None,  # Would be returned from UPDATE RETURNING
            message="Permission revoked from role successfully",
        )


# User-Role Management Mutations
@mutation
class AssignRoleToUser:
    """Assign a role to a user."""

    input: AssignRoleToUserInput
    success: UserRoleMutationResult
    error: UserRoleMutationResult

    @staticmethod
    def sql(
        user_id: UUID,
        role_id: UUID,
        tenant_id: Optional[UUID] = None,
        expires_at: Optional[datetime] = None,
    ) -> str:
        """Generate SQL to assign role to user."""
        columns = ["user_id", "role_id"]
        values = ["%s", "%s"]
        params = [str(user_id), str(role_id)]

        if tenant_id is not None:
            columns.append("tenant_id")
            values.append("%s")
            params.append(str(tenant_id))

        if expires_at is not None:
            columns.append("expires_at")
            values.append("%s")
            params.append(expires_at)

        columns_str = ", ".join(columns)
        placeholders = ", ".join(["%s"] * len(params))

        return f"""
        INSERT INTO user_roles ({columns_str})
        VALUES ({placeholders})
        ON CONFLICT (user_id, role_id,
            COALESCE(tenant_id, '00000000-0000-0000-0000-000000000000'::uuid))
        DO NOTHING
        RETURNING id
        """

    @staticmethod
    def execute(
        user_id: UUID,
        role_id: UUID,
        tenant_id: Optional[UUID] = None,
        expires_at: Optional[datetime] = None,
    ) -> dict:
        """Execute role assignment."""
        return {
            "user_id": user_id,
            "role_id": role_id,
            "tenant_id": tenant_id,
            "expires_at": expires_at,
        }

    @classmethod
    def resolve(cls, _input: AssignRoleToUserInput) -> UserRoleMutationResult:
        """GraphQL resolver for role assignment."""
        return UserRoleMutationResult(
            success=True,
            user_role_id=None,  # Would be returned from INSERT RETURNING
            message="Role assigned to user successfully",
        )


@mutation
class RevokeRoleFromUser:
    """Revoke a role from a user."""

    input: RevokeRoleFromUserInput
    success: UserRoleMutationResult
    error: UserRoleMutationResult

    @staticmethod
    def sql(
        user_id: UUID,
        role_id: UUID,
        tenant_id: Optional[UUID] = None,
    ) -> str:
        """Generate SQL to revoke role from user."""
        if tenant_id is not None:
            return """
            DELETE FROM user_roles
            WHERE user_id = %s AND role_id = %s AND tenant_id = %s
            RETURNING id
            """
        return """
            DELETE FROM user_roles
            WHERE user_id = %s AND role_id = %s AND tenant_id IS NULL
            RETURNING id
            """

    @staticmethod
    def execute(
        user_id: UUID,
        role_id: UUID,
        tenant_id: Optional[UUID] = None,
    ) -> dict:
        """Execute role revocation."""
        return {
            "user_id": user_id,
            "role_id": role_id,
            "tenant_id": tenant_id,
        }

    @classmethod
    def resolve(cls, _input: RevokeRoleFromUserInput) -> UserRoleMutationResult:
        """GraphQL resolver for role revocation."""
        return UserRoleMutationResult(
            success=True,
            user_role_id=None,  # Would be returned from DELETE RETURNING
            message="Role revoked from user successfully",
        )
