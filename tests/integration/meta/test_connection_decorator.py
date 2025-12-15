"""Integration tests for @connection decorator with database execution."""

import pytest
from graphql import graphql

from fraiseql import fraise_type
from fraiseql.decorators import connection
from fraiseql.decorators import query as query_decorator
from fraiseql.gql.builders import SchemaRegistry


@fraise_type(sql_source="users")
class User:
    id: int
    name: str
    email: str | None = None


@pytest.fixture
def connection_schema():
    """Create schema with connection decorator."""
    registry = SchemaRegistry.get_instance()
    registry.clear()

    @query_decorator
    @connection(node_type=User)
    async def users_connection(info):
        """Get users connection."""
        return []

    registry.register_type(User)
    registry.register_query(users_connection)

    return registry.build_schema()


async def test_connection_schema_generation(connection_schema):
    """Connection decorator should generate proper GraphQL schema."""
    query_type = connection_schema.query_type
    assert query_type is not None

    # Verify field exists
    assert "usersConnection" in query_type.fields
    field = query_type.fields["usersConnection"]

    # Verify return type
    assert field.type.name == "UserConnection"

    # Verify pagination arguments
    assert "first" in field.args
    assert "after" in field.args
    assert "last" in field.args
    assert "before" in field.args
    assert "where" in field.args


async def test_connection_type_fields(connection_schema):
    """UserConnection type should have correct fields."""
    query_type = connection_schema.query_type
    field = query_type.fields["usersConnection"]
    connection_type = field.type

    # Verify Connection fields
    assert "edges" in connection_type.fields
    assert "pageInfo" in connection_type.fields
    assert "totalCount" in connection_type.fields


async def test_edge_type_fields(connection_schema):
    """UserEdge type should have node and cursor."""
    query_type = connection_schema.query_type
    field = query_type.fields["usersConnection"]
    connection_type = field.type

    # Get Edge type from edges field (unwrap GraphQLNonNull -> GraphQLList -> GraphQLNonNull -> GraphQLObjectType)
    edges_field = connection_type.fields["edges"]
    edge_type = edges_field.type.of_type.of_type.of_type  # Unwrap all levels

    assert edge_type.name == "UserEdge"
    assert "node" in edge_type.fields
    assert "cursor" in edge_type.fields


async def test_page_info_type_fields(connection_schema):
    """PageInfo type should have pagination fields."""
    query_type = connection_schema.query_type
    field = query_type.fields["usersConnection"]
    connection_type = field.type

    page_info_field = connection_type.fields["pageInfo"]
    page_info_type = page_info_field.type.of_type  # Unwrap NonNull

    assert page_info_type.name == "PageInfo"
    assert "hasNextPage" in page_info_type.fields
    assert "hasPreviousPage" in page_info_type.fields
    assert "startCursor" in page_info_type.fields
    assert "endCursor" in page_info_type.fields


async def test_connection_query_execution_without_db(connection_schema):
    """Connection query should execute (will fail without db, but schema is valid)."""
    query = """
    query {
        usersConnection(first: 10) {
            edges {
                node {
                    id
                    name
                }
                cursor
            }
            pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
            }
            totalCount
        }
    }
    """

    # This will fail at execution (no db in context)
    # But query syntax and schema should be valid
    result = await graphql(connection_schema, query)

    # Query is valid (no GraphQL schema errors)
    # Execution errors are expected (no db context)
    if result.errors:
        # Should be execution error (missing db), not schema error
        assert any(
            "Database repository not found" in str(e) or "'NoneType' object" in str(e)
            for e in result.errors
        ), f"Expected db error, got: {result.errors}"


async def test_connection_decorator_schema_has_args(connection_schema):
    """Connection decorator should generate schema with pagination arguments."""
    query_type = connection_schema.query_type
    field = query_type.fields["usersConnection"]

    # Verify pagination arguments exist in schema
    assert "first" in field.args
    assert "after" in field.args
    assert "last" in field.args
    assert "before" in field.args
    assert "where" in field.args

    print("✅ Schema has all pagination arguments")


async def test_multiple_connection_types_coexist(connection_schema):
    """Multiple Connection types (User, Post) should coexist."""

    # Create second type
    @fraise_type(sql_source="posts")
    class Post:
        id: int
        title: str

    registry = SchemaRegistry.get_instance()

    @query_decorator
    @connection(node_type=Post)
    async def posts_connection(info):
        return []

    registry.register_type(Post)
    registry.register_query(posts_connection)

    # Rebuild schema
    schema = registry.build_schema()
    query_type = schema.query_type

    # Verify both connections exist
    assert "usersConnection" in query_type.fields
    assert "postsConnection" in query_type.fields

    # Verify different return types
    users_type = query_type.fields["usersConnection"].type
    posts_type = query_type.fields["postsConnection"].type

    assert users_type.name == "UserConnection"
    assert posts_type.name == "PostConnection"

    # Verify they share same PageInfo type
    users_page_info = users_type.fields["pageInfo"].type
    posts_page_info = posts_type.fields["pageInfo"].type
    # Both should reference same PageInfo object
    assert users_page_info.of_type.name == "PageInfo"
    assert posts_page_info.of_type.name == "PageInfo"


async def test_connection_introspection(connection_schema):
    """GraphQL introspection should show Connection types."""
    introspection_query = """
    query {
        __type(name: "UserConnection") {
            name
            kind
            fields {
                name
                type {
                    name
                    kind
                }
            }
        }
    }
    """

    result = await graphql(connection_schema, introspection_query)
    assert not result.errors

    connection_type = result.data["__type"]
    assert connection_type["name"] == "UserConnection"
    assert connection_type["kind"] == "OBJECT"

    field_names = [f["name"] for f in connection_type["fields"]]
    assert "edges" in field_names
    assert "pageInfo" in field_names
    assert "totalCount" in field_names
