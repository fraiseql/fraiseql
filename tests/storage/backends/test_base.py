"""Tests for APQ storage backend abstract interface."""

import pytest

from fraiseql.storage.backends.base import APQStorageBackend


def test_apq_storage_backend_is_abstract() -> None:
    """Test that APQStorageBackend is an abstract base class."""
    # Should not be able to instantiate directly
    with pytest.raises(TypeError):
        APQStorageBackend()


def test_apq_storage_backend_interface() -> None:
    """Test that APQStorageBackend defines the required interface."""
    # Check that all required methods are abstract
    abstract_methods = APQStorageBackend.__abstractmethods__
    expected_methods = {
        "get_persisted_query",
        "store_persisted_query",
        "get_cached_response",
        "store_cached_response",
    }
    assert abstract_methods == expected_methods


def test_concrete_implementation_must_implement_all_methods() -> None:
    """Test that concrete implementations must implement all abstract methods."""

    class IncompleteBackend(APQStorageBackend):
        """Incomplete implementation missing required methods."""

        def get_persisted_query(self, hash_value: str) -> None:
            return None

    # Should fail to instantiate due to missing methods
    with pytest.raises(TypeError):
        IncompleteBackend()


def test_concrete_implementation_with_all_methods() -> None:
    """Test that concrete implementations work when all methods are implemented."""

    class CompleteBackend(APQStorageBackend):
        """Complete implementation with all required methods."""

        def get_persisted_query(self, hash_value: str) -> None:
            return None

        def store_persisted_query(self, hash_value: str, query: str) -> None:
            pass

        def get_cached_response(self, hash_value: str) -> None:
            return None

        def store_cached_response(self, hash_value: str, response) -> None:
            pass

    # Should successfully instantiate
    backend = CompleteBackend()
    assert isinstance(backend, APQStorageBackend)


def test_method_signatures() -> None:
    """Test that abstract methods have correct signatures."""

    class TestBackend(APQStorageBackend):
        def get_persisted_query(self, hash_value: str) -> str | None:
            return "test query"

        def store_persisted_query(self, hash_value: str, query: str) -> None:
            pass

        def get_cached_response(self, hash_value: str) -> dict | None:
            return {"data": {"test": "response"}}

        def store_cached_response(self, hash_value: str, response: dict) -> None:
            pass

    backend = TestBackend()

    # Test method calls work with expected signatures
    assert backend.get_persisted_query("hash123") == "test query"
    backend.store_persisted_query("hash123", "query")
    assert backend.get_cached_response("hash123") == {"data": {"test": "response"}}
    backend.store_cached_response("hash123", {"data": {"test": True}})
