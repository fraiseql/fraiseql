"""Tests for GraphQL test client utilities.

This test client mimics real GraphQL client behavior by:
1. Executing queries against the GraphQL schema
2. Deserializing RustResponseBytes into typed objects
3. Providing type-safe response objects
"""

from dataclasses import dataclass

import pytest

# We'll import these once implemented
# from tests.utils.graphql_test_client import TypedGraphQLResponse, GraphQLTestClient


@dataclass
class Product:
    """Sample product type for testing."""

    id: int
    name: str
    price: float


# TDD Cycle 4.1: Basic TypedGraphQLResponse
def test_typed_graphql_response_creation() -> None:
    """Test TypedGraphQLResponse can be created with data and errors."""
    from tests.utils.graphql_test_client import TypedGraphQLResponse

    # Test successful response
    product = Product(id=1, name="Test Product", price=99.99)
    response = TypedGraphQLResponse(data=product, errors=None)

    assert response.data == product
    assert response.errors is None
    assert response.ok is True

    # Test error response
    errors = [{"message": "Something went wrong"}]
    response_with_errors = TypedGraphQLResponse(data=None, errors=errors)

    assert response_with_errors.data is None
    assert response_with_errors.errors == errors
    assert response_with_errors.ok is False


def test_typed_graphql_response_with_list_data() -> None:
    """Test TypedGraphQLResponse handles list data correctly."""
    from tests.utils.graphql_test_client import TypedGraphQLResponse

    products = [
        Product(id=1, name="Product 1", price=10.0),
        Product(id=2, name="Product 2", price=20.0),
    ]
    response = TypedGraphQLResponse(data=products, errors=None)

    assert response.data == products
    assert len(response.data) == 2
    assert response.ok is True


def test_typed_graphql_response_partial_success() -> None:
    """Test TypedGraphQLResponse with both data and errors (partial success)."""
    from tests.utils.graphql_test_client import TypedGraphQLResponse

    products = [Product(id=1, name="Product 1", price=10.0)]
    errors = [{"message": "Some field failed to resolve"}]

    response = TypedGraphQLResponse(data=products, errors=errors)

    assert response.data == products
    assert response.errors == errors
    # Partial success: has data but also has errors
    assert response.ok is True  # Still ok because we have data


# TDD Cycle 4.2: GraphQLTestClient Query Execution
@pytest.mark.asyncio
async def test_graphql_test_client_executes_query() -> None:
    """Test GraphQLTestClient can execute a basic query."""
    from graphql import GraphQLField, GraphQLObjectType, GraphQLSchema, GraphQLString
    from tests.utils.graphql_test_client import GraphQLTestClient

    # Create a simple schema
    schema = GraphQLSchema(
        query=GraphQLObjectType(
            "Query", {"hello": GraphQLField(GraphQLString, resolve=lambda obj, info: "world")}
        )
    )

    # Create test client
    client = GraphQLTestClient(schema)

    # Execute query
    response = await client.query("{ hello }", result_type=dict)

    assert response.ok is True
    assert response.data == {"hello": "world"}
    assert response.errors is None


@pytest.mark.asyncio
async def test_graphql_test_client_handles_rustresponsebytes() -> None:
    """Test GraphQLTestClient handles RustResponseBytes return from execute_graphql."""
    import json

    from graphql import (
        GraphQLField,
        GraphQLFloat,
        GraphQLInt,
        GraphQLList,
        GraphQLObjectType,
        GraphQLSchema,
        GraphQLString,
    )
    from tests.utils.graphql_test_client import GraphQLTestClient

    from fraiseql.core.rust_pipeline import RustResponseBytes

    # Create a schema that returns list of products
    schema = GraphQLSchema(
        query=GraphQLObjectType(
            "Query",
            {
                "products": GraphQLField(
                    GraphQLList(
                        GraphQLObjectType(
                            "Product",
                            {
                                "id": GraphQLField(GraphQLInt),
                                "name": GraphQLField(GraphQLString),
                                "price": GraphQLField(GraphQLFloat),
                            },
                        )
                    ),
                    resolve=lambda obj, info: RustResponseBytes(
                        json.dumps(
                            [
                                {"id": 1, "name": "Product 1", "price": 10.0},
                                {"id": 2, "name": "Product 2", "price": 20.0},
                            ]
                        ).encode("utf-8")
                    ),
                )
            },
        )
    )

    client = GraphQLTestClient(schema)
    response = await client.query("{ products { id name price } }", result_type=list[Product])

    assert response.ok is True
    assert len(response.data) == 2
    assert isinstance(response.data[0], Product)
    assert response.data[0].id == 1
    assert response.data[0].name == "Product 1"


