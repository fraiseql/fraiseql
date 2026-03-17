"""Tests for the FraiseQL error hierarchy."""

import pytest

from fraiseql.errors import (
    AuthenticationError,
    FederationValidationError,
    FraiseQLError,
    GraphQLError,
    NetworkError,
    TimeoutError,
)

# ─── Hierarchy ────────────────────────────────────────────────────────────────


def test_graphql_error_is_fraiseql_error():
    assert issubclass(GraphQLError, FraiseQLError)


def test_network_error_is_fraiseql_error():
    assert issubclass(NetworkError, FraiseQLError)


def test_timeout_error_is_network_error():
    assert issubclass(TimeoutError, NetworkError)


def test_authentication_error_is_fraiseql_error():
    assert issubclass(AuthenticationError, FraiseQLError)


def test_federation_validation_error_is_value_error():
    assert issubclass(FederationValidationError, ValueError)


# ─── GraphQLError ─────────────────────────────────────────────────────────────


def test_graphql_error_message_from_first_error():
    exc = GraphQLError([{"message": "Field not found"}, {"message": "Another error"}])
    assert str(exc) == "Field not found"


def test_graphql_error_stores_full_errors_list():
    errors = [{"message": "A"}, {"message": "B"}]
    exc = GraphQLError(errors)
    assert exc.errors is errors


def test_graphql_error_empty_list_uses_default_message():
    exc = GraphQLError([])
    assert str(exc) == "GraphQL error"


def test_graphql_error_missing_message_key_uses_default():
    exc = GraphQLError([{"extensions": {"code": "INTERNAL"}}])
    assert str(exc) == "GraphQL error"


def test_graphql_error_catchable_as_fraiseql_error():
    with pytest.raises(FraiseQLError):
        raise GraphQLError([{"message": "boom"}])


# ─── NetworkError ─────────────────────────────────────────────────────────────


def test_network_error_basic():
    exc = NetworkError("Connection refused")
    assert str(exc) == "Connection refused"


def test_timeout_error_is_also_network_error():
    with pytest.raises(NetworkError):
        raise TimeoutError("timed out")


# ─── AuthenticationError ──────────────────────────────────────────────────────


def test_authentication_error_401():
    exc = AuthenticationError(401)
    assert exc.status_code == 401
    assert "401" in str(exc)


def test_authentication_error_403():
    exc = AuthenticationError(403)
    assert exc.status_code == 403
    assert "403" in str(exc)


def test_authentication_error_catchable_as_fraiseql_error():
    with pytest.raises(FraiseQLError):
        raise AuthenticationError(401)
