"""RED Phase: Tests for field-level scope requirements (scope-based RBAC).

These tests verify that field scopes defined with fraiseql.field(requires_scope=...)
are properly collected, exported in schema.json, and ready for compiler integration.
"""

import json
from typing import Annotated

import pytest

import fraiseql
from fraiseql.scalars import ID
from fraiseql.registry import SchemaRegistry


class TestFieldScopeDeclaration:
    """Test declaring field-level scope requirements."""

    def test_field_with_single_scope_requirement(self):
        """Test field with a single scope requirement.

        RED: Verify that field scope is included in extracted field info.
        """
        @fraiseql.type
        class User:
            """User with public and sensitive fields."""

            id: ID
            name: str
            # Sensitive field requiring specific scope
            email: Annotated[str, fraiseql.field(requires_scope="read:User.email")]

        from fraiseql.types import extract_field_info

        field_info = extract_field_info(User)

        # Verify scope is in field info (RED: This should pass now)
        assert "email" in field_info
        assert field_info["email"]["requires_scope"] == "read:User.email"

    def test_field_with_custom_scope(self):
        """Test field with custom scope format."""

        @fraiseql.type
        class Employee:
            """Employee with custom scope requirements."""

            id: ID
            name: str
            # Custom scope for HR-only data
            salary: Annotated[float, fraiseql.field(requires_scope="hr:view_compensation")]
            # PII scope
            ssn: Annotated[str, fraiseql.field(requires_scope="pii:view")]

        from fraiseql.types import extract_field_info

        field_info = extract_field_info(Employee)

        assert field_info["salary"]["requires_scope"] == "hr:view_compensation"
        assert field_info["ssn"]["requires_scope"] == "pii:view"

    def test_field_scope_and_description_together(self):
        """Test field can have both scope and description."""

        @fraiseql.type
        class Product:
            """Product with documentation and access control."""

            id: ID
            name: str
            cost: Annotated[float, fraiseql.field(
                requires_scope="read:Product.cost",
                description="Internal cost of the product"
            )]

        from fraiseql.types import extract_field_info

        field_info = extract_field_info(Product)

        assert field_info["cost"]["requires_scope"] == "read:Product.cost"
        assert field_info["cost"]["description"] == "Internal cost of the product"

    def test_field_scope_in_schema_json(self):
        """Test scope appears in generated schema.json.

        RED: Verify schema generation includes scope metadata.
        """
        # Reset registry for clean test
        SchemaRegistry.clear()

        @fraiseql.type
        class Account:
            """Account with scoped fields."""

            id: ID
            accountNumber: str
            # Sensitive financial information
            balance: Annotated[float, fraiseql.field(requires_scope="read:Account.balance")]

        # Get schema directly from registry
        schema_dict = SchemaRegistry.get_schema()

        # Find Account type in schema
        account_type = None
        for type_def in schema_dict.get("types", []):
            if type_def.get("name") == "Account":
                account_type = type_def
                break

        assert account_type is not None, "Account type not found in schema"

        # Find balance field
        balance_field = None
        for field in account_type.get("fields", []):
            if field.get("name") == "balance":
                balance_field = field
                break

        assert balance_field is not None, "balance field not found"
        # RED: This assertion will fail until requires_scope is added to registry
        assert balance_field.get("requires_scope") == "read:Account.balance"


class TestFieldScopeWildcards:
    """Test wildcard scope patterns."""

    def test_read_all_wildcard(self):
        """Test read:* wildcard scope."""

        @fraiseql.type
        class Document:
            """Document with read-all scope."""

            id: ID
            # Any read scope is acceptable
            content: Annotated[str, fraiseql.field(requires_scope="read:*")]

        from fraiseql.types import extract_field_info

        field_info = extract_field_info(Document)
        assert field_info["content"]["requires_scope"] == "read:*"

    def test_type_wildcard(self):
        """Test read:User.* wildcard for all fields of a type."""

        @fraiseql.type
        class Profile:
            """Profile with type-level wildcard."""

            id: ID
            # Requires any read scope for Profile type
            data: Annotated[str, fraiseql.field(requires_scope="read:Profile.*")]

        from fraiseql.types import extract_field_info

        field_info = extract_field_info(Profile)
        assert field_info["data"]["requires_scope"] == "read:Profile.*"


class TestMixedScopedAndPublicFields:
    """Test types with both public and scoped fields."""

    def test_public_and_private_fields(self):
        """Test type with both public and access-controlled fields."""

        @fraiseql.type
        class MixedData:
            """Data with public and private fields."""

            id: ID  # Public
            name: str  # Public
            # Private fields requiring scopes
            internalNotes: Annotated[str, fraiseql.field(requires_scope="internal:view_notes")]
            budget: Annotated[float, fraiseql.field(requires_scope="finance:view_budget")]

        from fraiseql.types import extract_field_info

        field_info = extract_field_info(MixedData)

        # Public fields should not have scope
        assert "requires_scope" not in field_info["id"]
        assert "requires_scope" not in field_info["name"]

        # Private fields should have scope
        assert field_info["internalNotes"]["requires_scope"] == "internal:view_notes"
        assert field_info["budget"]["requires_scope"] == "finance:view_budget"


class TestFieldScopeEdgeCases:
    """Test edge cases for field scopes."""

    def test_empty_scope_treated_as_none(self):
        """Test that empty scope string is treated as no scope."""

        @fraiseql.type
        class TestType:
            """Test empty scope handling."""

            id: ID
            # Empty scope should not require any scope
            data: Annotated[str, fraiseql.field(requires_scope="")]

        from fraiseql.types import extract_field_info

        field_info = extract_field_info(TestType)

        # Empty scope should not appear in field_info
        assert "requires_scope" not in field_info["data"] or field_info["data"]["requires_scope"] == ""

    def test_scope_with_special_characters(self):
        """Test scope with allowed special characters."""

        @fraiseql.type
        class SpecialScopes:
            """Test special character scopes."""

            id: ID
            data1: Annotated[str, fraiseql.field(requires_scope="read:User.email_verified")]
            data2: Annotated[str, fraiseql.field(requires_scope="admin_read:system:config")]

        from fraiseql.types import extract_field_info

        field_info = extract_field_info(SpecialScopes)

        assert field_info["data1"]["requires_scope"] == "read:User.email_verified"
        assert field_info["data2"]["requires_scope"] == "admin_read:system:config"


# Placeholder for future cycles
class TestScopeInCompiledSchema:
    """Test that scopes are preserved in compiled schema (Cycle 4)."""

    @pytest.mark.skip(reason="Requires Cycle 4: Compiler Integration")
    def test_scope_in_compiled_schema(self):
        """Test scope appears in schema.compiled.json."""
        pass


class TestRuntimeScopeEnforcement:
    """Test runtime enforcement of field scopes (Cycle 5)."""

    @pytest.mark.skip(reason="Requires Cycle 5: Runtime Field Filtering")
    def test_runtime_filters_fields_by_scope(self):
        """Test executor filters fields based on user scopes."""
        pass
