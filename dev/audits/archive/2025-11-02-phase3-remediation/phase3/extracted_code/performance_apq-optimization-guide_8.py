# Extracted from: docs/performance/apq-optimization-guide.md
# Block number: 8
from fraiseql.middleware.apq_caching import is_cacheable_response


def custom_is_cacheable(response: dict, query_string: str) -> bool:
    """Custom caching logic."""
    # Only cache read-only queries
    if "mutation" in query_string.lower():
        return False

    # Don't cache queries with specific directives
    if "@nocache" in query_string:
        return False

    # Use default logic
    return is_cacheable_response(response)
