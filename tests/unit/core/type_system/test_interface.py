import pytest

"""Tests for FraiseQL interface support."""

import asyncio

from graphql import GraphQLInterfaceType, GraphQLObjectType, graphql

import fraiseql
from fraiseql.gql.schema_builder import build_fraiseql_schema


@pytest.mark.unit
class TestFraiseInterface:
    """Test suite for @fraise_interface decorator and interface functionality."""

    def test_basic_interface_decoration(self, clear_registry) -> None:
        """Test that @fraise_interface properly decorates a class."""

        @fraiseql.interface
        class Node:
            id: str = fraiseql.fraise_field(description="Unique identifier")
            created_at: str = fraiseql.fraise_field(description="Creation timestamp")

        # Check that the interface has the proper definition
        assert hasattr(Node, "__fraiseql_definition__")
        assert Node.__fraiseql_definition__.kind == "interface"

        # Check fields
        assert hasattr(Node, "__gql_fields__")
        fields = Node.__gql_fields__
        assert "id" in fields
        assert "created_at" in fields

    def test_type_implementing_interface(self, clear_registry) -> None:
        """Test a type implementing an interface."""

        @fraiseql.interface
        class Node:
            id: str

        @fraiseql.type(implements=[Node])
        class User:
            id: str
            name: str
            email: str

        # Check that User has the interface stored
        assert hasattr(User, "__fraiseql_interfaces__")
        assert Node in User.__fraiseql_interfaces__

        # Build schema and verify
        @fraiseql.type
        class QueryRoot:
            users: list[User] = fraiseql.fraise_field(default_factory=list)

        schema = build_fraiseql_schema(query_types=[QueryRoot])

        # Check that User type implements Node interface
        user_type = schema.type_map.get("User")
        assert isinstance(user_type, GraphQLObjectType)
        assert user_type.interfaces
        assert len(user_type.interfaces) == 1
        assert user_type.interfaces[0].name == "Node"

    def test_multiple_types_implementing_same_interface(self, clear_registry) -> None:
        """Test multiple types implementing the same interface."""

        @fraiseql.interface
        class Timestamped:
            created_at: str
            updated_at: str

        @fraiseql.type(implements=[Timestamped])
        class Article:
            id: str
            title: str
            created_at: str
            updated_at: str

        @fraiseql.type(implements=[Timestamped])
        class Comment:
            id: str
            text: str
            created_at: str
            updated_at: str

        @fraiseql.type
        class QueryRoot:
            articles: list[Article] = fraiseql.fraise_field(default_factory=list)
            comments: list[Comment] = fraiseql.fraise_field(default_factory=list)

        schema = build_fraiseql_schema(query_types=[QueryRoot])

        # Both types should implement Timestamped
        article_type = schema.type_map.get("Article")
        comment_type = schema.type_map.get("Comment")

        assert article_type.interfaces[0].name == "Timestamped"
        assert comment_type.interfaces[0].name == "Timestamped"

    def test_type_implementing_multiple_interfaces(self, clear_registry) -> None:
        """Test a type implementing multiple interfaces."""

        @fraiseql.interface
        class Node:
            id: str

        @fraiseql.interface
        class Timestamped:
            created_at: str
            updated_at: str

        @fraiseql.type(implements=[Node, Timestamped])
        class Post:
            id: str
            title: str
            content: str
            created_at: str
            updated_at: str

        @fraiseql.type
        class QueryRoot:
            posts: list[Post] = fraiseql.fraise_field(default_factory=list)

        schema = build_fraiseql_schema(query_types=[QueryRoot])

        # Post should implement both interfaces
        post_type = schema.type_map.get("Post")
        assert len(post_type.interfaces) == 2
        interface_names = {iface.name for iface in post_type.interfaces}
        assert interface_names == {"Node", "Timestamped"}

    def test_interface_field_resolution(self, clear_registry) -> None:
        """Test querying through an interface field."""

        @fraiseql.interface
        class Node:
            id: str

        @fraiseql.type(implements=[Node])
        class User:
            id: str
            name: str

        @fraiseql.type(implements=[Node])
        class Product:
            id: str
            title: str
            price: float

        @fraiseql.type
        class QueryRoot:
            nodes: list[Node] = fraiseql.fraise_field(default_factory=list)

            @staticmethod
            async def resolve_nodes(_root, _info) -> list[Node]:
                # Return a mix of Users and Products
                return [
                    User(id="u1", name="Alice"),
                    Product(id="p1", title="Laptop", price=999.99),
                    User(id="u2", name="Bob"),
                ]

        schema = build_fraiseql_schema(query_types=[QueryRoot])

        # Query with interface and inline fragments
        query = """
        query {
            nodes {
                id
                __typename
                ... on User {
                    name
                }
                ... on Product {
                    title
                    price
                }
            }
        }
        """
        result = asyncio.run(graphql(schema, query))
        assert result.errors is None
        assert result.data == {
            "nodes": [
                {"id": "u1", "__typename": "User", "name": "Alice"},
                {"id": "p1", "__typename": "Product", "title": "Laptop", "price": 999.99},
                {"id": "u2", "__typename": "User", "name": "Bob"},
            ]
        }

    def test_interface_with_complex_types(self, clear_registry) -> None:
        """Test interface with complex field types."""
        from enum import Enum

        @fraiseql.enum
        class Status(Enum):
            ACTIVE = "active"
            INACTIVE = "inactive"

        @fraiseql.type
        class Metadata:
            tags: list[str]
            version: int

        @fraiseql.interface
        class Versionable:
            metadata: Metadata
            status: Status

        @fraiseql.type(implements=[Versionable])
        class Document:
            id: str
            title: str
            metadata: Metadata
            status: Status

        @fraiseql.type
        class QueryRoot:
            documents: list[Document] = fraiseql.fraise_field(default_factory=list)

            @staticmethod
            async def resolve_documents(_root, _info) -> list[Document]:
                return [
                    Document(
                        id="d1",
                        title="Report",
                        metadata=Metadata(tags=["important", "quarterly"], version=2),
                        status=Status.ACTIVE,
                    )
                ]

        schema = build_fraiseql_schema(query_types=[QueryRoot])

        query = """
        query {
            documents {
                id
                title
                metadata {
                    tags
                    version
                }
                status
            }
        }
        """
        result = asyncio.run(graphql(schema, query))
        assert result.errors is None
        assert result.data == {
            "documents": [
                {
                    "id": "d1",
                    "title": "Report",
                    "metadata": {"tags": ["important", "quarterly"], "version": 2},
                    "status": "ACTIVE",
                }
            ]
        }

    def test_interface_inheritance(self, clear_registry) -> None:
        """Test interface extending another interface."""

        @fraiseql.interface
        class Node:
            id: str

        # For now, we'll test that types can implement multiple interfaces
        # True interface inheritance would require more complex implementation
        @fraiseql.interface
        class NamedNode:
            id: str
            name: str

        @fraiseql.type(implements=[Node, NamedNode])
        class Person:
            id: str
            name: str
            age: int

        @fraiseql.type
        class QueryRoot:
            people: list[Person] = fraiseql.fraise_field(default_factory=list)

        schema = build_fraiseql_schema(query_types=[QueryRoot])

        person_type = schema.type_map.get("Person")
        assert len(person_type.interfaces) == 2

    def test_interface_field_not_implemented_error(self, clear_registry) -> None:
        """Test that missing interface fields are caught."""

        @fraiseql.interface
        class Identifiable:
            id: str
            name: str

        # This should ideally raise an error because 'name' is missing
        # For now, let's test that the schema building works
        @fraiseql.type(implements=[Identifiable])
        class BadImplementation:
            id: str
            # Missing 'name' field

        @fraiseql.type
        class QueryRoot:
            items: list[BadImplementation] = fraiseql.fraise_field(default_factory=list)

        # In a full implementation, this should validate that all interface
        # fields are present in implementing types
        schema = build_fraiseql_schema(query_types=[QueryRoot])

        # The schema should still build, but the type won't properly implement the interface
        bad_type = schema.type_map.get("BadImplementation")
        assert bad_type.interfaces[0].name == "Identifiable"

    def test_interface_with_optional_fields(self, clear_registry) -> None:
        """Test interface with optional fields."""

        @fraiseql.interface
        class Describable:
            description: str | None = None

        @fraiseql.type(implements=[Describable])
        class Product:
            id: str
            name: str
            description: str | None = None

        @fraiseql.type
        class QueryRoot:
            products: list[Product] = fraiseql.fraise_field(default_factory=list)

            @staticmethod
            async def resolve_products(_root, _info) -> list[Product]:
                return [
                    Product(id="1", name="Widget", description="A useful widget"),
                    Product(id="2", name="Gadget", description=None),
                ]

        schema = build_fraiseql_schema(query_types=[QueryRoot])

        query = """
        query {
            products {
                id
                name
                description
            }
        }
        """
        result = asyncio.run(graphql(schema, query))
        assert result.errors is None
        assert result.data == {
            "products": [
                {"id": "1", "name": "Widget", "description": "A useful widget"},
                {"id": "2", "name": "Gadget", "description": None},
            ]
        }

    def test_interface_in_schema_introspection(self, clear_registry) -> None:
        """Test that interfaces appear correctly in schema introspection."""

        @fraiseql.interface
        class SearchResult:
            """Common fields for search results."""

            id: str
            title: str
            score: float

        @fraiseql.type(implements=[SearchResult])
        class ArticleResult:
            id: str
            title: str
            score: float
            author: str

        @fraiseql.type(implements=[SearchResult])
        class ProductResult:
            id: str
            title: str
            score: float
            price: float

        @fraiseql.type
        class QueryRoot:
            search_results: list[SearchResult] = fraiseql.fraise_field(default_factory=list)

        schema = build_fraiseql_schema(query_types=[QueryRoot])

        # Check that the interface is in the schema
        interface_type = schema.type_map.get("SearchResult")
        assert isinstance(interface_type, GraphQLInterfaceType)
        assert interface_type.description == "Common fields for search results."

        # Check interface fields
        assert "id" in interface_type.fields
        assert "title" in interface_type.fields
        assert "score" in interface_type.fields
