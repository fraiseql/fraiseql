"""APQ storage backend abstract interface for FraiseQL."""

from abc import ABC, abstractmethod
from typing import Any, Dict, Optional


class APQStorageBackend(ABC):
    """Abstract base class for APQ storage backends.

    This interface provides pluggable storage support for FraiseQL's APQ system,
    enabling different storage implementations for persisted queries and cached responses.

    Backends can support:
    1. Persistent query storage by hash
    2. Pre-computed JSON response caching
    3. Direct JSON passthrough (bypass GraphQL execution for cached responses)
    """

    @abstractmethod
    def get_persisted_query(self, hash_value: str) -> Optional[str]:
        """Retrieve stored query by hash.

        Args:
            hash_value: SHA256 hash of the persisted query

        Returns:
            GraphQL query string if found, None otherwise
        """

    @abstractmethod
    def store_persisted_query(self, hash_value: str, query: str) -> None:
        """Store query by hash.

        Args:
            hash_value: SHA256 hash of the query
            query: GraphQL query string to store
        """

    @abstractmethod
    def get_cached_response(self, hash_value: str) -> Optional[Dict[str, Any]]:
        """Get cached JSON response for APQ hash.

        This enables direct JSON passthrough, bypassing GraphQL execution
        for pre-computed responses.

        Args:
            hash_value: SHA256 hash of the persisted query

        Returns:
            Cached GraphQL response dict if found, None otherwise
        """

    @abstractmethod
    def store_cached_response(self, hash_value: str, response: Dict[str, Any]) -> None:
        """Store pre-computed JSON response for APQ hash.

        Args:
            hash_value: SHA256 hash of the persisted query
            response: GraphQL response dict to cache
        """
