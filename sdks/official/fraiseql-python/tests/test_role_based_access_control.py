"""Tests for role-based access control (RBAC) in FraiseQL Python.

Demonstrates @role_required decorator for RBAC patterns.
"""

import pytest
from typing import Annotated

import fraiseql
from fraiseql.scalars import ID
from fraiseql.security import RoleMatchStrategy


class TestSingleRoleRequirements:
    """Test types with single role requirements."""

    def test_field_with_single_role_requirement(self):
        """Test field requiring a single role."""

        @fraiseql.type
        class AdminPanel:
            """Panel with admin-only field."""

            id: ID

            @fraiseql.role_required(roles="admin")
            systemSettings: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("AdminPanel")
        assert type_info is not None

    def test_type_with_admin_role_requirement(self):
        """Test entire type requiring admin role."""

        @fraiseql.role_required(roles="admin")
        @fraiseql.type
        class SystemConfiguration:
            """System configuration accessible only by admins."""

            id: ID
            databaseUrl: str
            apiKey: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("SystemConfiguration")
        assert type_info is not None


class TestMultipleRoleRequirements:
    """Test types with multiple role requirements."""

    def test_multiple_roles_any_strategy(self):
        """Test field accessible by multiple roles (ANY)."""

        @fraiseql.type
        class SalaryData:
            """Salary data accessible by manager, HR, or admin."""

            employeeId: ID

            @fraiseql.role_required(
                roles=["manager", "hr", "admin"],
                strategy=RoleMatchStrategy.ANY,
            )
            salary: float

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("SalaryData")
        assert type_info is not None

    def test_multiple_roles_all_strategy(self):
        """Test field requiring all roles (ALL)."""

        @fraiseql.type
        class ComplianceReport:
            """Report requiring both compliance officer and auditor roles."""

            reportId: ID

            @fraiseql.role_required(
                roles=["compliance_officer", "auditor"],
                strategy=RoleMatchStrategy.ALL,
            )
            auditTrail: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("ComplianceReport")
        assert type_info is not None

    def test_multiple_roles_exactly_strategy(self):
        """Test field requiring exact role match (EXACTLY)."""

        @fraiseql.type
        class ExactRoleData:
            """Data requiring exact role match."""

            id: ID

            @fraiseql.role_required(
                roles=["supervisor", "manager"],
                strategy=RoleMatchStrategy.EXACTLY,
            )
            supervisorData: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("ExactRoleData")
        assert type_info is not None


class TestRoleHierarchies:
    """Test role hierarchy support."""

    def test_role_hierarchy(self):
        """Test role hierarchies (higher roles inherit lower permissions)."""

        @fraiseql.type
        class ManagerData:
            """Data requiring manager role with hierarchy."""

            id: ID

            @fraiseql.role_required(roles="manager", hierarchy=True)
            budgetAmount: float

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("ManagerData")
        assert type_info is not None

    def test_hierarchical_access_levels(self):
        """Test hierarchical access levels."""

        @fraiseql.type
        class AccessLevels:
            """Data with multiple access levels."""

            id: ID

            @fraiseql.role_required(roles="employee", hierarchy=True)
            basicInfo: str

            @fraiseql.role_required(roles="manager", hierarchy=True)
            performanceData: str

            @fraiseql.role_required(roles="director", hierarchy=True)
            strategicData: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("AccessLevels")
        assert type_info is not None


class TestOperationSpecificRoles:
    """Test operation-specific role requirements."""

    def test_role_requirement_for_delete_only(self):
        """Test role requirement specific to delete operations."""

        @fraiseql.type
        class UserAccount:
            """User account with delete-specific role requirement."""

            id: ID
            email: str

            @fraiseql.role_required(roles="admin", operations="delete")
            accountStatus: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("UserAccount")
        assert type_info is not None

    def test_different_roles_for_different_operations(self):
        """Test different roles for different operations."""

        @fraiseql.type
        class MultiOpData:
            """Data with operation-specific role requirements."""

            id: ID

            @fraiseql.role_required(roles="viewer", operations="read")
            viewableData: str

            @fraiseql.role_required(roles="editor", operations="create,update")
            editableData: str

            @fraiseql.role_required(roles="admin", operations="delete")
            deletableData: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("MultiOpData")
        assert type_info is not None


class TestRoleProtectedMutations:
    """Test role-protected mutations."""

    def test_mutation_restricted_to_admin(self):
        """Test mutation restricted to admin role."""

        @fraiseql.type
        class User:
            """User type."""

            id: ID
            name: str

        @fraiseql.role_required(roles="admin")
        @fraiseql.mutation
        def deleteUser(userId: str) -> bool:
            """Delete a user (admin only)."""
            pass

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        mutation_info = registry.get_mutation("deleteUser")
        assert mutation_info is not None

    def test_mutation_requiring_multiple_roles(self):
        """Test mutation requiring multiple roles."""

        @fraiseql.type
        class DataTransfer:
            """Data transfer type."""

            id: ID
            amount: float

        @fraiseql.role_required(
            roles=["approver", "auditor"],
            strategy=RoleMatchStrategy.ALL,
        )
        @fraiseql.mutation
        def transferFunds(
            fromAccount: str, toAccount: str, amount: float
        ) -> bool:
            """Transfer funds (requires both approver and auditor)."""
            pass

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        mutation_info = registry.get_mutation("transferFunds")
        assert mutation_info is not None


class TestRoleInheritance:
    """Test role requirement inheritance."""

    def test_role_inheritance_from_type(self):
        """Test role requirements inherited from type to fields."""

        @fraiseql.role_required(roles="viewer", inherit=True)
        @fraiseql.type
        class TypeLevelProtected:
            """Type-level role inherited by fields."""

            id: ID

            @fraiseql.role_required(roles="editor")
            additionalData: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("TypeLevelProtected")
        assert type_info is not None

    def test_no_inheritance_when_disabled(self):
        """Test disabling role inheritance."""

        @fraiseql.role_required(roles="viewer", inherit=False)
        @fraiseql.type
        class NoInheritance:
            """Type-level role not inherited."""

            id: ID
            data: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("NoInheritance")
        assert type_info is not None


class TestRBACSuperposition:
    """Test RBAC patterns with custom error messages."""

    def test_role_with_custom_error_message(self):
        """Test custom error message for role failure."""

        @fraiseql.role_required(
            roles="manager",
            error_message="You must have the manager role to access this",
        )
        @fraiseql.type
        class CustomErrorMessage:
            """Type with custom role error message."""

            id: ID
            data: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("CustomErrorMessage")
        assert type_info is not None

    def test_role_with_description(self):
        """Test role requirement with description."""

        @fraiseql.role_required(
            roles="manager",
            description="Requires manager or higher roles to view sensitive salary data",
        )
        @fraiseql.type
        class DescribedRole:
            """Type with role description."""

            id: ID
            salary: float

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("DescribedRole")
        assert type_info is not None


class TestRoleQueriesAndQueries:
    """Test role requirements on queries."""

    def test_query_with_role_requirement(self):
        """Test query with role requirement."""

        @fraiseql.type
        class Report:
            """Report type."""

            id: ID
            data: str

        @fraiseql.role_required(roles="analyst")
        @fraiseql.query
        def analyticsReport() -> Report:
            """Analytics report (analyst role required)."""
            pass

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        query_info = registry.get_query("analyticsReport")
        assert query_info is not None
