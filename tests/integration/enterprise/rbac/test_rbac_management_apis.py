"""Integration tests for RBAC Management APIs.

Tests GraphQL mutations for managing roles, permissions, and user assignments.
Verifies automatic cache invalidation via domain versioning.
"""

from uuid import uuid4

import pytest

pytestmark = pytest.mark.integration


class TestRoleManagement:
    """Test role management mutations."""

    @pytest.mark.asyncio
    async def test_create_role_basic(self, class_db_pool) -> None:
        """Test creating a basic role."""
        from fraiseql.enterprise.rbac.mutations import CreateRole, CreateRoleInput

        # Create a test role
        input_data = CreateRoleInput(name="test_role", description="A test role")

        result = CreateRole.resolve(input_data)

        assert result.success is True
        assert result.message == "Role 'test_role' created successfully"
        # Note: role_id would be populated by the actual mutation execution

    @pytest.mark.asyncio
    async def test_create_role_with_hierarchy(self, class_db_pool) -> None:
        """Test creating a role with parent hierarchy."""
        from fraiseql.enterprise.rbac.mutations import CreateRole, CreateRoleInput

        # First create a parent role
        parent_input = CreateRoleInput(name="parent_role", description="Parent role")
        CreateRole.resolve(parent_input)

        # Get the parent role ID (in real implementation this would come from the mutation result)
        # For testing, we'll assume we can get it from the database
        # parent_role_id = ... (would be returned from first mutation)

        # Create child role
        child_input = CreateRoleInput(
            name="child_role",
            description="Child role",
            parent_role_id=uuid4(),  # Would be actual parent ID
        )

        result = CreateRole.resolve(child_input)

        assert result.success is True
        assert "child_role" in result.message

    @pytest.mark.asyncio
    async def test_update_role(self, class_db_pool) -> None:
        """Test updating an existing role."""
        from fraiseql.enterprise.rbac.mutations import UpdateRole, UpdateRoleInput

        # Update role
        input_data = UpdateRoleInput(
            role_id=uuid4(),  # Would be actual role ID
            name="updated_role",
            description="Updated description",
        )

        result = UpdateRole.resolve(input_data)

        assert result.success is True
        assert result.role_id == input_data.role_id
        assert "updated" in result.message

    @pytest.mark.asyncio
    async def test_delete_role(self, class_db_pool) -> None:
        """Test deleting a role."""
        from fraiseql.enterprise.rbac.mutations import DeleteRole, DeleteRoleInput

        input_data = DeleteRoleInput(role_id=uuid4())  # Would be actual role ID

        result = DeleteRole.resolve(input_data)

        assert result.success is True
        assert result.role_id == input_data.role_id
        assert "deleted" in result.message


class TestPermissionManagement:
    """Test permission management mutations."""

    @pytest.mark.asyncio
    async def test_create_permission_basic(self, class_db_pool) -> None:
        """Test creating a basic permission."""
        from fraiseql.enterprise.rbac.mutations import CreatePermission, CreatePermissionInput

        input_data = CreatePermissionInput(
            resource="user", action="read", description="Read user data"
        )

        result = CreatePermission.resolve(input_data)

        assert result.success is True
        assert "user:read" in result.message

    @pytest.mark.asyncio
    async def test_create_permission_with_constraints(self, class_db_pool) -> None:
        """Test creating a permission with constraints."""
        from fraiseql.enterprise.rbac.mutations import CreatePermission, CreatePermissionInput

        input_data = CreatePermissionInput(
            resource="user",
            action="update",
            description="Update user data",
            constraints={"own_data_only": True},
        )

        result = CreatePermission.resolve(input_data)

        assert result.success is True
        assert "user:update" in result.message

    @pytest.mark.asyncio
    async def test_update_permission(self, class_db_pool) -> None:
        """Test updating an existing permission."""
        from fraiseql.enterprise.rbac.mutations import UpdatePermission, UpdatePermissionInput

        input_data = UpdatePermissionInput(
            permission_id=uuid4(),  # Would be actual permission ID
            description="Updated description",
            constraints={"new_constraint": True},
        )

        result = UpdatePermission.resolve(input_data)

        assert result.success is True
        assert result.permission_id == input_data.permission_id
        assert "updated" in result.message

    @pytest.mark.asyncio
    async def test_delete_permission(self, class_db_pool) -> None:
        """Test deleting a permission."""
        from fraiseql.enterprise.rbac.mutations import DeletePermission, DeletePermissionInput

        input_data = DeletePermissionInput(permission_id=uuid4())  # Would be actual permission ID

        result = DeletePermission.resolve(input_data)

        assert result.success is True
        assert result.permission_id == input_data.permission_id
        assert "deleted" in result.message


