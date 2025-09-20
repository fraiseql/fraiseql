"""Apollo Automatic Persisted Queries (APQ) middleware for FraiseQL."""

from fraiseql.fastapi.routers import GraphQLRequest


def is_apq_request(request: GraphQLRequest) -> bool:
    """Detect if a GraphQL request is an APQ request.

    Args:
        request: GraphQL request to check

    Returns:
        True if the request contains APQ extensions, False otherwise
    """
    if not request.extensions:
        return False

    return "persistedQuery" in request.extensions


def get_apq_hash(request: GraphQLRequest) -> str | None:
    """Extract the APQ hash from a GraphQL request.

    Args:
        request: GraphQL request to extract hash from

    Returns:
        SHA256 hash string if APQ request, None otherwise
    """
    if not is_apq_request(request):
        return None

    persisted_query = request.extensions["persistedQuery"]
    return persisted_query.get("sha256Hash")


def is_apq_hash_only_request(request: GraphQLRequest) -> bool:
    """Check if request is APQ hash-only (no query field).

    Args:
        request: GraphQL request to check

    Returns:
        True if APQ request with no query, False otherwise
    """
    return is_apq_request(request) and not request.query


def is_apq_with_query_request(request: GraphQLRequest) -> bool:
    """Check if request is APQ with query (both hash and query).

    Args:
        request: GraphQL request to check

    Returns:
        True if APQ request with query field, False otherwise
    """
    return is_apq_request(request) and bool(request.query)
