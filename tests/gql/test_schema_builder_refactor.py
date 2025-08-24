"""Tests for the refactored schema builder modules."""

import pytest
from graphql import GraphQLObjectType, GraphQLSchema

from fraiseql.gql.builders.mutation_builder import MutationTypeBuilder
from fraiseql.gql.builders.query_builder import QueryTypeBuilder
from fraiseql.gql.builders.registry import SchemaRegistry
from fraiseql.gql.builders.schema_composer import SchemaComposer



@pytest.mark.unit
class TestSchemaRegistry:
    """Test the SchemaRegistry functionality."""

    def test_singleton_pattern(self):
        """Test that SchemaRegistry follows singleton pattern."""
        registry1 = SchemaRegistry.get_instance()
        registry2 = SchemaRegistry.get_instance()
        assert registry1 is registry2

    def test_register_and_retrieve_type(self):
        """Test registering and retrieving types."""
        registry = SchemaRegistry.get_instance()
        registry.clear()

        class TestType:
            pass

        registry.register_type(TestType)
        assert TestType in registry.types
        assert registry.types[TestType] == TestType

    def test_register_query(self):
        """Test registering query functions."""
        registry = SchemaRegistry.get_instance()
        registry.clear()

        def test_query(info) -> str:
            return "test"

        registry.register_query(test_query)
        assert "test_query" in registry.queries
        assert registry.queries["test_query"] == test_query

    def test_clear_registry(self):
        """Test that clear(): removes all registered items."""
        registry = SchemaRegistry.get_instance()

        class TestType:
            pass

        def test_query(info) -> str:
            return "test"

        registry.register_type(TestType)
        registry.register_query(test_query)

        registry.clear()

        assert len(registry.types) == 0
        assert len(registry.queries) == 0
        assert len(registry.mutations) == 0


class TestQueryTypeBuilder:
    """Test the QueryTypeBuilder functionality."""

    def test_build_empty_query_type_raises_error(self):
        """Test that building without any fields raises TypeError."""
        registry = SchemaRegistry.get_instance()
        registry.clear()

        builder = QueryTypeBuilder(registry)

        with pytest.raises(TypeError, match="Type Query must define one or more fields"):
            builder.build()

    def test_build_query_type_with_function(self):
        """Test building query type with registered query functions."""
        registry = SchemaRegistry.get_instance()
        registry.clear()

        def hello(info) -> str:
            return "world"

        registry.register_query(hello)

        builder = QueryTypeBuilder(registry)
        query_type = builder.build()

        assert isinstance(query_type, GraphQLObjectType)
        assert query_type.name == "Query"
        assert "hello" in query_type.fields


class TestMutationTypeBuilder:
    """Test the MutationTypeBuilder functionality."""

    def test_build_mutation_type(self):
        """Test building mutation type."""
        registry = SchemaRegistry.get_instance()
        registry.clear()

        def create_user(info, name: str) -> str:
            return f"Created {name}"

        registry.register_mutation(create_user)

        builder = MutationTypeBuilder(registry)
        mutation_type = builder.build()

        assert isinstance(mutation_type, GraphQLObjectType)
        assert mutation_type.name == "Mutation"
        # Field name is converted to camelCase by default
        assert "createUser" in mutation_type.fields


class TestSchemaComposer:
    """Test the SchemaComposer functionality."""

    def test_compose_schema_query_only(self):
        """Test composing schema with only queries."""
        registry = SchemaRegistry.get_instance()
        registry.clear()

        def hello(info) -> str:
            return "world"

        registry.register_query(hello)

        composer = SchemaComposer(registry)
        schema = composer.compose()

        assert isinstance(schema, GraphQLSchema)
        assert schema.query_type is not None
        assert schema.mutation_type is None
        assert schema.subscription_type is None

    def test_compose_schema_with_mutations(self):
        """Test composing schema with queries and mutations."""
        registry = SchemaRegistry.get_instance()
        registry.clear()

        def hello(info) -> str:
            return "world"

        def create_item(info, name: str) -> str:
            return f"Created {name}"

        registry.register_query(hello)
        registry.register_mutation(create_item)

        composer = SchemaComposer(registry)
        schema = composer.compose()

        assert isinstance(schema, GraphQLSchema)
        assert schema.query_type is not None
        assert schema.mutation_type is not None
        # Field name is converted to camelCase by default
        assert "createItem" in schema.mutation_type.fields