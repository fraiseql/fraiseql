"""
Federation schema.json generation tests for Cycle 16-3 RED phase.

Tests schema.json includes proper federation metadata.
All tests expected to FAIL initially (RED phase).
"""

import json
import pytest

try:
    from fraiseql import type as fraiseql_type, ID
    from fraiseql.federation import key, extends, external, requires, provides
    from fraiseql.registry import generate_schema_json
except ImportError:
    pass


class TestSchemaJSONFederationRoot:
    """Test federation metadata at schema root level."""

    def test_schema_has_federation_root_enabled(self):
        """schema.json includes federation.enabled = true."""
        @fraiseql_type
        @key("id")
        class User:
            id: ID
            email: str

        schema_json = generate_schema_json([User])

        assert "federation" in schema_json
        assert schema_json["federation"]["enabled"] is True

    def test_schema_federation_version_v2(self):
        """schema.json declares federation version v2."""
        @fraiseql_type
        @key("id")
        class User:
            id: ID

        schema_json = generate_schema_json([User])

        assert schema_json["federation"]["version"] == "v2"

    def test_non_federation_schema_no_federation_root(self):
        """schema.json without @key has federation disabled."""
        @fraiseql_type
        class User:
            id: ID
            email: str

        schema_json = generate_schema_json([User])

        # Should still have federation root, but disabled
        if "federation" in schema_json:
            assert schema_json["federation"]["enabled"] is False


class TestSchemaJSONTypeKeys:
    """Test federation keys in type metadata."""

    def test_type_with_single_key(self):
        """Type with @key includes key in schema.json."""
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

    def test_type_with_composite_key(self):
        """Type with multiple @key decorators includes all keys."""
        @fraiseql_type
        @key("tenant_id")
        @key("id")
        class Account:
            tenant_id: ID
            id: ID
            name: str

        schema_json = generate_schema_json([Account])

        account_type = next((t for t in schema_json["types"] if t["name"] == "Account"), None)
        keys = account_type["federation"]["keys"]
        assert len(keys) == 2
        assert {"fields": ["tenant_id"]} in keys
        assert {"fields": ["id"]} in keys

    def test_type_without_key(self):
        """Type without @key has empty keys array."""
        @fraiseql_type
        class Product:
            id: ID
            name: str

        schema_json = generate_schema_json([Product])

        product_type = next((t for t in schema_json["types"] if t["name"] == "Product"), None)
        assert product_type["federation"]["keys"] == []


class TestSchemaJSONExtends:
    """Test federation extends metadata."""

    def test_extended_type_marked_in_schema(self):
        """Extended type has federation.extend = true."""
        @fraiseql_type
        @extends
        @key("id")
        class User:
            id: ID = external()
            email: str = external()

        schema_json = generate_schema_json([User])

        user_type = next((t for t in schema_json["types"] if t["name"] == "User"), None)
        assert user_type["federation"]["extend"] is True

    def test_regular_type_not_extended(self):
        """Regular type has federation.extend = false."""
        @fraiseql_type
        @key("id")
        class User:
            id: ID
            email: str

        schema_json = generate_schema_json([User])

        user_type = next((t for t in schema_json["types"] if t["name"] == "User"), None)
        assert user_type["federation"]["extend"] is False


class TestSchemaJSONExternalFields:
    """Test federation external fields metadata."""

    def test_external_fields_listed(self):
        """External fields marked in federation.external_fields."""
        @fraiseql_type
        @extends
        @key("id")
        class User:
            id: ID = external()
            email: str = external()
            orders: list["Order"]

        schema_json = generate_schema_json([User])

        user_type = next((t for t in schema_json["types"] if t["name"] == "User"), None)
        external_fields = user_type["federation"]["external_fields"]
        assert "id" in external_fields
        assert "email" in external_fields
        assert "orders" not in external_fields

    def test_field_level_external_flag(self):
        """External fields marked at field level."""
        @fraiseql_type
        @extends
        @key("id")
        class User:
            id: ID = external()
            email: str = external()
            orders: list["Order"]

        schema_json = generate_schema_json([User])

        user_type = next((t for t in schema_json["types"] if t["name"] == "User"), None)

        # Check id field
        id_field = next((f for f in user_type["fields"] if f["name"] == "id"), None)
        assert id_field["federation"]["external"] is True

        # Check email field
        email_field = next((f for f in user_type["fields"] if f["name"] == "email"), None)
        assert email_field["federation"]["external"] is True

        # Check orders field
        orders_field = next((f for f in user_type["fields"] if f["name"] == "orders"), None)
        assert orders_field["federation"]["external"] is False


