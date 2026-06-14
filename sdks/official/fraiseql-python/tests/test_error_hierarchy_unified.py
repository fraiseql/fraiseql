"""H27: one unified ``FraiseQLError`` hierarchy at the package level.

``except fraiseql.FraiseQLError`` must catch every error either the synchronous
:class:`fraiseql.FraiseQLClient` or the asynchronous
:class:`fraiseql.AsyncFraiseQLClient` can raise. Before the fix the package-level
``FraiseQLError`` (defined in ``client.py``) was an entirely separate class from
the async client's errors (defined in ``errors.py``), so the documented catch-all
silently caught nothing the async client raised — ``issubclass`` was ``False``.
"""

import fraiseql
from fraiseql import client as client_mod
from fraiseql import errors as errors_mod


def test_async_client_errors_are_package_fraiseql_errors():
    """Everything AsyncFraiseQLClient raises is a fraiseql.FraiseQLError."""
    for exc_type in (
        fraiseql.GraphQLError,
        fraiseql.NetworkError,
        fraiseql.TimeoutError,
        fraiseql.AuthenticationError,
    ):
        assert issubclass(exc_type, fraiseql.FraiseQLError), (
            f"{exc_type.__name__} is not catchable as fraiseql.FraiseQLError"
        )


def test_sync_client_errors_are_package_fraiseql_errors():
    """Everything the sync FraiseQLClient classifies is a fraiseql.FraiseQLError."""
    for exc_type in (
        fraiseql.FraiseQLAuthError,
        fraiseql.FraiseQLUnsupportedError,
        fraiseql.FraiseQLRateLimitError,
        fraiseql.FraiseQLDatabaseError,
    ):
        assert issubclass(exc_type, fraiseql.FraiseQLError)


def test_single_base_class_object():
    """The sync and async sides share ONE base class object, not two same-named ones."""
    assert client_mod.FraiseQLError is errors_mod.FraiseQLError
    assert fraiseql.FraiseQLError is errors_mod.FraiseQLError


def test_catch_all_actually_catches_async_graphql_error():
    """The documented `except fraiseql.FraiseQLError` catches an async GraphQLError."""
    raised = fraiseql.GraphQLError([{"message": "boom"}])
    try:
        raise raised
    except fraiseql.FraiseQLError as caught:
        assert caught is raised
    else:  # pragma: no cover - only reached on regression
        raise AssertionError("fraiseql.FraiseQLError did not catch GraphQLError")
