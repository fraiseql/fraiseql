"""Regression tests for multiple root query fields.

Issue: When querying multiple root-level fields, only ONE field appears
in the response instead of all requested fields.

Example:
    query {
        users { id }
        posts { id }
    }

Expected: {"data": {"users": [...], "posts": [...]}}
Actual: {"data": {"posts": [...]}}  ← Missing "users"
"""

import pytest
from graphql import graphql

from fraiseql import fraise_type, query


@pytest.mark.asyncio
class TestMultipleRootQueryFields:
    """Test that multiple root query fields all appear in response."""

    async def test_two_root_fields(self, clear_registry):
        """Query with TWO root fields should return BOTH fields."""

        @fraise_type(sql_source="users")
        class User:
            id: int
            name: str

        @fraise_type(sql_source="posts")
        class Post:
            id: int
            title: str

        @query
        async def get_users(info) -> list[User]:
            return [
                User(id=1, name="Alice"),
                User(id=2, name="Bob"),
            ]

        @query
        async def get_posts(info) -> list[Post]:
            return [
                Post(id=101, title="First Post"),
                Post(id=102, title="Second Post"),
            ]

        from fraiseql.gql.schema_builder import build_fraiseql_schema

        schema = build_fraiseql_schema(
            query_types=[get_users, get_posts],
            mutation_resolvers=[],
            camel_case_fields=True,
        )

        result = await graphql(
            schema,
            """
            query {
                getUsers { id name }
                getPosts { id title }
            }
        """,
        )

        print(f"DEBUG: result.data = {result.data}")
        print(f"DEBUG: result.errors = {result.errors}")

        assert result.data is not None, "Query should return data"
        assert "getUsers" in result.data, "getUsers field missing from response"
        assert "getPosts" in result.data, "getPosts field missing from response"

        assert len(result.data["getUsers"]) == 2
        assert result.data["getUsers"][0]["id"] == 1
        assert len(result.data["getPosts"]) == 2
        assert result.data["getPosts"][0]["id"] == 101

    async def test_three_root_fields(self, clear_registry):
        """Query with THREE root fields should return ALL three fields."""

        @fraise_type(sql_source="users")
        class User:
            id: int

        @fraise_type(sql_source="posts")
        class Post:
            id: int

        @fraise_type(sql_source="comments")
        class Comment:
            id: int

        @query
        async def get_users(info) -> list[User]:
            return [User(id=1), User(id=2)]

        @query
        async def get_posts(info) -> list[Post]:
            return [Post(id=101)]

        @query
        async def get_comments(info) -> list[Comment]:
            return [Comment(id=201)]

        from fraiseql.gql.schema_builder import build_fraiseql_schema

        schema = build_fraiseql_schema(
            query_types=[get_users, get_posts, get_comments],
            mutation_resolvers=[],
            camel_case_fields=True,
        )

        result = await graphql(
            schema,
            """
            query {
                getUsers { id }
                getPosts { id }
                getComments { id }
            }
        """,
        )

        assert result.data is not None
        assert "getUsers" in result.data
        assert "getPosts" in result.data
        assert "getComments" in result.data

        assert len(result.data["getUsers"]) == 2
        assert len(result.data["getPosts"]) == 1
        assert len(result.data["getComments"]) == 1

    async def test_five_root_fields_printoptim_scenario(self, clear_registry):
        """Query with FIVE root fields (real PrintOptim scenario)."""

        @fraise_type(sql_source="dns_servers")
        class DnsServer:
            id: str
            ip_address: str

        @fraise_type(sql_source="gateways")
        class Gateway:
            id: str
            ip_address: str

        @fraise_type(sql_source="routers")
        class Router:
            id: str
            hostname: str

        @fraise_type(sql_source="smtp_servers")
        class SmtpServer:
            id: str
            hostname: str

        @fraise_type(sql_source="print_servers")
        class PrintServer:
            id: str
            hostname: str

        @query
        async def dns_servers(info) -> list[DnsServer]:
            return [DnsServer(id="dns-1", ip_address="8.8.8.8")]

        @query
        async def gateways(info) -> list[Gateway]:
            return [Gateway(id="gw-1", ip_address="192.168.1.1")]

        @query
        async def routers(info) -> list[Router]:
            return [Router(id="rt-1", hostname="router-01")]

        @query
        async def smtp_servers(info) -> list[SmtpServer]:
            return [SmtpServer(id="smtp-1", hostname="mail.example.com")]

        @query
        async def print_servers(info) -> list[PrintServer]:
            return [PrintServer(id="ps-1", hostname="print-01")]

        from fraiseql.gql.schema_builder import build_fraiseql_schema

        schema = build_fraiseql_schema(
            query_types=[dns_servers, gateways, routers, smtp_servers, print_servers],
            mutation_resolvers=[],
            camel_case_fields=True,
        )

        result = await graphql(
            schema,
            """
            query GetNetworkSettings {
                dnsServers { id ipAddress }
                gateways { id ipAddress }
                routers { id hostname }
                smtpServers { id hostname }
                printServers { id hostname }
            }
        """,
        )

        assert result.data is not None, "Query should return data"
        assert "dnsServers" in result.data, "dnsServers missing"
        assert "gateways" in result.data, "gateways missing"
        assert "routers" in result.data, "routers missing"
        assert "smtpServers" in result.data, "smtpServers missing"
        assert "printServers" in result.data, "printServers missing"

        assert result.data["dnsServers"][0]["ipAddress"] == "8.8.8.8"
        assert result.data["gateways"][0]["ipAddress"] == "192.168.1.1"
        assert result.data["routers"][0]["hostname"] == "router-01"

    async def test_single_field_still_works(self, clear_registry):
        """Single field queries should still work (fast path unchanged)."""

        @fraise_type(sql_source="users")
        class User:
            id: int
            name: str

        @query
        async def get_users(info) -> list[User]:
            return [User(id=1, name="Alice")]

        from fraiseql.gql.schema_builder import build_fraiseql_schema

        schema = build_fraiseql_schema(
            query_types=[get_users],
            mutation_resolvers=[],
            camel_case_fields=True,
        )

        result = await graphql(
            schema,
            """
            query {
                getUsers { id name }
            }
        """,
        )

        assert result.data is not None
        assert "getUsers" in result.data
        assert len(result.data["getUsers"]) == 1
        assert result.data["getUsers"][0]["name"] == "Alice"
