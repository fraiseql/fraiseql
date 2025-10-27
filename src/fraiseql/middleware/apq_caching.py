"""APQ cached response middleware for FraiseQL.

This module provides response caching functionality for APQ queries,
enabling direct JSON passthrough to bypass GraphQL execution for
pre-computed responses.
"""

import logging
from typing import Any, Optional

from fraiseql.fastapi.config import FraiseQLConfig
from fraiseql.fastapi.routers import GraphQLRequest
from fraiseql.monitoring import get_global_metrics
from fraiseql.storage.backends.base import APQStorageBackend
from fraiseql.storage.backends.factory import create_apq_backend

logger = logging.getLogger(__name__)

# Global backend cache to avoid recreating backends
_backend_cache: dict[str, APQStorageBackend] = {}


def get_apq_backend(config: FraiseQLConfig) -> APQStorageBackend:
    """Get APQ backend instance for the given configuration.

    Uses singleton pattern to avoid recreating backends for the same config.

    Args:
        config: FraiseQL configuration

    Returns:
        APQ storage backend instance
    """
    # Create a cache key based on backend type and config
    cache_key = f"{config.apq_storage_backend}:{hash(str(config.apq_backend_config))}"

    if cache_key not in _backend_cache:
        _backend_cache[cache_key] = create_apq_backend(config)
        logger.debug(f"Created APQ backend: {config.apq_storage_backend}")

    return _backend_cache[cache_key]


def handle_apq_request_with_cache(
    request: GraphQLRequest,
    backend: APQStorageBackend,
    config: FraiseQLConfig,
    context: Optional[dict[str, Any]] = None,
) -> Optional[dict[str, Any]]:
    """Handle APQ request with response caching support.

    This function implements the enhanced APQ flow:
    1. Check for cached response (if caching enabled)
    2. Return cached response if found
    3. Return None if cache miss (caller should execute query)

    Args:
        request: GraphQL request with APQ extensions
        backend: APQ storage backend
        config: FraiseQL configuration
        context: Optional request context containing user/tenant information

    Returns:
        Cached response dict if found, None if cache miss or caching disabled
    """
    if not config.apq_cache_responses:
        logger.debug("APQ response caching is disabled")
        return None

    # Extract APQ hash
    if not request.extensions or "persistedQuery" not in request.extensions:
        return None

    persisted_query = request.extensions["persistedQuery"]
    sha256_hash = persisted_query.get("sha256Hash")

    if not sha256_hash:
        return None

    # Try to get cached response
    try:
        metrics = get_global_metrics()
        cached_response = backend.get_cached_response(sha256_hash, context=context)
        if cached_response:
            logger.debug(f"APQ cache hit: {sha256_hash[:8]}...")
            metrics.record_response_cache_hit(sha256_hash)
            return cached_response
        logger.debug(f"APQ cache miss: {sha256_hash[:8]}...")
        metrics.record_response_cache_miss(sha256_hash)
        return None
    except Exception as e:
        logger.warning(f"Failed to retrieve cached response: {e}")
        return None


def store_response_in_cache(
    hash_value: str,
    response: dict[str, Any],
    backend: APQStorageBackend,
    config: FraiseQLConfig,
    context: Optional[dict[str, Any]] = None,
) -> None:
    """Store GraphQL response in cache for future APQ requests.

    Only stores successful responses (no errors). Responses with errors
    are not cached to avoid serving stale error responses.

    Args:
        hash_value: SHA256 hash of the persisted query
        response: GraphQL response dict to cache
        backend: APQ storage backend
        config: FraiseQL configuration
        context: Optional request context containing user/tenant information
    """
    if not config.apq_cache_responses:
        return

    # Don't cache error responses or partial responses with errors
    if "errors" in response:
        logger.debug(f"Skipping cache for response with errors: {hash_value[:8]}...")
        return

    # Don't cache responses without data
    if "data" not in response:
        logger.debug(f"Skipping cache for response without data: {hash_value[:8]}...")
        return

    try:
        backend.store_cached_response(hash_value, response, context=context)
        metrics = get_global_metrics()
        metrics.record_response_cache_store(hash_value)
        logger.debug(f"Stored response in cache: {hash_value[:8]}...")
    except Exception as e:
        logger.warning(f"Failed to store response in cache: {e}")


def get_apq_hash_from_request(request: GraphQLRequest) -> Optional[str]:
    """Extract APQ hash from GraphQL request.

    Args:
        request: GraphQL request

    Returns:
        SHA256 hash if APQ request, None otherwise
    """
    if not request.extensions or "persistedQuery" not in request.extensions:
        return None

    persisted_query = request.extensions["persistedQuery"]
    return persisted_query.get("sha256Hash")


def is_cacheable_response(response: dict[str, Any]) -> bool:
    """Check if a GraphQL response is suitable for caching.

    Args:
        response: GraphQL response dict

    Returns:
        True if response can be cached, False otherwise
    """
    # Don't cache responses with errors
    if "errors" in response:
        return False

    # Don't cache responses without data
    if "data" not in response:
        return False

    # Could add more sophisticated caching rules here
    # For example, check for cache-control directives in extensions
    return True


def clear_backend_cache() -> None:
    """Clear the global backend cache.

    This is primarily useful for testing.
    """
    global _backend_cache  # noqa: PLW0602
    _backend_cache.clear()
    logger.debug("Cleared APQ backend cache")
