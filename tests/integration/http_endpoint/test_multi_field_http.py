"""HTTP endpoint tests for multiple root query fields.

Issue: When querying multiple root-level fields via the /graphql HTTP endpoint,
only ONE field appears in the response instead of all requested fields.

This bug does NOT occur with direct graphql() execution, only via HTTP.

Context: Discovered in PrintOptim backend (Issue #259)
"""

import pytest
from fastapi.testclient import TestClient

import fraiseql
from fraiseql.fastapi import create_fraiseql_app


@pytest.mark.asyncio
class TestMultiFieldHTTP:
    """Test that multiple root query fields work via HTTP endpoint."""

    async def test_two_fields_via_http(self, clear_registry):
        """Query with TWO root fields via HTTP should return BOTH fields."""

        @fraiseql.type(sql_source="users")
        class User:
            id: int
            name: str

        @fraiseql.type(sql_source="posts")
        class Post:
            id: int
            title: str

        @fraiseql.query
        async def get_users(info) -> list[User]:
            return [
                User(id=1, name="Alice"),
                User(id=2, name="Bob"),
            ]

        @fraiseql.query
        async def get_posts(info) -> list[Post]:
            return [
                Post(id=101, title="First Post"),
                Post(id=102, title="Second Post"),
            ]

        app = create_fraiseql_app(
            database_url="postgresql://test@localhost/test",
            types=[User, Post],
            queries=[get_users, get_posts],
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        query {
                            getUsers { id name }
                            getPosts { id title }
                        }
                    """
                },
            )

            assert response.status_code == 200
            result = response.json()

            assert "data" in result, "Response should have data"
            assert "getUsers" in result["data"], "getUsers field missing from HTTP response"
            assert "getPosts" in result["data"], "getPosts field missing from HTTP response"

            assert len(result["data"]["getUsers"]) == 2
            assert result["data"]["getUsers"][0]["id"] == 1
            assert len(result["data"]["getPosts"]) == 2
            assert result["data"]["getPosts"][0]["id"] == 101

    async def test_three_fields_via_http(self, clear_registry):
        """Query with THREE root fields via HTTP should return ALL three."""

        @fraiseql.type(sql_source="users")
        class User:
            id: int

        @fraiseql.type(sql_source="posts")
        class Post:
            id: int

        @fraiseql.type(sql_source="comments")
        class Comment:
            id: int

        @fraiseql.query
        async def get_users(info) -> list[User]:
            return [User(id=1), User(id=2)]

        @fraiseql.query
        async def get_posts(info) -> list[Post]:
            return [Post(id=101)]

        @fraiseql.query
        async def get_comments(info) -> list[Comment]:
            return [Comment(id=201)]

        app = create_fraiseql_app(
            database_url="postgresql://test@localhost/test",
            types=[User, Post, Comment],
            queries=[get_users, get_posts, get_comments],
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        query {
                            getUsers { id }
                            getPosts { id }
                            getComments { id }
                        }
                    """
                },
            )

            result = response.json()

            assert "getUsers" in result["data"]
            assert "getPosts" in result["data"]
            assert "getComments" in result["data"]

            assert len(result["data"]["getUsers"]) == 2
            assert len(result["data"]["getPosts"]) == 1
            assert len(result["data"]["getComments"]) == 1

    async def test_five_fields_via_http_printoptim_scenario(self, clear_registry):
        """Query with FIVE root fields (PrintOptim Issue #259 scenario)."""

        @fraiseql.type(sql_source="dns_servers")
        class DnsServer:
            id: str
            ip_address: str

        @fraiseql.type(sql_source="gateways")
        class Gateway:
            id: str
            ip_address: str

        @fraiseql.type(sql_source="routers")
        class Router:
            id: str
            hostname: str

        @fraiseql.type(sql_source="smtp_servers")
        class SmtpServer:
            id: str
            hostname: str

        @fraiseql.type(sql_source="print_servers")
        class PrintServer:
            id: str
            hostname: str

        @fraiseql.query
        async def dns_servers(info) -> list[DnsServer]:
            return [DnsServer(id="dns-1", ip_address="8.8.8.8")]

        @fraiseql.query
        async def gateways(info) -> list[Gateway]:
            return [Gateway(id="gw-1", ip_address="192.168.1.1")]

        @fraiseql.query
        async def routers(info) -> list[Router]:
            return [Router(id="rt-1", hostname="router-01")]

        @fraiseql.query
        async def smtp_servers(info) -> list[SmtpServer]:
            return [SmtpServer(id="smtp-1", hostname="mail.example.com")]

        @fraiseql.query
        async def print_servers(info) -> list[PrintServer]:
            return [PrintServer(id="ps-1", hostname="print-01")]

        app = create_fraiseql_app(
            database_url="postgresql://test@localhost/test",
            types=[DnsServer, Gateway, Router, SmtpServer, PrintServer],
            queries=[dns_servers, gateways, routers, smtp_servers, print_servers],
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        query GetNetworkSettings {
                            dnsServers { id ipAddress }
                            gateways { id ipAddress }
                            routers { id hostname }
                            smtpServers { id hostname }
                            printServers { id hostname }
                        }
                    """
                },
            )

            result = response.json()

            assert "data" in result, "Response should have data"
            assert "dnsServers" in result["data"], "dnsServers missing"
            assert "gateways" in result["data"], "gateways missing"
            assert "routers" in result["data"], "routers missing"
            assert "smtpServers" in result["data"], "smtpServers missing"
            assert "printServers" in result["data"], "printServers missing"

            assert result["data"]["dnsServers"][0]["ipAddress"] == "8.8.8.8"
            assert result["data"]["gateways"][0]["ipAddress"] == "192.168.1.1"
            assert result["data"]["routers"][0]["hostname"] == "router-01"

    async def test_single_field_via_http_still_works(self, clear_registry):
        """Single field queries via HTTP should still work (regression protection)."""

        @fraiseql.type(sql_source="users")
        class User:
            id: int
            name: str

        @fraiseql.query
        async def get_users(info) -> list[User]:
            return [User(id=1, name="Alice")]

        app = create_fraiseql_app(
            database_url="postgresql://test@localhost/test",
            types=[User],
            queries=[get_users],
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        query {
                            getUsers { id name }
                        }
                    """
                },
            )

            result = response.json()

            assert "getUsers" in result["data"]
            assert len(result["data"]["getUsers"]) == 1
            assert result["data"]["getUsers"][0]["name"] == "Alice"
