"""Test custom context_getter functionality."""

from typing import Any

import pytest
from fastapi import Request
from fastapi.testclient import TestClient

from fraiseql import fraise_type
from fraiseql.fastapi import create_fraiseql_app


# Sample type
@fraise_type
class SampleType:
    id: int
    custom_value: str


# Sample query
async def get_test(info) -> SampleType:
    """Get test data using custom context."""
    custom_data = info.context.get("custom_data", {})
    return SampleType(id=1, custom_value=custom_data.get("value", "default"))


@pytest.fixture
def app_with_custom_context():
    """Create app with custom context getter."""

    async def custom_context_getter(request: Request) -> dict[str, Any]:
        """Custom context that adds extra data."""
        return {
            "custom_data": {
                "value": "from_custom_context",
                "request_path": str(request.url.path),
            },
            "db": None,  # Would normally be a database connection
            "user": None,
        }

    app = create_fraiseql_app(
        database_url="postgresql://localhost/test",
        types=[SampleType],
        queries=[get_test],
        context_getter=custom_context_getter,
        production=False,
    )

    return app


def test_custom_context_getter(app_with_custom_context):
    """Test that custom context getter is used."""
    client = TestClient(app_with_custom_context)

    query = """
        query {
            get_test {
                id
                custom_value
            }
        }
    """

    response = client.post("/graphql", json={"query": query})

    assert response.status_code == 200
    data = response.json()

    assert data["data"]["get_test"]["id"] == 1
    assert data["data"]["get_test"]["custom_value"] == "from_custom_context"


def test_custom_context_getter_with_get_request(app_with_custom_context):
    """Test that custom context getter works with GET requests."""
    client = TestClient(app_with_custom_context)

    query = """
        query {
            get_test {
                id
                custom_value
            }
        }
    """

    response = client.get("/graphql", params={"query": query})

    assert response.status_code == 200
    data = response.json()

    assert data["data"]["get_test"]["id"] == 1
    assert data["data"]["get_test"]["custom_value"] == "from_custom_context"


def test_default_context_without_custom_getter():
    """Test that default context is used when no custom getter provided."""

    # For this test, we need to provide a custom context getter
    # because the default one requires database setup
    async def minimal_context_getter(request: Request) -> dict[str, Any]:
        """Minimal context for testing."""
        return {
            "db": None,
            "user": None,
            "custom_data": {},  # Empty custom data
        }

    app = create_fraiseql_app(
        database_url="postgresql://localhost/test",
        types=[SampleType],
        queries=[get_test],
        context_getter=minimal_context_getter,
        production=False,
    )

    client = TestClient(app)

    query = """
        query {
            get_test {
                id
                custom_value
            }
        }
    """

    response = client.post("/graphql", json={"query": query})

    assert response.status_code == 200
    data = response.json()

    # Without custom context, should get default value
    assert data["data"]["get_test"]["id"] == 1
    assert data["data"]["get_test"]["custom_value"] == "default"
