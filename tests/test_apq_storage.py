"""Tests for APQ query storage functionality."""

import pytest


def test_apq_query_storage():
    """Test storing and retrieving persisted queries by hash."""
    # This test will fail until we implement the storage functions
    from fraiseql.storage.apq_store import store_persisted_query, get_persisted_query

    query = "{ users { id name } }"
    hash_value = "ecf4edb46db40b5132295c0291d62fb65d6759a9eedfa4d5d612dd5ec54a6b38"

    # Should store successfully
    store_persisted_query(hash_value, query)

    # Should retrieve successfully
    retrieved = get_persisted_query(hash_value)
    assert retrieved == query


def test_apq_query_storage_missing_key():
    """Test retrieving non-existent persisted query."""
    from fraiseql.storage.apq_store import get_persisted_query

    result = get_persisted_query("nonexistent_hash")
    assert result is None


def test_apq_query_storage_overwrite():
    """Test overwriting existing persisted query."""
    from fraiseql.storage.apq_store import store_persisted_query, get_persisted_query

    hash_value = "test_hash_123"
    query1 = "{ users }"
    query2 = "{ posts }"

    # Store first query
    store_persisted_query(hash_value, query1)
    assert get_persisted_query(hash_value) == query1

    # Overwrite with second query
    store_persisted_query(hash_value, query2)
    assert get_persisted_query(hash_value) == query2


def test_apq_query_storage_multiple_queries():
    """Test storing multiple different queries."""
    from fraiseql.storage.apq_store import store_persisted_query, get_persisted_query

    queries = {
        "hash1": "{ users { id } }",
        "hash2": "{ posts { title } }",
        "hash3": "{ comments { content } }"
    }

    # Store all queries
    for hash_value, query in queries.items():
        store_persisted_query(hash_value, query)

    # Retrieve and verify all queries
    for hash_value, expected_query in queries.items():
        retrieved = get_persisted_query(hash_value)
        assert retrieved == expected_query


def test_apq_query_storage_clear():
    """Test clearing the APQ storage."""
    from fraiseql.storage.apq_store import store_persisted_query, get_persisted_query, clear_storage

    # Store a query
    store_persisted_query("test_hash", "{ hello }")
    assert get_persisted_query("test_hash") == "{ hello }"

    # Clear storage
    clear_storage()

    # Query should no longer exist
    assert get_persisted_query("test_hash") is None


def test_apq_storage_validation_errors():
    """Test storage validation catches invalid inputs."""
    from fraiseql.storage.apq_store import store_persisted_query

    # Empty hash should raise error
    with pytest.raises(ValueError, match="Hash value cannot be empty"):
        store_persisted_query("", "{ hello }")

    # Whitespace-only hash should raise error
    with pytest.raises(ValueError, match="Hash value cannot be empty"):
        store_persisted_query("   ", "{ hello }")

    # Empty query should raise error
    with pytest.raises(ValueError, match="Query cannot be empty"):
        store_persisted_query("test_hash", "")

    # Whitespace-only query should raise error
    with pytest.raises(ValueError, match="Query cannot be empty"):
        store_persisted_query("test_hash", "   ")


def test_compute_query_hash():
    """Test query hash computation."""
    from fraiseql.storage.apq_store import compute_query_hash

    query = "{ hello }"
    hash_result = compute_query_hash(query)

    # Should be valid SHA256 hex string
    assert len(hash_result) == 64
    assert all(c in "0123456789abcdef" for c in hash_result)

    # Same query should produce same hash
    assert compute_query_hash(query) == hash_result

    # Different query should produce different hash
    different_query = "{ world }"
    assert compute_query_hash(different_query) != hash_result


def test_get_storage_stats():
    """Test storage statistics."""
    from fraiseql.storage.apq_store import (
        store_persisted_query,
        get_storage_stats,
        clear_storage
    )

    # Clear storage first
    clear_storage()

    # Empty storage stats
    stats = get_storage_stats()
    assert stats["stored_queries"] == 0
    assert stats["total_size_bytes"] == 0

    # Add some queries
    store_persisted_query("hash1", "{ hello }")
    store_persisted_query("hash2", "{ world }")

    # Check updated stats
    stats = get_storage_stats()
    assert stats["stored_queries"] == 2
    assert stats["total_size_bytes"] > 0


def test_get_persisted_query_edge_cases():
    """Test edge cases for query retrieval."""
    from fraiseql.storage.apq_store import get_persisted_query

    # Empty hash should return None
    assert get_persisted_query("") is None

    # None hash should return None
    assert get_persisted_query(None) is None
