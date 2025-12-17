"""Test execute_multi_field_query() function (Phase 2).

This test verifies the Python implementation that extracts fields from queries
and executes resolvers for multi-field GraphQL responses.
"""

import json

import pytest
from graphql import GraphQLField, GraphQLList, GraphQLObjectType, GraphQLSchema, GraphQLString

from fraiseql.core.rust_pipeline import RustResponseBytes
from fraiseql.fastapi.routers import _extract_root_query_fields, execute_multi_field_query


def test_extract_root_query_fields_two_fields():
    """Test extracting two root fields from a query."""
    query = """
    {
        dnsServers {
            id
            ipAddress
        }
        gateways {
            id
            hostname
        }
    }
    """

    fields = _extract_root_query_fields(query)

    assert len(fields) == 2

    # First field
    assert fields[0]["field_name"] == "dnsServers"
    assert len(fields[0]["selections"]) == 2
    assert fields[0]["selections"][0] == {"field_name": "id", "alias": None}
    assert fields[0]["selections"][1] == {"field_name": "ipAddress", "alias": None}

    # Second field
    assert fields[1]["field_name"] == "gateways"
    assert len(fields[1]["selections"]) == 2
    assert fields[1]["selections"][0] == {"field_name": "id", "alias": None}
    assert fields[1]["selections"][1] == {"field_name": "hostname", "alias": None}


def test_extract_root_query_fields_single_field():
    """Test extracting a single root field."""
    query = """
    {
        users {
            id
            userName
        }
    }
    """

    fields = _extract_root_query_fields(query)

    assert len(fields) == 1
    assert fields[0]["field_name"] == "users"
    assert len(fields[0]["selections"]) == 2
    assert fields[0]["selections"][0] == {"field_name": "id", "alias": None}
    assert fields[0]["selections"][1] == {"field_name": "userName", "alias": None}


def test_extract_root_query_fields_no_selections():
    """Test extracting fields with no sub-selections."""
    query = """
    {
        count
        total
    }
    """

    fields = _extract_root_query_fields(query)

    assert len(fields) == 2
    assert fields[0]["field_name"] == "count"
    assert fields[0]["selections"] == []
    assert fields[1]["field_name"] == "total"
    assert fields[1]["selections"] == []


def test_extract_root_query_fields_skips_introspection():
    """Test that introspection fields (__schema, __type) are skipped."""
    query = """
    {
        __schema {
            types {
                name
            }
        }
        users {
            id
        }
    }
    """

    fields = _extract_root_query_fields(query)

    # Should only have 'users', not '__schema'
    assert len(fields) == 1
    assert fields[0]["field_name"] == "users"


def test_extract_root_query_fields_with_aliases():
    """Test extracting fields with aliases."""
    query = """
    {
        allUsers: users {
            userId: id
            fullName: name
            email
        }
        allPosts: posts {
            postId: id
            title
        }
    }
    """

    fields = _extract_root_query_fields(query)

    assert len(fields) == 2

    # First field (users with alias allUsers)
    assert fields[0]["field_name"] == "users"  # Actual field name for resolver lookup
    assert fields[0]["response_key"] == "allUsers"  # Alias for response
    assert len(fields[0]["selections"]) == 3
    # Field selections use field_name, not materialized_path (conversion happens later)
    assert fields[0]["selections"][0] == {"field_name": "id", "alias": "userId"}
    assert fields[0]["selections"][1] == {"field_name": "name", "alias": "fullName"}
    assert fields[0]["selections"][2] == {"field_name": "email", "alias": None}

    # Second field (posts with alias allPosts)
    assert fields[1]["field_name"] == "posts"  # Actual field name for resolver lookup
    assert fields[1]["response_key"] == "allPosts"  # Alias for response
    assert len(fields[1]["selections"]) == 2
    assert fields[1]["selections"][0] == {"field_name": "id", "alias": "postId"}
    assert fields[1]["selections"][1] == {"field_name": "title", "alias": None}


@pytest.mark.asyncio
async def test_execute_multi_field_query_basic(init_schema_registry_fixture):
    """Test basic multi-field query execution.

    This test uses mock resolvers that return simple data.
    """

    # Create mock resolvers
    async def dns_servers_resolver(info):
        """Mock resolver for dnsServers field."""
        return [
            {"id": 1, "ip_address": "8.8.8.8"},
            {"id": 2, "ip_address": "1.1.1.1"},
        ]

    async def gateways_resolver(info):
        """Mock resolver for gateways field."""
        return [
            {"id": 10, "hostname": "gateway1"},
        ]

    # Build minimal schema
    dns_server_type = GraphQLObjectType(
        "DnsServer",
        {
            "id": GraphQLField(GraphQLString),
            "ipAddress": GraphQLField(GraphQLString),
        },
    )

    gateway_type = GraphQLObjectType(
        "Gateway",
        {
            "id": GraphQLField(GraphQLString),
            "hostname": GraphQLField(GraphQLString),
        },
    )

    query_type = GraphQLObjectType(
        "Query",
        {
            "dnsServers": GraphQLField(
                GraphQLList(dns_server_type),
                resolve=dns_servers_resolver,
            ),
            "gateways": GraphQLField(
                GraphQLList(gateway_type),
                resolve=gateways_resolver,
            ),
        },
    )

    schema = GraphQLSchema(query=query_type)

    # Execute multi-field query
    query = """
    {
        dnsServers {
            id
            ipAddress
        }
        gateways {
            id
            hostname
        }
    }
    """

    context = {}
    result = await execute_multi_field_query(schema, query, None, context)

    # Verify result is RustResponseBytes
    assert isinstance(result, RustResponseBytes)

    # Parse and verify structure
    result_json = json.loads(bytes(result))

    assert "data" in result_json
    assert "dnsServers" in result_json["data"]
    assert "gateways" in result_json["data"]

    # Verify dnsServers data
    assert len(result_json["data"]["dnsServers"]) == 2
    assert result_json["data"]["dnsServers"][0]["id"] == 1

    # Verify gateways data
    assert len(result_json["data"]["gateways"]) == 1
    assert result_json["data"]["gateways"][0]["id"] == 10


@pytest.fixture(scope="module", autouse=True)
def init_schema_registry_fixture():
    """Initialize schema registry for multi-field executor tests."""
    import fraiseql._fraiseql_rs as fraiseql_rs

    # Reset the schema registry to allow re-initialization
    fraiseql_rs.reset_schema_registry_for_testing()

    # Schema IR with types for testing
    schema_ir = {
        "version": "1.0",
        "features": ["type_resolution"],
        "types": {
            "DnsServer": {
                "fields": {
                    "id": {"type_name": "Int", "is_nested_object": False, "is_list": False},
                    "ip_address": {
                        "type_name": "String",
                        "is_nested_object": False,
                        "is_list": False,
                    },
                }
            },
            "Gateway": {
                "fields": {
                    "id": {"type_name": "Int", "is_nested_object": False, "is_list": False},
                    "hostname": {
                        "type_name": "String",
                        "is_nested_object": False,
                        "is_list": False,
                    },
                }
            },
        },
    }

    fraiseql_rs.initialize_schema_registry(json.dumps(schema_ir))
