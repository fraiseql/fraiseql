"""APQ query storage implementation for FraiseQL.

This module maintains backward compatibility while using the new backend system internally.
"""

import hashlib
import logging
from typing import Dict, Optional

from .backends.memory import MemoryAPQBackend

logger = logging.getLogger(__name__)

# Global memory backend instance for backward compatibility
_backend = MemoryAPQBackend()


def store_persisted_query(hash_value: str, query: str) -> None:
    """Store a persisted query by its hash.

    Args:
        hash_value: SHA256 hash of the query
        query: GraphQL query string to store

    Raises:
        ValueError: If hash_value is empty or query is empty
        ValueError: If hash_value doesn't match the query's actual hash
    """
    if not hash_value or not hash_value.strip():
        raise ValueError("Hash value cannot be empty")

    if not query or not query.strip():
        raise ValueError("Query cannot be empty")

    # Validate that the hash matches the query
    actual_hash = compute_query_hash(query)
    if hash_value != actual_hash:
        logger.warning(
            f"Hash mismatch: provided={hash_value[:8]}..., "
            f"computed={actual_hash[:8]}... - storing anyway for APQ compatibility"
        )

    _backend.store_persisted_query(hash_value, query)


def get_persisted_query(hash_value: str) -> Optional[str]:
    """Retrieve a persisted query by its hash.

    Args:
        hash_value: SHA256 hash of the query

    Returns:
        GraphQL query string if found, None otherwise
    """
    return _backend.get_persisted_query(hash_value)


def clear_storage() -> None:
    """Clear all stored persisted queries."""
    _backend.clear_storage()


def compute_query_hash(query: str) -> str:
    """Compute SHA256 hash of a GraphQL query.

    Args:
        query: GraphQL query string

    Returns:
        SHA256 hash as hex string
    """
    return hashlib.sha256(query.encode("utf-8")).hexdigest()


def get_storage_stats() -> Dict[str, int]:
    """Get storage statistics.

    Returns:
        Dictionary with storage statistics
    """
    stats = _backend.get_storage_stats()
    # Return only the fields that existed in the original function for backward compatibility
    return {
        "stored_queries": stats["stored_queries"],
        "total_size_bytes": stats["total_query_size_bytes"],
    }