@pytest.mark.asyncio
async def test_graphql_test_client_handles_errors() -> None:
    """Test GraphQLTestClient handles GraphQL errors correctly."""
    from graphql import GraphQLField, GraphQLObjectType, GraphQLSchema, GraphQLString
    from tests.utils.graphql_test_client import GraphQLTestClient

    # Create schema with a field that raises an error
    def error_resolver(obj, info) -> None:
        raise Exception("Something went wrong")

    schema = GraphQLSchema(
        query=GraphQLObjectType(
            "Query", {"failing": GraphQLField(GraphQLString, resolve=error_resolver)}
        )
    )

    client = GraphQLTestClient(schema)
    response = await client.query("{ failing }", result_type=dict)

    # GraphQL returns {'failing': None} for failed fields (partial success)
    assert response.ok is True  # Has data (even if field is None)
    assert response.data == {"failing": None}
    assert response.errors is not None
    assert len(response.errors) > 0
    assert "Something went wrong" in str(response.errors)


# TDD Cycle 4.3: Type Deserialization
@pytest.mark.asyncio
async def test_graphql_test_client_deserializes_nested_types() -> None:
    """Test GraphQLTestClient handles nested dataclass types."""
    from graphql import (
        GraphQLField,
        GraphQLFloat,
        GraphQLInt,
        GraphQLObjectType,
        GraphQLSchema,
        GraphQLString,
    )
    from tests.utils.graphql_test_client import GraphQLTestClient

    @dataclass
    class Category:
        id: int
        name: str

    @dataclass
    class ProductWithCategory:
        id: int
        name: str
        price: float
        category: Category

    # Create schema with nested types
    schema = GraphQLSchema(
        query=GraphQLObjectType(
            "Query",
            {
                "product": GraphQLField(
                    GraphQLObjectType(
                        "Product",
                        {
                            "id": GraphQLField(GraphQLInt),
                            "name": GraphQLField(GraphQLString),
                            "price": GraphQLField(GraphQLFloat),
                            "category": GraphQLField(
                                GraphQLObjectType(
                                    "Category",
                                    {
                                        "id": GraphQLField(GraphQLInt),
                                        "name": GraphQLField(GraphQLString),
                                    },
                                )
                            ),
                        },
                    ),
                    resolve=lambda obj, info: {
                        "id": 1,
                        "name": "Product 1",
                        "price": 99.99,
                        "category": {"id": 10, "name": "Electronics"},
                    },
                )
            },
        )
    )

    client = GraphQLTestClient(schema)
    # Result type should match the GraphQL response shape: {"product": {...}}
    response = await client.query(
        "{ product { id name price category { id name } } }", result_type=dict
    )

    assert response.ok is True
    assert isinstance(response.data, dict)
    # GraphQL returns nested dict structure
    assert "product" in response.data
    product = response.data["product"]
    assert product["id"] == 1
    assert product["name"] == "Product 1"
    assert product["category"]["id"] == 10
    assert product["category"]["name"] == "Electronics"


@pytest.mark.asyncio
async def test_graphql_test_client_handles_null_values() -> None:
    """Test GraphQLTestClient handles null values in responses."""
    from graphql import GraphQLField, GraphQLInt, GraphQLObjectType, GraphQLSchema, GraphQLString
    from tests.utils.graphql_test_client import GraphQLTestClient

    schema = GraphQLSchema(
        query=GraphQLObjectType(
            "Query",
            {
                "product": GraphQLField(
                    GraphQLObjectType(
                        "Product",
                        {
                            "id": GraphQLField(GraphQLInt),
                            "name": GraphQLField(GraphQLString),
                            "description": GraphQLField(GraphQLString),
                        },
                    ),
                    resolve=lambda obj, info: {"id": 1, "name": "Product 1", "description": None},
                )
            },
        )
    )

    client = GraphQLTestClient(schema)
    response = await client.query("{ product { id name description } }", result_type=dict)

    assert response.ok is True
    product = response.data["product"]
    assert product["id"] == 1
    assert product["name"] == "Product 1"
    assert product["description"] is None


@pytest.mark.asyncio
async def test_graphql_test_client_handles_empty_lists() -> None:
    """Test GraphQLTestClient handles empty list responses."""
    from graphql import (
        GraphQLField,
        GraphQLFloat,
        GraphQLInt,
        GraphQLList,
        GraphQLObjectType,
        GraphQLSchema,
        GraphQLString,
    )
    from tests.utils.graphql_test_client import GraphQLTestClient

    schema = GraphQLSchema(
        query=GraphQLObjectType(
            "Query",
            {
                "products": GraphQLField(
                    GraphQLList(
                        GraphQLObjectType(
                            "Product",
                            {
                                "id": GraphQLField(GraphQLInt),
                                "name": GraphQLField(GraphQLString),
                                "price": GraphQLField(GraphQLFloat),
                            },
                        )
                    ),
                    resolve=lambda obj, info: [],
                )
            },
        )
    )

    client = GraphQLTestClient(schema)
    response = await client.query("{ products { id name price } }", result_type=dict)

    assert response.ok is True
    assert response.data == {"products": []}
    assert len(response.data["products"]) == 0
