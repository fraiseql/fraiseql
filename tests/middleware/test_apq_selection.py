"""Tests for APQ selection set extraction and response filtering."""

import pytest

from fraiseql.middleware.apq_selection import (
    extract_fragments,
    extract_selection_set,
    filter_response_by_selection,
)

pytestmark = pytest.mark.unit


class TestExtractSelectionSet:
    """Tests for extract_selection_set function."""

    def test_extract_simple_query(self) -> None:
        """Test extracting selection from a simple query."""
        query = "query { user { id name } }"
        selection = extract_selection_set(query)

        assert selection is not None
        assert len(selection.selections) == 1  # user field

    def test_extract_query_with_arguments(self) -> None:
        """Test extracting selection from query with arguments."""
        query = "query { user(id: 1) { id name email } }"
        selection = extract_selection_set(query)

        assert selection is not None
        assert len(selection.selections) == 1

    def test_extract_named_query(self) -> None:
        """Test extracting selection from named query."""
        query = "query GetUser { user { id name } }"
        selection = extract_selection_set(query)

        assert selection is not None

    def test_extract_with_operation_name(self) -> None:
        """Test extracting selection by operation name."""
        query = """
            query GetUser { user { id } }
            query GetPosts { posts { title } }
        """
        # Get first operation (GetUser)
        selection = extract_selection_set(query, "GetUser")
        assert selection is not None

        # Get second operation (GetPosts)
        selection = extract_selection_set(query, "GetPosts")
        assert selection is not None

    def test_extract_with_wrong_operation_name(self) -> None:
        """Test that wrong operation name returns None."""
        query = "query GetUser { user { id } }"
        selection = extract_selection_set(query, "WrongName")

        assert selection is None

    def test_extract_mutation(self) -> None:
        """Test extracting selection from mutation."""
        query = 'mutation CreateUser { createUser(name: "John") { id } }'
        selection = extract_selection_set(query)

        assert selection is not None

    def test_extract_invalid_query(self) -> None:
        """Test that invalid query returns None."""
        query = "this is not valid graphql {"
        selection = extract_selection_set(query)

        assert selection is None

    def test_extract_empty_query(self) -> None:
        """Test that empty query returns None."""
        selection = extract_selection_set("")

        assert selection is None


class TestExtractFragments:
    """Tests for extract_fragments function."""

    def test_extract_single_fragment(self) -> None:
        """Test extracting a single fragment."""
        query = """
            query { user { ...UserFields } }
            fragment UserFields on User { id name }
        """
        fragments = extract_fragments(query)

        assert "UserFields" in fragments
        assert fragments["UserFields"].selection_set is not None

    def test_extract_multiple_fragments(self) -> None:
        """Test extracting multiple fragments."""
        query = """
            query { user { ...UserFields ...ProfileFields } }
            fragment UserFields on User { id name }
            fragment ProfileFields on User { bio avatar }
        """
        fragments = extract_fragments(query)

        assert len(fragments) == 2
        assert "UserFields" in fragments
        assert "ProfileFields" in fragments

    def test_extract_no_fragments(self) -> None:
        """Test query without fragments."""
        query = "query { user { id name } }"
        fragments = extract_fragments(query)

        assert len(fragments) == 0

    def test_extract_fragments_invalid_query(self) -> None:
        """Test that invalid query returns empty dict."""
        fragments = extract_fragments("invalid query {")

        assert len(fragments) == 0


