import pytest

"""Test that GraphQL info is properly passed to repository context."""

from fraiseql.db import FraiseQLRepository


@pytest.mark.unit
def test_repository_context_preserved() -> None:
    """Test that repository context can be updated with GraphQL info."""
    # Create repository with initial context
    repo_context = {"mode": "production", "query_timeout": 30, "custom": "value"}
    repo = FraiseQLRepository(None, repo_context)

    # Simulate GraphQL info being added (what the fixed query builder will do)
    repo.context["graphql_info"] = "test_info"
    repo.context["graphql_field_name"] = "testField"

    # Original context should still be there
    assert repo.context["mode"] == "production"
    assert repo.context["query_timeout"] == 30
    assert repo.context["custom"] == "value"

    # New context should be added
    assert repo.context["graphql_info"] == "test_info"
    assert repo.context["graphql_field_name"] == "testField"


def test_repository_context_is_mutable() -> None:
    """Test that repository context is a mutable dict."""
    repo = FraiseQLRepository(None, {"initial": "value"})

    # Should be able to add new keys
    repo.context["new_key"] = "new_value"
    assert repo.context["new_key"] == "new_value"

    # Should be able to update existing keys
    repo.context["initial"] = "updated"
    assert repo.context["initial"] == "updated"

    # Should have both keys
    assert len(repo.context) == 2
