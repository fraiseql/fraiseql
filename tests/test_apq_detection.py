"""Tests for APQ request detection functionality."""

import pytest
from fraiseql.fastapi.routers import GraphQLRequest


def test_detect_apq_request():
    """Test detection of APQ requests vs normal GraphQL requests."""
    # This test will fail until we implement is_apq_request function
    from fraiseql.middleware.apq import is_apq_request

    apq_request = GraphQLRequest(
        extensions={"persistedQuery": {"version": 1, "sha256Hash": "abc123"}}
    )
    normal_request = GraphQLRequest(query="{ hello }")

    assert is_apq_request(apq_request) == True
    assert is_apq_request(normal_request) == False


def test_detect_apq_request_with_both_query_and_hash():
    """Test APQ detection when both query and hash are present."""
    from fraiseql.middleware.apq import is_apq_request

    apq_request = GraphQLRequest(
        query="{ hello }", extensions={"persistedQuery": {"version": 1, "sha256Hash": "abc123"}}
    )

    assert is_apq_request(apq_request) == True


def test_detect_apq_request_with_non_apq_extensions():
    """Test APQ detection with non-APQ extensions."""
    from fraiseql.middleware.apq import is_apq_request

    request = GraphQLRequest(query="{ hello }", extensions={"tracing": {"version": 1}})

    assert is_apq_request(request) == False


def test_detect_apq_request_no_extensions():
    """Test APQ detection with no extensions."""
    from fraiseql.middleware.apq import is_apq_request

    request = GraphQLRequest(query="{ hello }")

    assert is_apq_request(request) == False


def test_detect_apq_request_empty_extensions():
    """Test APQ detection with empty extensions."""
    from fraiseql.middleware.apq import is_apq_request

    request = GraphQLRequest(query="{ hello }", extensions={})

    assert is_apq_request(request) == False


def test_get_apq_hash():
    """Test extracting APQ hash from request."""
    from fraiseql.middleware.apq import get_apq_hash

    apq_request = GraphQLRequest(
        extensions={"persistedQuery": {"version": 1, "sha256Hash": "abc123"}}
    )
    normal_request = GraphQLRequest(query="{ hello }")

    assert get_apq_hash(apq_request) == "abc123"
    assert get_apq_hash(normal_request) is None


def test_is_apq_hash_only_request():
    """Test detecting hash-only APQ requests."""
    from fraiseql.middleware.apq import is_apq_hash_only_request

    hash_only = GraphQLRequest(
        extensions={"persistedQuery": {"version": 1, "sha256Hash": "abc123"}}
    )
    with_query = GraphQLRequest(
        query="{ hello }", extensions={"persistedQuery": {"version": 1, "sha256Hash": "abc123"}}
    )
    normal = GraphQLRequest(query="{ hello }")

    assert is_apq_hash_only_request(hash_only) == True
    assert is_apq_hash_only_request(with_query) == False
    assert is_apq_hash_only_request(normal) == False


def test_is_apq_with_query_request():
    """Test detecting APQ requests that include query."""
    from fraiseql.middleware.apq import is_apq_with_query_request

    hash_only = GraphQLRequest(
        extensions={"persistedQuery": {"version": 1, "sha256Hash": "abc123"}}
    )
    with_query = GraphQLRequest(
        query="{ hello }", extensions={"persistedQuery": {"version": 1, "sha256Hash": "abc123"}}
    )
    normal = GraphQLRequest(query="{ hello }")

    assert is_apq_with_query_request(hash_only) == False
    assert is_apq_with_query_request(with_query) == True
    assert is_apq_with_query_request(normal) == False
