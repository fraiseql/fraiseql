"""Tests for authorization policies in FraiseQL Python.

Demonstrates policy definition and reuse via @authz_policy decorator.
"""

import pytest
from typing import Annotated

import fraiseql
from fraiseql.scalars import ID
from fraiseql.security import AuthzPolicyType


class TestRBACPolicies:
    """Test role-based authorization policies."""

    def test_rbac_policy_definition_and_reference(self):
        """Test defining and referencing an RBAC policy."""

        @fraiseql.authz_policy(
            name="adminOnly",
            description="Access restricted to administrators",
            policy_type=AuthzPolicyType.RBAC,
            rule="hasRole($context, 'admin')",
            audit_logging=True,
        )
        class AdminOnlyPolicy:
            """Policy for admin-only access."""

            pass

        @fraiseql.type
        class AdminProtectedData:
            """Data protected by adminOnly policy."""

            id: ID

            @fraiseql.authorize(policy="adminOnly")
            sensitiveData: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("AdminProtectedData")
        assert type_info is not None

    def test_multiple_fields_with_rbac_policy(self):
        """Test multiple fields referencing same RBAC policy."""

        @fraiseql.authz_policy(
            name="piiAccess",
            description="Access to Personally Identifiable Information",
            policy_type=AuthzPolicyType.RBAC,
            rule="hasRole($context, 'data_manager') OR hasScope($context, 'read:pii')",
        )
        class PIIAccessPolicy:
            """Policy for PII access."""

            pass

        @fraiseql.type
        class Customer:
            """Customer with PII fields protected by policy."""

            id: ID
            name: str

            @fraiseql.authorize(policy="piiAccess")
            email: str

            @fraiseql.authorize(policy="piiAccess")
            phoneNumber: str

            @fraiseql.authorize(policy="piiAccess")
            socialSecurityNumber: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("Customer")
        assert type_info is not None


class TestABACPolicies:
    """Test attribute-based authorization policies."""

    def test_abac_policy_definition(self):
        """Test defining an ABAC policy."""

        @fraiseql.authz_policy(
            name="secretClearance",
            description="Requires top secret clearance",
            policy_type=AuthzPolicyType.ABAC,
            attributes=[
                "clearance_level >= 3",
                "background_check == true",
            ],
        )
        class ClearancePolicy:
            """Policy for secret clearance."""

            pass

        @fraiseql.type
        class SecretData:
            """Data protected by secret clearance policy."""

            id: ID

            @fraiseql.authorize(policy="secretClearance")
            classification: str

            @fraiseql.authorize(policy="secretClearance")
            content: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("SecretData")
        assert type_info is not None

    def test_attribute_conditions_in_policy(self):
        """Test policy with attribute conditions."""

        @fraiseql.authz_policy(
            name="financialData",
            description="Access to financial records",
            policy_type=AuthzPolicyType.ABAC,
            attributes=[
                "clearance_level >= 2",
                "department == 'finance'",
            ],
        )
        class FinancialAccessPolicy:
            """Policy for financial data."""

            pass

        @fraiseql.type
        class FinancialRecord:
            """Financial record protected by policy."""

            transactionId: ID

            @fraiseql.authorize(policy="financialData")
            transactionAmount: float

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("FinancialRecord")
        assert type_info is not None


class TestHybridPolicies:
    """Test hybrid authorization policies combining roles and attributes."""

    def test_hybrid_policy(self):
        """Test hybrid policy combining roles and attributes."""

        @fraiseql.authz_policy(
            name="auditAccess",
            description="Access to audit trails with role and attribute checks",
            policy_type=AuthzPolicyType.HYBRID,
            rule="hasRole($context, 'auditor')",
            attributes=["audit_enabled == true"],
        )
        class AuditAccessPolicy:
            """Hybrid policy for audit access."""

            pass

        @fraiseql.type
        class AuditLog:
            """Audit log protected by hybrid policy."""

            auditId: ID

            @fraiseql.authorize(policy="auditAccess")
            auditTrail: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("AuditLog")
        assert type_info is not None


class TestRecursivePolicies:
    """Test policies applied recursively to nested types."""

    def test_recursive_policy_application(self):
        """Test policy with recursive application to nested types."""

        @fraiseql.authz_policy(
            name="recursiveProtection",
            description="Recursively applies to nested types",
            policy_type=AuthzPolicyType.CUSTOM,
            rule="canAccessNested($context)",
            recursive=True,
        )
        class RecursiveAuthPolicy:
            """Policy for recursive protection."""

            pass

        @fraiseql.type
        class NestedData:
            """Nested data type."""

            value: str

        @fraiseql.type
        class ParentData:
            """Parent type with recursive policy."""

            id: ID

            @fraiseql.authorize(policy="recursiveProtection")
            nestedData: NestedData

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("ParentData")
        assert type_info is not None