class TestFilterResponseBySelection:
    """Tests for filter_response_by_selection function."""

    def test_filter_simple_response(self) -> None:
        """Test filtering a simple response."""
        response = {"data": {"user": {"id": 1, "name": "John", "email": "john@test.com"}}}
        query = "query { user { id name } }"
        selection = extract_selection_set(query)

        assert selection is not None
        filtered = filter_response_by_selection(response, selection)

        assert filtered["data"]["user"]["id"] == 1
        assert filtered["data"]["user"]["name"] == "John"
        assert "email" not in filtered["data"]["user"]

    def test_filter_nested_response(self) -> None:
        """Test filtering nested objects."""
        response = {
            "data": {
                "user": {
                    "id": 1,
                    "profile": {"bio": "Hello", "avatar": "url", "private": True},
                }
            }
        }
        query = "query { user { id profile { bio } } }"
        selection = extract_selection_set(query)

        assert selection is not None
        filtered = filter_response_by_selection(response, selection)

        assert filtered["data"]["user"]["id"] == 1
        assert filtered["data"]["user"]["profile"]["bio"] == "Hello"
        assert "avatar" not in filtered["data"]["user"]["profile"]
        assert "private" not in filtered["data"]["user"]["profile"]

    def test_filter_list_response(self) -> None:
        """Test filtering lists of objects."""
        response = {
            "data": {
                "users": [
                    {"id": 1, "name": "John", "email": "john@test.com"},
                    {"id": 2, "name": "Jane", "email": "jane@test.com"},
                ]
            }
        }
        query = "query { users { id name } }"
        selection = extract_selection_set(query)

        assert selection is not None
        filtered = filter_response_by_selection(response, selection)

        assert len(filtered["data"]["users"]) == 2
        assert filtered["data"]["users"][0]["id"] == 1
        assert filtered["data"]["users"][0]["name"] == "John"
        assert "email" not in filtered["data"]["users"][0]
        assert "email" not in filtered["data"]["users"][1]

    def test_filter_with_alias(self) -> None:
        """Test filtering with field aliases."""
        response = {"data": {"myUser": {"id": 1, "name": "John", "email": "x"}}}
        query = "query { myUser: user { id name } }"
        selection = extract_selection_set(query)

        assert selection is not None
        filtered = filter_response_by_selection(response, selection)

        assert filtered["data"]["myUser"]["id"] == 1
        assert filtered["data"]["myUser"]["name"] == "John"
        assert "email" not in filtered["data"]["myUser"]

    def test_filter_preserves_null_data(self) -> None:
        """Test that null data is preserved."""
        response = {"data": None}
        query = "query { user { id } }"
        selection = extract_selection_set(query)

        assert selection is not None
        filtered = filter_response_by_selection(response, selection)

        assert filtered["data"] is None

    def test_filter_preserves_extensions(self) -> None:
        """Test that extensions are preserved."""
        response = {
            "data": {"user": {"id": 1, "name": "John", "email": "x"}},
            "extensions": {"timing": 100},
        }
        query = "query { user { id name } }"
        selection = extract_selection_set(query)

        assert selection is not None
        filtered = filter_response_by_selection(response, selection)

        assert "extensions" in filtered
        assert filtered["extensions"]["timing"] == 100
        assert "email" not in filtered["data"]["user"]

    def test_filter_response_without_data(self) -> None:
        """Test filtering response without data key."""
        response = {"errors": [{"message": "Error"}]}
        query = "query { user { id } }"
        selection = extract_selection_set(query)

        assert selection is not None
        filtered = filter_response_by_selection(response, selection)

        # Should return unchanged
        assert filtered == response

    def test_filter_with_fragment_spread(self) -> None:
        """Test filtering with fragment spread."""
        response = {"data": {"user": {"id": 1, "name": "John", "email": "x", "age": 30}}}
        query = """
            query { user { ...UserFields email } }
            fragment UserFields on User { id name }
        """
        selection = extract_selection_set(query)
        fragments = extract_fragments(query)

        assert selection is not None
        filtered = filter_response_by_selection(response, selection, fragments)

        assert filtered["data"]["user"]["id"] == 1
        assert filtered["data"]["user"]["name"] == "John"
        assert filtered["data"]["user"]["email"] == "x"
        assert "age" not in filtered["data"]["user"]

    def test_filter_deeply_nested(self) -> None:
        """Test filtering deeply nested structures."""
        response = {
            "data": {
                "company": {
                    "id": 1,
                    "name": "Acme",
                    "address": {
                        "street": "123 Main",
                        "city": "NYC",
                        "country": {
                            "name": "USA",
                            "code": "US",
                            "population": 330000000,
                        },
                    },
                }
            }
        }
        query = """
            query {
                company {
                    id
                    address {
                        city
                        country { name }
                    }
                }
            }
        """
        selection = extract_selection_set(query)

        assert selection is not None
        filtered = filter_response_by_selection(response, selection)

        assert filtered["data"]["company"]["id"] == 1
        assert "name" not in filtered["data"]["company"]
        assert "street" not in filtered["data"]["company"]["address"]
        assert filtered["data"]["company"]["address"]["city"] == "NYC"
        assert filtered["data"]["company"]["address"]["country"]["name"] == "USA"
        assert "code" not in filtered["data"]["company"]["address"]["country"]
        assert "population" not in filtered["data"]["company"]["address"]["country"]

    def test_filter_empty_nested_list(self) -> None:
        """Test filtering with empty nested lists."""
        response = {"data": {"users": []}}
        query = "query { users { id name } }"
        selection = extract_selection_set(query)

        assert selection is not None
        filtered = filter_response_by_selection(response, selection)

        assert filtered["data"]["users"] == []

    def test_filter_scalar_values_in_list(self) -> None:
        """Test that scalar values in lists pass through."""
        response = {"data": {"tags": ["a", "b", "c"]}}
        query = "query { tags }"
        selection = extract_selection_set(query)

        assert selection is not None
        filtered = filter_response_by_selection(response, selection)

        assert filtered["data"]["tags"] == ["a", "b", "c"]
