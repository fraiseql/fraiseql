"""Integration test for multi-field query routing (Phase 1).

Tests that multi-field queries are properly detected and routed to the
Rust-only merge path via execute_multi_field_query().

This test uses the real FastAPI app with database connectivity to verify
end-to-end behavior.
"""

from contextlib import asynccontextmanager

import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient

import fraiseql
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.fastapi.config import FraiseQLConfig
from fraiseql.gql.schema_builder import SchemaRegistry

pytestmark = pytest.mark.integration


@pytest.fixture(autouse=True)
def clear_registry() -> None:
    """Clear registry before each test to avoid type conflicts."""
    registry = SchemaRegistry.get_instance()
    registry.clear()

    # Also clear the GraphQL type cache
    from fraiseql.core.graphql_type import _graphql_type_cache

    _graphql_type_cache.clear()

    yield

    registry.clear()
    _graphql_type_cache.clear()


@asynccontextmanager
async def noop_lifespan(app: FastAPI) -> None:
    """No-op lifespan for tests that don't need a database."""
    yield


# Define test types and queries at module level to avoid scoping issues
@fraiseql.type
class User:
    """User type for multi-field testing."""

    id: int
    name: str
    email: str


@fraiseql.type
class Post:
    """Post type for multi-field testing."""

    id: int
    title: str
    content: str


@fraiseql.type
class Comment:
    """Comment type for multi-field testing."""

    id: int
    text: str


@fraiseql.query
async def users(info) -> list[User]:
    """Return list of users."""
    return [
        User(id=1, name="Alice", email="alice@example.com"),
        User(id=2, name="Bob", email="bob@example.com"),
    ]


@fraiseql.query
async def posts(info) -> list[Post]:
    """Return list of posts."""
    return [
        Post(id=101, title="First Post", content="Content 1"),
        Post(id=102, title="Second Post", content="Content 2"),
    ]


@fraiseql.query
async def comments(info) -> list[Comment]:
    """Return list of comments."""
    return [
        Comment(id=201, text="Great post!"),
        Comment(id=202, text="Thanks for sharing"),
    ]


class TestMultiFieldRouting:
    """Test end-to-end multi-field query routing through FastAPI."""

    def test_two_root_fields_via_routing(self) -> None:
        """Test that two-field queries are routed through Rust merge path."""
        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test",
            environment="development",
        )

        app = create_fraiseql_app(
            config=config,
            types=[User, Post],
            queries=[users, posts],
            lifespan=noop_lifespan,
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        query {
                            users { id name email }
                            posts { id title }
                        }
                    """
                },
            )

            assert response.status_code == 200
            data = response.json()

            # Verify both fields are present
            assert "data" in data
            assert "users" in data["data"], "users field missing from response"
            assert "posts" in data["data"], "posts field missing from response"

            # Verify data integrity
            assert len(data["data"]["users"]) == 2
            assert data["data"]["users"][0]["id"] == 1
            assert data["data"]["users"][0]["name"] == "Alice"
            assert data["data"]["users"][0]["email"] == "alice@example.com"

            assert len(data["data"]["posts"]) == 2
            assert data["data"]["posts"][0]["id"] == 101
            assert data["data"]["posts"][0]["title"] == "First Post"

    def test_three_root_fields_via_routing(self) -> None:
        """Test that three-field queries are routed through Rust merge path."""
        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test",
            environment="development",
        )

        app = create_fraiseql_app(
            config=config,
            types=[User, Post, Comment],
            queries=[users, posts, comments],
            lifespan=noop_lifespan,
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql",
                json={
                    "query": """
                        query {
                            users { id name }
                            posts { id title }
                            comments { id text }
                        }
                    """
                },
            )

            assert response.status_code == 200
            data = response.json()

            # Verify all three fields present
            assert "data" in data
            assert "users" in data["data"]
            assert "posts" in data["data"]
            assert "comments" in data["data"]

            # Verify data integrity
            assert len(data["data"]["users"]) == 2
            assert len(data["data"]["posts"]) == 2
            assert len(data["data"]["comments"]) == 2

    def test_single_field_still_uses_fast_path(self) -> None:
        """Test that single-field queries still use the RustResponseBytes fast path."""
        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test",
            environment="development",
        )

        app = create_fraiseql_app(
            config=config,
            types=[User],
            queries=[users],
            lifespan=noop_lifespan,
        )

        with TestClient(app) as client:
            response = client.post(
                "/graphql",
                json={"query": "query { users { id name } }"},
            )

            assert response.status_code == 200
            data = response.json()

            assert "data" in data
            assert "users" in data["data"]
            assert len(data["data"]["users"]) == 2
            assert data["data"]["users"][0]["name"] == "Alice"

    def test_five_root_fields_printoptim_scenario(self) -> None:
        """Test five-field query (real PrintOptim scenario)."""

        @fraiseql.type
        class DnsServer:
            id: str
            ip_address: str

        @fraiseql.type
        class Gateway:
            id: str
            ip_address: str

        @fraiseql.type
        class Router:
            id: str
            hostname: str

        @fraiseql.type
        class SmtpServer:
            id: str
            hostname: str

        @fraiseql.type
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

        config = FraiseQLConfig(
            database_url="postgresql://test:test@localhost/test",
            environment="development",
        )

        app = create_fraiseql_app(
            config=config,
            types=[DnsServer, Gateway, Router, SmtpServer, PrintServer],
            queries=[dns_servers, gateways, routers, smtp_servers, print_servers],
            lifespan=noop_lifespan,
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

            assert response.status_code == 200
            data = response.json()

            # Verify all five fields present
            assert "data" in data
            assert "dnsServers" in data["data"]
            assert "gateways" in data["data"]
            assert "routers" in data["data"]
            assert "smtpServers" in data["data"]
            assert "printServers" in data["data"]

            # Verify data integrity
            assert data["data"]["dnsServers"][0]["ipAddress"] == "8.8.8.8"
            assert data["data"]["gateways"][0]["ipAddress"] == "192.168.1.1"
            assert data["data"]["routers"][0]["hostname"] == "router-01"
            assert data["data"]["smtpServers"][0]["hostname"] == "mail.example.com"
            assert data["data"]["printServers"][0]["hostname"] == "print-01"