class TestOperationSpecificPolicies:
    """Test policies specific to certain operations."""

    def test_operation_specific_policy(self):
        """Test policy that applies only to specific operations."""

        @fraiseql.authz_policy(
            name="readOnly",
            description="Policy applies only to read operations",
            policy_type=AuthzPolicyType.CUSTOM,
            rule="hasRole($context, 'viewer')",
            operations="read",
        )
        class ReadOnlyPolicy:
            """Policy for read-only access."""

            pass

        @fraiseql.type
        class ReadProtectedData:
            """Data protected by read-only policy."""

            id: ID

            @fraiseql.authorize(policy="readOnly")
            sensitiveField: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("ReadProtectedData")
        assert type_info is not None


class TestCachedPolicies:
    """Test authorization policies with caching."""

    def test_cached_policy(self):
        """Test policy with authorization decision caching."""

        @fraiseql.authz_policy(
            name="cachedAccess",
            description="Access control with result caching",
            policy_type=AuthzPolicyType.CUSTOM,
            rule="hasRole($context, 'viewer')",
            cacheable=True,
            cache_duration_seconds=3600,
        )
        class CachedAccessPolicy:
            """Policy with cached decisions."""

            pass

        @fraiseql.type
        class CachedProtectedData:
            """Data protected by cached policy."""

            id: ID

            @fraiseql.authorize(policy="cachedAccess")
            cachedField: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("CachedProtectedData")
        assert type_info is not None


class TestAuditedPolicies:
    """Test authorization policies with audit logging."""

    def test_audited_policy(self):
        """Test policy with audit logging enabled."""

        @fraiseql.authz_policy(
            name="auditedAccess",
            description="Access with comprehensive audit logging",
            policy_type=AuthzPolicyType.RBAC,
            rule="hasRole($context, 'auditor')",
            audit_logging=True,
        )
        class AuditedAccessPolicy:
            """Policy with audit logging."""

            pass

        @fraiseql.type
        class AuditedData:
            """Data protected by audited policy."""

            id: ID

            @fraiseql.authorize(policy="auditedAccess")
            sensitiveField: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("AuditedData")
        assert type_info is not None


class TestPolicyMutations:
    """Test mutations protected by authorization policies."""

    def test_mutation_with_policy(self):
        """Test mutation protected by authorization policy."""

        @fraiseql.authz_policy(
            name="adminOnly",
            rule="hasRole($context, 'admin')",
        )
        class AdminPolicy:
            """Admin-only policy."""

            pass

        @fraiseql.authorize(policy="adminOnly")
        @fraiseql.mutation
        def deleteUser(userId: str) -> bool:
            """Delete a user (admin only via policy)."""
            pass

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        mutation_info = registry.get_mutation("deleteUser")
        assert mutation_info is not None


class TestPolicyComposition:
    """Test composing multiple policies."""

    def test_multiple_policies_on_type(self):
        """Test multiple policies referenced on different fields."""

        @fraiseql.authz_policy(
            name="publicAccess",
            rule="true",  # Everyone has access
        )
        class PublicPolicy:
            """Public policy."""

            pass

        @fraiseql.authz_policy(
            name="piiAccess",
            rule="hasRole($context, 'data_manager')",
        )
        class PIIPolicy:
            """PII access policy."""

            pass

        @fraiseql.authz_policy(
            name="adminAccess",
            rule="hasRole($context, 'admin')",
        )
        class AdminPolicy:
            """Admin policy."""

            pass

        @fraiseql.type
        class ComposedData:
            """Data with multiple policies."""

            @fraiseql.authorize(policy="publicAccess")
            publicField: str

            @fraiseql.authorize(policy="piiAccess")
            piiField: str

            @fraiseql.authorize(policy="adminAccess")
            adminField: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("ComposedData")
        assert type_info is not None


class TestPolicyWithErrorMessages:
    """Test policies with custom error messages."""

    def test_policy_with_custom_error(self):
        """Test policy with custom error message."""

        @fraiseql.authz_policy(
            name="restrictedAccess",
            rule="hasRole($context, 'executive')",
            error_message="Only executive level users can access this resource",
        )
        class RestrictedPolicy:
            """Policy with custom error."""

            pass

        @fraiseql.type
        class RestrictedData:
            """Data with custom error message."""

            id: ID

            @fraiseql.authorize(policy="restrictedAccess")
            restrictedField: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("RestrictedData")
        assert type_info is not None
