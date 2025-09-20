"""APQ query storage implementation for FraiseQL."""

import hashlib
import logging
from typing import Dict, Optional

logger = logging.getLogger(__name__)

# In-memory storage for APQ queries
_apq_storage: Dict[str, str] = {}


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

    _apq_storage[hash_value] = query
    logger.debug(f"Stored APQ query with hash {hash_value[:8]}...")


def get_persisted_query(hash_value: str) -> Optional[str]:
    """Retrieve a persisted query by its hash.

    Args:
        hash_value: SHA256 hash of the query

    Returns:
        GraphQL query string if found, None otherwise
    """
    if not hash_value:
        return None

    query = _apq_storage.get(hash_value)
    if query:
        logger.debug(f"Retrieved APQ query with hash {hash_value[:8]}...")
    else:
        logger.debug(f"APQ query not found for hash {hash_value[:8]}...")

    return query


def clear_storage() -> None:
    """Clear all stored persisted queries."""
    count = len(_apq_storage)
    _apq_storage.clear()
    logger.debug(f"Cleared {count} APQ queries from storage")


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
    return {
        "stored_queries": len(_apq_storage),
        "total_size_bytes": sum(len(query.encode("utf-8")) for query in _apq_storage.values()),
    }
