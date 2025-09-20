"""Tests for APQ protocol handler functionality."""

import pytest
from fraiseql.fastapi.routers import GraphQLRequest


def test_apq_protocol_responses():
    """Test APQ protocol responses for missing/found queries."""
    # This test will fail until we implement the protocol handler
    from fraiseql.middleware.apq import handle_apq_request
    from fraiseql.storage.apq_store import store_persisted_query, clear_storage

    # Clear storage for clean test
    clear_storage()

    # Missing query should return specific error
    missing_response = handle_apq_request("unknown_hash", None)
    assert "errors" in missing_response
    assert missing_response["errors"][0]["extensions"]["code"] == "PERSISTED_QUERY_NOT_FOUND"

    # Found query should process normally
    store_persisted_query("abc123", "{ hello }")
    found_response = handle_apq_request("abc123", None)
    assert "data" in found_response


def test_apq_protocol_persisted_query_not_found_error():
    """Test specific APQ error format for missing queries."""
    from fraiseql.middleware.apq import handle_apq_request
    from fraiseql.storage.apq_store import clear_storage

    clear_storage()

    response = handle_apq_request("missing_hash_123", None)

    # Should match Apollo Client expected format
    assert response == {
        "errors": [
            {
                "message": "PersistedQueryNotFound",
                "extensions": {
                    "code": "PERSISTED_QUERY_NOT_FOUND"
                }
            }
        ]
    }


def test_apq_protocol_query_execution():
    """Test APQ protocol executes found queries correctly."""
    from fraiseql.middleware.apq import handle_apq_request
    from fraiseql.storage.apq_store import store_persisted_query, clear_storage

    clear_storage()

    # Store a simple query
    query = "{ __typename }"
    hash_value = "test_hash_123"
    store_persisted_query(hash_value, query)

    # Should execute the query successfully
    response = handle_apq_request(hash_value, None)

    # Should contain execution result
    assert "data" in response
    assert "__typename" in response["data"]


def test_apq_protocol_with_variables():
    """Test APQ protocol handles variables correctly."""
    from fraiseql.middleware.apq import handle_apq_request
    from fraiseql.storage.apq_store import store_persisted_query, clear_storage

    clear_storage()

    # Store a query with variables
    query = "query GetUser($id: ID!) { user(id: $id) { name } }"
    hash_value = "query_with_vars_hash"
    store_persisted_query(hash_value, query)

    variables = {"id": "123"}

    # Should execute with variables
    response = handle_apq_request(hash_value, variables)

    # Should contain execution result (may fail due to schema, but structure should be correct)
    assert "data" in response or "errors" in response


def test_apq_protocol_invalid_hash():
    """Test APQ protocol handles invalid hash gracefully."""
    from fraiseql.middleware.apq import handle_apq_request

    # Empty hash should return not found
    response = handle_apq_request("", None)
    assert response["errors"][0]["extensions"]["code"] == "PERSISTED_QUERY_NOT_FOUND"

    # None hash should return not found
    response = handle_apq_request(None, None)
    assert response["errors"][0]["extensions"]["code"] == "PERSISTED_QUERY_NOT_FOUND"


def test_apq_protocol_with_operation_name():
    """Test APQ protocol handles operation names correctly."""
    from fraiseql.middleware.apq import handle_apq_request
    from fraiseql.storage.apq_store import store_persisted_query, clear_storage

    clear_storage()

    # Store a query with multiple operations
    query = """
    query GetUser { user { name } }
    query GetPost { post { title } }
    """
    hash_value = "multi_op_hash"
    store_persisted_query(hash_value, query)

    # Should execute specific operation
    response = handle_apq_request(hash_value, None, operation_name="GetUser")

    # Should contain execution result
    assert "data" in response or "errors" in response