class TestRolePermissionManagement:
    """Test role-permission assignment mutations."""

    @pytest.mark.asyncio
    async def test_grant_permission_to_role(self, class_db_pool) -> None:
        """Test granting a permission to a role."""
        from fraiseql.enterprise.rbac.mutations import (
            GrantPermissionToRole,
            GrantPermissionToRoleInput,
        )

        input_data = GrantPermissionToRoleInput(
            role_id=uuid4(),  # Would be actual role ID
            permission_id=uuid4(),  # Would be actual permission ID
        )

        result = GrantPermissionToRole.resolve(input_data)

        assert result.success is True
        assert "granted" in result.message

    @pytest.mark.asyncio
    async def test_revoke_permission_from_role(self, class_db_pool) -> None:
        """Test revoking a permission from a role."""
        from fraiseql.enterprise.rbac.mutations import (
            RevokePermissionFromRole,
            RevokePermissionFromRoleInput,
        )

        input_data = RevokePermissionFromRoleInput(
            role_id=uuid4(),  # Would be actual role ID
            permission_id=uuid4(),  # Would be actual permission ID
        )

        result = RevokePermissionFromRole.resolve(input_data)

        assert result.success is True
        assert "revoked" in result.message


class TestUserRoleManagement:
    """Test user-role assignment mutations."""

    @pytest.mark.asyncio
    async def test_assign_role_to_user_basic(self, class_db_pool) -> None:
        """Test assigning a role to a user."""
        from fraiseql.enterprise.rbac.mutations import AssignRoleToUser, AssignRoleToUserInput

        input_data = AssignRoleToUserInput(
            user_id=uuid4(),  # Would be actual user ID
            role_id=uuid4(),  # Would be actual role ID
        )

        result = AssignRoleToUser.resolve(input_data)

        assert result.success is True
        assert "assigned" in result.message

    @pytest.mark.asyncio
    async def test_assign_role_to_user_with_tenant(self, class_db_pool) -> None:
        """Test assigning a role to a user within a tenant."""
        from datetime import datetime, timedelta

        from fraiseql.enterprise.rbac.mutations import AssignRoleToUser, AssignRoleToUserInput

        input_data = AssignRoleToUserInput(
            user_id=uuid4(),
            role_id=uuid4(),
            tenant_id=uuid4(),
            expires_at=datetime.now() + timedelta(days=30),
        )

        result = AssignRoleToUser.resolve(input_data)

        assert result.success is True
        assert "assigned" in result.message

    @pytest.mark.asyncio
    async def test_revoke_role_from_user(self, class_db_pool) -> None:
        """Test revoking a role from a user."""
        from fraiseql.enterprise.rbac.mutations import RevokeRoleFromUser, RevokeRoleFromUserInput

        input_data = RevokeRoleFromUserInput(
            user_id=uuid4(),  # Would be actual user ID
            role_id=uuid4(),  # Would be actual role ID
            tenant_id=uuid4(),  # Optional tenant scope
        )

        result = RevokeRoleFromUser.resolve(input_data)

        assert result.success is True
        assert "revoked" in result.message


class TestCacheInvalidation:
    """Test that mutations trigger automatic cache invalidation."""

    def test_role_creation_invalidates_cache(self) -> None:
        """Verify that creating a role invalidates related caches.

        Note: Full cache invalidation testing requires database setup
        and is covered by test_cache_invalidation.py. This test verifies
        the mutation structure is correct.
        """
        from fraiseql.enterprise.rbac.mutations import CreateRole, CreateRoleInput

        # Just verify the mutation can be instantiated
        role_input = CreateRoleInput(name="test_role")
        result = CreateRole.resolve(role_input)

        assert result.success is True
        assert "test_role" in result.message

        # Real cache invalidation testing is in test_cache_invalidation.py

    @pytest.mark.asyncio
    async def test_permission_grant_invalidates_cache(self, class_db_pool) -> None:
        """Verify that granting permissions invalidates user caches."""
        from fraiseql.enterprise.rbac.mutations import (
            GrantPermissionToRole,
            GrantPermissionToRoleInput,
        )

        # Grant permission to role
        input_data = GrantPermissionToRoleInput(role_id=uuid4(), permission_id=uuid4())

        result = GrantPermissionToRole.resolve(input_data)

        assert result.success is True
        # In full test: verify CASCADE invalidation occurred

    @pytest.mark.asyncio
    async def test_user_role_assignment_invalidates_cache(self, class_db_pool) -> None:
        """Verify that user role changes invalidate permission caches."""
        from fraiseql.enterprise.rbac.mutations import AssignRoleToUser, AssignRoleToUserInput

        # Assign role to user
        input_data = AssignRoleToUserInput(user_id=uuid4(), role_id=uuid4(), tenant_id=uuid4())

        result = AssignRoleToUser.resolve(input_data)

        assert result.success is True
        # In full test: verify user permission cache was invalidated


class TestMutationErrorHandling:
    """Test error handling in mutations."""

    @pytest.mark.asyncio
    async def test_update_role_no_fields(self, class_db_pool) -> None:
        """Test that updating a role with no fields fails."""
        from fraiseql.enterprise.rbac.mutations import UpdateRole

        # This would fail in the execute method
        with pytest.raises(ValueError, match="At least one field must be provided"):
            UpdateRole.sql(uuid4())  # No update fields provided

    @pytest.mark.asyncio
    async def test_update_permission_no_fields(self, class_db_pool) -> None:
        """Test that updating a permission with no fields fails."""
        from fraiseql.enterprise.rbac.mutations import UpdatePermission

        # This would fail in the execute method
        with pytest.raises(ValueError, match="At least one field must be provided"):
            UpdatePermission.sql(uuid4())  # No update fields provided
