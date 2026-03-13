"""Tests for LlamaIndex integration."""

from unittest.mock import AsyncMock

import pytest


@pytest.fixture
def mock_client():
    from fraiseql.client import FraiseQLClient

    client = AsyncMock(spec=FraiseQLClient)
    return client


@pytest.mark.anyio
async def test_reader_returns_documents(mock_client):
    from fraiseql.integrations.llamaindex import FraiseQLReader

    mock_client.execute.return_value = {
        "data": {"users": [{"id": "1", "name": "Alice", "email": "alice@example.com"}]}
    }

    reader = FraiseQLReader(client=mock_client)
    docs = await reader.aload_data(query="{ users { id name email } }")

    assert len(docs) == 1
    assert "Alice" in docs[0].text or "alice" in docs[0].text
    assert docs[0].metadata["source_query"] == "users"


@pytest.mark.anyio
async def test_reader_custom_text_template(mock_client):
    from fraiseql.integrations.llamaindex import FraiseQLReader

    mock_client.execute.return_value = {
        "data": {"users": [{"name": "Alice", "email": "alice@example.com"}]}
    }

    reader = FraiseQLReader(client=mock_client)
    docs = await reader.aload_data(
        query="{ users { name email } }",
        text_template="{name} ({email})",
    )

    assert len(docs) == 1
    assert docs[0].text == "Alice (alice@example.com)"


@pytest.mark.anyio
async def test_reader_metadata_fields(mock_client):
    from fraiseql.integrations.llamaindex import FraiseQLReader

    mock_client.execute.return_value = {
        "data": {
            "users": [{"id": "1", "name": "Alice", "email": "alice@example.com", "role": "admin"}]
        }
    }

    reader = FraiseQLReader(client=mock_client)
    docs = await reader.aload_data(
        query="{ users { id name email role } }",
        metadata_fields=["id", "role"],
    )

    assert len(docs) == 1
    assert docs[0].metadata["id"] == "1"
    assert docs[0].metadata["role"] == "admin"
    # Fields not in metadata_fields should not be in metadata
    assert "email" not in docs[0].metadata


@pytest.mark.anyio
async def test_reader_empty_results(mock_client):
    from fraiseql.integrations.llamaindex import FraiseQLReader

    mock_client.execute.return_value = {"data": {"users": []}}

    reader = FraiseQLReader(client=mock_client)
    docs = await reader.aload_data(query="{ users { id name } }")

    assert len(docs) == 0
