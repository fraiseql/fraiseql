"""Tests for the framework-agnostic RAG source."""

from unittest.mock import AsyncMock

import pytest


@pytest.fixture
def mock_client():
    from fraiseql.client import FraiseQLClient

    return AsyncMock(spec=FraiseQLClient)


@pytest.mark.anyio
async def test_retrieve_returns_text_and_metadata(mock_client):
    from fraiseql.integrations.rag import as_source

    mock_client.execute.return_value = {
        "data": {
            "documents": [
                {"id": "d1", "title": "Alpha", "content": "first body"},
                {"id": "d2", "title": "Beta", "content": "second body"},
            ]
        }
    }
    source = as_source(
        mock_client,
        "documents",
        fields=["id", "title", "content"],
        text_key="content",
    )

    hits = await source.retrieve("rotate credentials", top_k=2)
    assert [h["text"] for h in hits] == ["first body", "second body"]
    assert hits[0]["metadata"] == {"source": "documents", "id": "d1", "title": "Alpha"}

    # The query text and top_k reached the server under the declared arg names.
    assert mock_client.execute.call_args.kwargs["variables"] == {
        "search": "rotate credentials",
        "limit": 2,
    }
    (document,) = mock_client.execute.call_args.args
    assert document == (
        "query ($search: String, $limit: Int) "
        "{ documents(search: $search, limit: $limit) { id title content } }"
    )


@pytest.mark.anyio
async def test_metadata_keys_narrow_the_metadata(mock_client):
    from fraiseql.integrations.rag import as_source

    mock_client.execute.return_value = {
        "data": {"documents": [{"id": "d1", "title": "Alpha", "content": "body"}]}
    }
    source = as_source(
        mock_client,
        "documents",
        fields=["id", "title", "content"],
        text_key="content",
        metadata_keys=["id"],
    )

    hits = await source.retrieve("q")
    assert hits[0]["metadata"] == {"source": "documents", "id": "d1"}


def test_empty_fields_is_refused(mock_client):
    from fraiseql.integrations.rag import as_source

    with pytest.raises(ValueError, match="at least one"):
        as_source(mock_client, "documents", fields=[])