class TestSchemaJSONFieldLevelFederation:
    """Test field-level federation metadata."""

    def test_field_requires_metadata(self):
        """Fields with @requires include requires in schema.json."""
        @fraiseql_type
        @extends
        @key("id")
        class User:
            id: ID = external()
            email: str = external()
            profile: str = requires("email")

        schema_json = generate_schema_json([User])

        user_type = next((t for t in schema_json["types"] if t["name"] == "User"), None)
        profile_field = next((f for f in user_type["fields"] if f["name"] == "profile"), None)

        assert "federation" in profile_field
        assert profile_field["federation"]["requires"] == "email"

    def test_field_provides_metadata(self):
        """Fields with @provides include provides in schema.json."""
        @fraiseql_type
        @key("id")
        class User:
            id: ID
            email: str
            owner_profile: str = provides("Order.owner_email")

        schema_json = generate_schema_json([User])

        user_type = next((t for t in schema_json["types"] if t["name"] == "User"), None)
        profile_field = next((f for f in user_type["fields"] if f["name"] == "owner_profile"), None)

        assert "federation" in profile_field
        assert profile_field["federation"]["provides"] == ["Order.owner_email"]

    def test_field_without_federation_metadata(self):
        """Regular fields include federation object with defaults."""
        @fraiseql_type
        @key("id")
        class User:
            id: ID
            email: str

        schema_json = generate_schema_json([User])

        user_type = next((t for t in schema_json["types"] if t["name"] == "User"), None)
        email_field = next((f for f in user_type["fields"] if f["name"] == "email"), None)

        # Should have federation object with defaults
        assert "federation" in email_field
        assert email_field["federation"]["external"] is False
        assert email_field["federation"].get("requires") is None
        assert email_field["federation"].get("provides") is None


class TestSchemaJSONMultipleTypes:
    """Test schema.json with multiple federation types."""

    def test_multiple_federation_types(self):
        """schema.json includes all federation types."""
        @fraiseql_type
        @key("id")
        class User:
            id: ID
            email: str

        @fraiseql_type
        @key("id")
        class Order:
            id: ID
            user_id: ID
            total: float

        schema_json = generate_schema_json([User, Order])

        user_type = next((t for t in schema_json["types"] if t["name"] == "User"), None)
        order_type = next((t for t in schema_json["types"] if t["name"] == "Order"), None)

        assert user_type["federation"]["keys"] == [{"fields": ["id"]}]
        assert order_type["federation"]["keys"] == [{"fields": ["id"]}]

    def test_mixed_federation_and_local_types(self):
        """schema.json handles mix of federation and non-federation types."""
        @fraiseql_type
        @key("id")
        class User:
            id: ID

        @fraiseql_type
        class Product:
            id: ID
            name: str

        schema_json = generate_schema_json([User, Product])

        user_type = next((t for t in schema_json["types"] if t["name"] == "User"), None)
        product_type = next((t for t in schema_json["types"] if t["name"] == "Product"), None)

        assert user_type["federation"]["keys"] == [{"fields": ["id"]}]
        assert product_type["federation"]["keys"] == []


class TestSchemaJSONValidStructure:
    """Test schema.json structure is valid JSON and well-formed."""

    def test_schema_json_is_valid_json(self):
        """Generated schema.json is valid JSON."""
        @fraiseql_type
        @key("id")
        class User:
            id: ID
            email: str

        schema_json = generate_schema_json([User])

        # Should be serializable
        json_str = json.dumps(schema_json)
        assert json_str is not None

        # Should deserialize back
        parsed = json.loads(json_str)
        assert parsed["federation"]["enabled"] is True

    def test_schema_json_federation_schema_matches_spec(self):
        """schema.json federation metadata matches Apollo spec."""
        @fraiseql_type
        @extends
        @key("id")
        class User:
            id: ID = external()
            email: str = external()
            orders: list["Order"] = requires("email")

        schema_json = generate_schema_json([User])

        user_type = next((t for t in schema_json["types"] if t["name"] == "User"), None)
        fed = user_type["federation"]

        # Required fields per spec
        assert "keys" in fed
        assert "extend" in fed
        assert "external_fields" in fed

        # Optional but important
        assert isinstance(fed["keys"], list)
        assert isinstance(fed["external_fields"], list)
        assert isinstance(fed["extend"], bool)
