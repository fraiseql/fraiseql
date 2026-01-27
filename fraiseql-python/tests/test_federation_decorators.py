"""
Federation decorator tests for Cycle 16-3 RED phase.

Tests federation decorators: @key, @extends, @external, @requires, @provides.
All tests are expected to FAIL initially (RED phase).
"""

import pytest

# These imports will fail until we implement the federation decorators
try:
    from fraiseql import type as fraiseql_type, ID
    from fraiseql.federation import key, extends, external, requires, provides
except ImportError:
    pass


class TestKeyDecorator:
    """Test @key decorator for federation primary keys."""

    def test_key_single_field(self):
        """@key("id") marks type as having federation key."""
        @fraiseql_type
        @key("id")
        class User:
            id: ID
            email: str

        # Assert: metadata includes key
        assert hasattr(User, "__fraiseql_federation__")
        assert User.__fraiseql_federation__["keys"] == [{"fields": ["id"]}]

    def test_key_multiple_fields(self):
        """Multiple @key decorators for composite keys."""
        @fraiseql_type
        @key("tenant_id")
        @key("id")
        class Account:
            tenant_id: ID
            id: ID
            name: str

        # Assert: both keys present
        keys = Account.__fraiseql_federation__["keys"]
        assert len(keys) == 2
        assert {"fields": ["tenant_id"]} in keys
        assert {"fields": ["id"]} in keys

    def test_key_nonexistent_field(self):
        """@key with non-existent field raises error."""
        with pytest.raises(ValueError, match="Field 'nonexistent' not found"):
            @fraiseql_type
            @key("nonexistent")
            class User:
                id: ID
                email: str

    def test_key_on_simple_type_fails(self):
        """@key requires @type decorator."""
        with pytest.raises((AttributeError, TypeError)):
            @key("id")
            class User:
                id: ID


class TestExtendsDecorator:
    """Test @extends decorator for extending types from other subgraphs."""

    def test_extends_marks_type(self):
        """@extends marks type as extended."""
        @fraiseql_type
        @extends
        @key("id")
        class User:
            id: ID = external()
            email: str = external()

        assert User.__fraiseql_federation__["extend"] is True

    def test_extends_without_key_fails(self):
        """@extends requires @key decorator."""
        with pytest.raises((ValueError, TypeError)):
            @fraiseql_type
            @extends
            class User:
                id: ID = external()

    def test_external_field_in_extends(self):
        """@external() marks field as external."""
        @fraiseql_type
        @extends
        @key("id")
        class User:
            id: ID = external()
            email: str = external()
            orders: list["Order"]  # Regular field, not external

        ext_fields = User.__fraiseql_federation__["external_fields"]
        assert "id" in ext_fields
        assert "email" in ext_fields
        assert "orders" not in ext_fields

    def test_external_on_non_extended_type_fails(self):
        """@external on non-extended type raises error."""
        with pytest.raises(ValueError, match="@external can only be used with @extends"):
            @fraiseql_type
            @key("id")
            class User:
                id: ID = external()
                email: str


class TestRequiresDecorator:
    """Test @requires decorator for field dependencies."""

    def test_requires_marks_dependency(self):
        """@requires("field") marks field as needing data resolution."""
        @fraiseql_type
        @extends
        @key("id")
        class User:
            id: ID = external()
            email: str = external()
            profile: str = requires("email")

        requires_dict = User.__fraiseql_federation__["requires"]
        assert requires_dict["profile"] == "email"

    def test_requires_nonexistent_field_fails(self):
        """@requires("nonexistent") raises error."""
        with pytest.raises(ValueError, match="Field 'nonexistent' not found"):
            @fraiseql_type
            @extends
            @key("id")
            class User:
                id: ID = external()
                profile: str = requires("nonexistent")

    def test_requires_multiple_dependencies(self):
        """Multiple @requires dependencies tracked."""
        @fraiseql_type
        @extends
        @key("id")
        class User:
            id: ID = external()
            email: str = external()
            phone: str = external()
            profile: str = requires("email")
            contact_info: str = requires("phone")

        requires_dict = User.__fraiseql_federation__["requires"]
        assert requires_dict["profile"] == "email"
        assert requires_dict["contact_info"] == "phone"

    def test_requires_on_non_extended_type(self):
        """@requires on locally-owned type works (provides resolution info)."""
        @fraiseql_type
        @key("id")
        class Order:
            id: ID
            user_id: ID
            user: str = requires("user_id")  # Requires own field

        requires_dict = Order.__fraiseql_federation__["requires"]
        assert requires_dict["user"] == "user_id"


class TestProvidesDecorator:
    """Test @provides decorator for field data provision."""

    def test_provides_marks_data_provider(self):
        """@provides marks field as providing data for other subgraph."""
        @fraiseql_type
        @key("id")
        class User:
            id: ID
            email: str
            owner_profile: str = provides("Order.owner_email")

        provides_data = User.__fraiseql_federation__["provides_data"]
        assert "Order.owner_email" in provides_data

    def test_provides_multiple_targets(self):
        """Multiple @provides decorators for same field."""
        @fraiseql_type
        @key("id")
        class User:
            id: ID
            email: str
            name: str = provides("Order.owner_name", "Invoice.owner_name")

        provides_data = User.__fraiseql_federation__["provides_data"]
        assert "Order.owner_name" in provides_data
        assert "Invoice.owner_name" in provides_data


