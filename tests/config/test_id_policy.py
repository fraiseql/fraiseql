"""Tests for ID Policy configuration.

FraiseQL provides configurable ID policy via SchemaConfig:

- IDPolicy.UUID (default): ID type uses custom IDScalar that enforces UUID format
- IDPolicy.OPAQUE: ID type uses GraphQL's built-in ID (accepts any string)

The policy affects only the ID type annotation. uuid.UUID always maps to UUIDScalar
regardless of policy.
"""

import uuid

import pytest
from graphql import GraphQLID

from fraiseql.config.schema_config import IDPolicy, SchemaConfig
from fraiseql.types import ID
from fraiseql.types.scalars import IDScalar, UUIDScalar
from fraiseql.types.scalars.graphql_utils import convert_scalar_to_graphql


@pytest.fixture(autouse=True)
def reset_config():
    """Reset SchemaConfig before and after each test."""
    SchemaConfig.reset()
    yield
    SchemaConfig.reset()


class TestIDPolicyEnum:
    """Tests for IDPolicy enum."""

    def test_default_policy_is_uuid(self):
        """Test that default ID policy is UUID enforcement."""
        config = SchemaConfig.get_instance()
        assert config.id_policy == IDPolicy.UUID

    def test_uuid_policy_enforces_uuid(self):
        """Test that UUID policy indicates UUID enforcement."""
        assert IDPolicy.UUID.enforces_uuid() is True

    def test_opaque_policy_does_not_enforce_uuid(self):
        """Test that OPAQUE policy does not enforce UUID."""
        assert IDPolicy.OPAQUE.enforces_uuid() is False

    def test_policy_values(self):
        """Test that policy enum has correct string values."""
        assert IDPolicy.UUID.value == "uuid"
        assert IDPolicy.OPAQUE.value == "opaque"


class TestIDPolicyConfiguration:
    """Tests for configuring ID policy."""

    def test_set_uuid_policy(self):
        """Test setting UUID policy explicitly."""
        SchemaConfig.set_config(id_policy=IDPolicy.UUID)
        assert SchemaConfig.get_instance().id_policy == IDPolicy.UUID

    def test_set_opaque_policy(self):
        """Test setting OPAQUE policy."""
        SchemaConfig.set_config(id_policy=IDPolicy.OPAQUE)
        assert SchemaConfig.get_instance().id_policy == IDPolicy.OPAQUE

    def test_reset_restores_default(self):
        """Test that reset restores default UUID policy."""
        SchemaConfig.set_config(id_policy=IDPolicy.OPAQUE)
        assert SchemaConfig.get_instance().id_policy == IDPolicy.OPAQUE

        SchemaConfig.reset()
        assert SchemaConfig.get_instance().id_policy == IDPolicy.UUID


class TestIDPolicyTypeMapping:
    """Tests for type mapping based on ID policy."""

    def test_uuid_policy_id_uses_id_scalar(self):
        """Test that ID maps to IDScalar with UUID policy."""
        SchemaConfig.set_config(id_policy=IDPolicy.UUID)

        result = convert_scalar_to_graphql(ID)
        assert result is IDScalar
        assert result.name == "ID"

    def test_opaque_policy_id_uses_graphql_id(self):
        """Test that ID maps to GraphQL's built-in ID with OPAQUE policy."""
        SchemaConfig.set_config(id_policy=IDPolicy.OPAQUE)

        result = convert_scalar_to_graphql(ID)
        assert result is GraphQLID
        assert result.name == "ID"

    def test_uuid_uuid_always_maps_to_uuid_scalar(self):
        """Test that uuid.UUID always maps to UUIDScalar regardless of policy."""
        for policy in IDPolicy:
            SchemaConfig.set_config(id_policy=policy)
            result = convert_scalar_to_graphql(uuid.UUID)
            assert result is UUIDScalar
            assert result.name == "UUID"

    def test_uuid_field_always_maps_to_uuid_scalar(self):
        """Test that UUIDField always maps to UUIDScalar regardless of policy."""
        from fraiseql.types.scalars.uuid import UUIDField

        for policy in IDPolicy:
            SchemaConfig.set_config(id_policy=policy)
            result = convert_scalar_to_graphql(UUIDField)
            assert result is UUIDScalar
            assert result.name == "UUID"


