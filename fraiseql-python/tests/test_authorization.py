"""Tests for custom authorization rules in FraiseQL Python.

Demonstrates field-level and type-level authorization via @authorize decorator.
"""

import pytest
from typing import Annotated

import fraiseql
from fraiseql.scalars import ID


class TestBasicAuthorizationRules:
    """Test basic authorization rule registration."""

    def test_type_with_authorization_rule(self):
        """Test registering a type with custom authorization rule."""

        @fraiseql.authorize(rule="isOwner($context.userId, $field.ownerId)")
        @fraiseql.type
        class ProtectedNote:
            """User can only access their own notes."""

            id: ID
            content: str
            ownerId: str

        # Verify type is registered
        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("ProtectedNote")
        assert type_info is not None

    def test_field_with_authorization_rule(self):
        """Test field-level authorization rules."""

        @fraiseql.type
        class ProtectedData:
            """Type with field-level authorization."""

            id: ID

            @fraiseql.authorize(
                rule="isOwner($context.userId, $field.ownerId) OR hasRole($context, 'admin')"
            )
            secret_field: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("ProtectedData")
        assert type_info is not None

    def test_authorization_with_custom_error_message(self):
        """Test custom error messages on authorization failures."""

        @fraiseql.authorize(
            rule="hasRole($context, 'auditor')",
            error_message="You do not have permission to access restricted data",
        )
        @fraiseql.type
        class CustomErrorData:
            """Type with custom error message."""

            id: ID
            restricted_field: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("CustomErrorData")
        assert type_info is not None

    def test_recursive_authorization(self):
        """Test recursive authorization on nested types."""

        @fraiseql.type
        class SecureNested:
            """Nested type with authorization."""

            value: str

        @fraiseql.authorize(rule="canAccessNested($context)", recursive=True)
        @fraiseql.type
        class SecureContainer:
            """Container with recursive authorization."""

            id: ID
            nested_secure_data: SecureNested

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("SecureContainer")
        assert type_info is not None

    def test_operation_specific_authorization(self):
        """Test authorization specific to certain operations."""

        @fraiseql.type
        class ReadProtectedData:
            """Data with read-specific authorization."""

            id: ID

            @fraiseql.authorize(
                rule="hasScope($context, 'read:sensitive')",
                operations="read",
            )
            sensitive_field: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("ReadProtectedData")
        assert type_info is not None

    def test_authorization_with_policy_reference(self):
        """Test authorization via policy reference."""

        @fraiseql.authorize(policy="piiAccess")
        @fraiseql.type
        class Customer:
            """Customer with PII protection via policy."""

            id: ID
            name: str
            email: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("Customer")
        assert type_info is not None


class TestAuthorizationQueries:
    """Test authorization on queries and mutations."""

    def test_query_with_authorization_protection(self):
        """Test query with authorization protection."""

        @fraiseql.type
        class ProtectedNote:
            """Note type."""

            id: ID
            content: str
            ownerId: str

        @fraiseql.authorize(rule="isOwner($context.userId, $field.ownerId)")
        @fraiseql.query
        def myNotes(userId: str) -> list[ProtectedNote]:
            """Get user's protected notes."""
            pass

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        query_info = registry.get_query("myNotes")
        assert query_info is not None

    def test_mutation_with_authorization_requirement(self):
        """Test mutation with authorization."""

        @fraiseql.type
        class ProtectedNote:
            """Note type."""

            id: ID
            content: str

        @fraiseql.authorize(rule="hasRole($context, 'editor')")
        @fraiseql.mutation
        def createNote(content: str, userId: str) -> ProtectedNote:
            """Create a protected note."""
            pass

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        mutation_info = registry.get_mutation("createNote")
        assert mutation_info is not None


class TestAuthorizationPatterns:
    """Test common authorization patterns."""

    def test_ownership_pattern(self):
        """Test ownership-based authorization pattern."""

        @fraiseql.authorize(rule="isOwner($context.userId, $field.ownerId)")
        @fraiseql.type
        class OwnedResource:
            """Resource that can only be accessed by owner."""

            id: ID
            ownerId: str
            data: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("OwnedResource")
        assert type_info is not None

    def test_admin_restriction_pattern(self):
        """Test admin-only access pattern."""

        @fraiseql.authorize(rule="hasRole($context, 'admin')")
        @fraiseql.type
        class AdminOnly:
            """Only admins can access this."""

            id: ID
            systemSetting: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("AdminOnly")
        assert type_info is not None

    def test_combined_conditions_pattern(self):
        """Test authorization with combined conditions."""

        @fraiseql.authorize(
            rule="(hasRole($context, 'manager') OR hasScope($context, 'read:employees')) "
            "AND $context.department == $field.department"
        )
        @fraiseql.type
        class DepartmentData:
            """Access requires manager role or scope AND matching department."""

            id: ID
            department: str
            salary: float

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("DepartmentData")
        assert type_info is not None

    def test_time_based_access_pattern(self):
        """Test time-based access control."""

        @fraiseql.authorize(
            rule="currentTime() >= $field.validFrom AND currentTime() <= $field.validUntil"
        )
        @fraiseql.type
        class TimeSensitiveData:
            """Data only accessible during specific time window."""

            id: ID
            content: str
            validFrom: str
            validUntil: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("TimeSensitiveData")
        assert type_info is not None


class TestAuthorizationCaching:
    """Test authorization decision caching."""

    def test_cacheable_authorization(self):
        """Test cached authorization decisions."""

        @fraiseql.authorize(
            rule="hasRole($context, 'viewer')",
            cacheable=True,
            cache_duration_seconds=3600,
        )
        @fraiseql.type
        class CachedProtectedData:
            """Authorization decisions cached for 1 hour."""

            id: ID
            data: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("CachedProtectedData")
        assert type_info is not None

    def test_non_cacheable_authorization(self):
        """Test non-cached authorization for sensitive checks."""

        @fraiseql.authorize(
            rule="hasActiveSession($context)",
            cacheable=False,
        )
        @fraiseql.type
        class SessionSensitiveData:
            """Authorization not cached due to session sensitivity."""

            id: ID
            data: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("SessionSensitiveData")
        assert type_info is not None