class TestSchemaJSONGeneration:
    """Test schema.json generation with federation metadata."""

    def test_schema_json_includes_federation_metadata(self):
        """schema.json includes federation root metadata."""
        from fraiseql.registry import generate_schema_json

        @fraiseql_type
        @key("id")
        class User:
            id: ID
            email: str

        schema_json = generate_schema_json([User])

        assert "federation" in schema_json
        assert schema_json["federation"]["enabled"] is True
        assert schema_json["federation"]["version"] == "v2"

    def test_schema_json_type_federation_metadata(self):
        """schema.json includes per-type federation metadata."""
        from fraiseql.registry import generate_schema_json

        @fraiseql_type
        @key("id")
        class User:
            id: ID
            email: str

        schema_json = generate_schema_json([User])

        user_type = next((t for t in schema_json["types"] if t["name"] == "User"), None)
        assert user_type is not None
        assert "federation" in user_type
        assert user_type["federation"]["keys"] == [{"fields": ["id"]}]
        assert user_type["federation"]["extend"] is False

    def test_schema_json_extends_metadata(self):
        """schema.json marks extended types and external fields."""
        from fraiseql.registry import generate_schema_json

        @fraiseql_type
        @extends
        @key("id")
        class User:
            id: ID = external()
            email: str = external()
            orders: list["Order"]

        schema_json = generate_schema_json([User])

        user_type = next((t for t in schema_json["types"] if t["name"] == "User"), None)
        assert user_type["federation"]["extend"] is True
        assert "id" in user_type["federation"]["external_fields"]
        assert "email" in user_type["federation"]["external_fields"]

    def test_schema_json_field_level_federation(self):
        """schema.json includes field-level federation metadata."""
        from fraiseql.registry import generate_schema_json

        @fraiseql_type
        @extends
        @key("id")
        class User:
            id: ID = external()
            email: str
            profile: str = requires("email")

        schema_json = generate_schema_json([User])

        user_type = next((t for t in schema_json["types"] if t["name"] == "User"), None)

        # Check external field
        id_field = next((f for f in user_type["fields"] if f["name"] == "id"), None)
        assert id_field["federation"]["external"] is True

        # Check requires field
        profile_field = next((f for f in user_type["fields"] if f["name"] == "profile"), None)
        assert profile_field["federation"]["requires"] == "email"


class TestCompileTimeValidation:
    """Test compile-time validation of federation schemas."""

    def test_invalid_key_field_rejected(self):
        """Invalid key field rejected at compile time."""
        with pytest.raises(ValueError, match="Field 'nonexistent' not found"):
            @fraiseql_type
            @key("nonexistent")
            class User:
                id: ID
                email: str

    def test_external_without_extends_rejected(self):
        """External field without @extends rejected."""
        with pytest.raises(ValueError, match="@external requires @extends"):
            @fraiseql_type
            @key("id")
            class User:
                id: ID = external()
                email: str

    def test_requires_nonexistent_field_rejected(self):
        """@requires with non-existent field rejected."""
        with pytest.raises(ValueError, match="Field 'nonexistent' not found"):
            @fraiseql_type
            @extends
            @key("id")
            class User:
                id: ID = external()
                profile: str = requires("nonexistent")

    def test_multiple_keys_same_field_rejected(self):
        """Multiple @key decorators with same field rejected."""
        with pytest.raises(ValueError, match="Duplicate key field"):
            @fraiseql_type
            @key("id")
            @key("id")
            class User:
                id: ID
                email: str


class TestFieldValidation:
    """Test field-level validation across federation decorators."""

    def test_external_field_must_exist(self):
        """@external can only mark existing fields."""
        with pytest.raises(ValueError, match="Field 'nonexistent' not found"):
            @fraiseql_type
            @extends
            @key("id")
            class User:
                id: ID
                email: str

                nonexistent: str = external()

    def test_requires_field_dependency_chain(self):
        """Field chain for @requires must be valid."""
        @fraiseql_type
        @key("id")
        class Order:
            id: ID
            user_id: ID
            user: str = requires("user_id")

        requires_dict = Order.__fraiseql_federation__["requires"]
        assert requires_dict["user"] == "user_id"

    def test_mixed_external_and_regular_fields(self):
        """Extended type can mix external and regular fields."""
        @fraiseql_type
        @extends
        @key("id")
        class User:
            id: ID = external()
            email: str = external()
            orders: list["Order"]  # Regular field
            purchases: list["Purchase"]  # Another regular field

        ext_fields = User.__fraiseql_federation__["external_fields"]
        assert len(ext_fields) == 2
        assert "id" in ext_fields
        assert "email" in ext_fields
        assert "orders" not in ext_fields
        assert "purchases" not in ext_fields