class TestIDPolicySchemaBuilding:
    """Tests for schema building with different ID policies."""

    @pytest.fixture(autouse=True)
    def clear_registry(self):
        """Clear the schema registry before and after each test."""
        from fraiseql.gql.builders.registry import SchemaRegistry

        registry = SchemaRegistry.get_instance()
        registry.clear()
        yield
        registry.clear()

    def test_schema_builds_with_uuid_policy(self):
        """Test that schema builds correctly with UUID policy."""
        import fraiseql

        SchemaConfig.set_config(id_policy=IDPolicy.UUID)

        @fraiseql.type
        class Entity:
            id: ID
            name: str

        async def entities(info) -> list[Entity]:
            return []

        schema = fraiseql.build_fraiseql_schema(query_types=[entities])

        # Schema should build without errors
        assert schema is not None

        # ID field should use ID scalar
        entity_type = schema.type_map.get("Entity")
        assert entity_type is not None
        id_field = entity_type.fields.get("id")
        assert id_field is not None
        assert id_field.type.name == "ID"

    def test_schema_builds_with_opaque_policy(self):
        """Test that schema builds correctly with OPAQUE policy."""
        import fraiseql

        SchemaConfig.set_config(id_policy=IDPolicy.OPAQUE)

        @fraiseql.type
        class Resource:
            id: ID
            value: str

        async def resources(info) -> list[Resource]:
            return []

        schema = fraiseql.build_fraiseql_schema(query_types=[resources])

        # Schema should build without errors
        assert schema is not None

        # ID field should still use ID type (but GraphQL's built-in)
        resource_type = schema.type_map.get("Resource")
        assert resource_type is not None
        id_field = resource_type.fields.get("id")
        assert id_field is not None
        assert id_field.type.name == "ID"


class TestIDPolicyDocumentation:
    """Tests documenting ID policy behavior for users."""

    def test_example_uuid_policy_usage(self):
        """Document how to use UUID policy (default)."""
        # UUID policy is the default - no configuration needed
        # IDs must be valid UUIDs

        config = SchemaConfig.get_instance()
        assert config.id_policy == IDPolicy.UUID
        assert config.id_policy.enforces_uuid() is True

        # ID type will use IDScalar which enforces UUID format
        result = convert_scalar_to_graphql(ID)
        assert result is IDScalar

    def test_example_opaque_policy_usage(self):
        """Document how to use OPAQUE policy for GraphQL spec compliance."""
        # Set OPAQUE policy when you need GraphQL spec-compliant IDs
        # that accept any string, not just UUIDs
        SchemaConfig.set_config(id_policy=IDPolicy.OPAQUE)

        config = SchemaConfig.get_instance()
        assert config.id_policy == IDPolicy.OPAQUE
        assert config.id_policy.enforces_uuid() is False

        # ID type will use GraphQL's built-in ID (accepts any string)
        result = convert_scalar_to_graphql(ID)
        assert result is GraphQLID

    def test_uuid_vs_id_semantic_difference(self):
        """Document the semantic difference between uuid.UUID and ID."""
        # uuid.UUID: A general-purpose UUID type
        # - Always maps to UUIDScalar (name="UUID")
        # - Use for: correlation IDs, external references, non-identifier UUIDs

        # ID: An identifier type
        # - Maps to IDScalar (UUID policy) or GraphQLID (OPAQUE policy)
        # - Use for: entity identifiers, primary keys

        # uuid.UUID always maps to UUID scalar
        uuid_result = convert_scalar_to_graphql(uuid.UUID)
        assert uuid_result.name == "UUID"

        # ID maps based on policy
        SchemaConfig.set_config(id_policy=IDPolicy.UUID)
        id_result_uuid = convert_scalar_to_graphql(ID)
        assert id_result_uuid.name == "ID"

        SchemaConfig.set_config(id_policy=IDPolicy.OPAQUE)
        id_result_opaque = convert_scalar_to_graphql(ID)
        assert id_result_opaque.name == "ID"
