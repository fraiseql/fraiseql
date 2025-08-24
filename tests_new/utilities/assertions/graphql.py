"""GraphQL-specific assertion helpers for FraiseQL testing.

This module provides specialized assertion utilities for GraphQL responses,
including error checking, data validation, schema compliance verification,
and common GraphQL patterns validation.
"""

import json
from typing import Any, Dict, List, Optional, Union


def assert_graphql_response_valid(response: Dict[str, Any]) -> None:
    """Assert that GraphQL response has valid structure.

    Args:
        response: GraphQL response dictionary

    Raises:
        AssertionError: If response structure is invalid
    """
    assert isinstance(response, dict), f"Response must be dict, got {type(response)}"

    # Must have either data or errors (or both)
    has_data = "data" in response
    has_errors = "errors" in response

    assert has_data or has_errors, "Response must contain 'data' or 'errors'"

    # Validate data structure if present
    if has_data:
        assert response["data"] is None or isinstance(response["data"], dict), (
            f"Data must be dict or None, got {type(response['data'])}"
        )

    # Validate errors structure if present
    if has_errors:
        assert isinstance(response["errors"], list), (
            f"Errors must be list, got {type(response['errors'])}"
        )

        for error in response["errors"]:
            assert isinstance(error, dict), f"Error must be dict, got {type(error)}"
            assert "message" in error, "Error must have 'message' field"


def assert_no_graphql_errors(response: Dict[str, Any]) -> None:
    """Assert GraphQL response has no errors.

    Args:
        response: GraphQL response dictionary

    Raises:
        AssertionError: If response contains errors
    """
    assert_graphql_response_valid(response)

    if "errors" in response:
        error_messages = [error.get("message", "Unknown error") for error in response["errors"]]
        raise AssertionError(f"GraphQL response contains errors: {error_messages}")


def assert_graphql_errors_present(response: Dict[str, Any], count: Optional[int] = None) -> None:
    """Assert GraphQL response contains errors.

    Args:
        response: GraphQL response dictionary
        count: Expected number of errors (if specified)

    Raises:
        AssertionError: If response has no errors or wrong count
    """
    assert_graphql_response_valid(response)

    assert "errors" in response, "Expected GraphQL errors but found none"

    if count is not None:
        actual_count = len(response["errors"])
        assert actual_count == count, (
            f"Expected {count} errors, got {actual_count}: {response['errors']}"
        )


def assert_graphql_error_message(
    response: Dict[str, Any], expected_message: str, exact: bool = False
) -> None:
    """Assert GraphQL response contains specific error message.

    Args:
        response: GraphQL response dictionary
        expected_message: Expected error message
        exact: Whether to match exactly or just contain the message

    Raises:
        AssertionError: If error message not found
    """
    assert_graphql_errors_present(response)

    error_messages = [error.get("message", "") for error in response["errors"]]

    if exact:
        assert expected_message in error_messages, (
            f"Exact error message '{expected_message}' not found in: {error_messages}"
        )
    else:
        found = any(expected_message in msg for msg in error_messages)
        assert found, (
            f"Error message containing '{expected_message}' not found in: {error_messages}"
        )


def assert_graphql_error_path(
    response: Dict[str, Any], expected_path: List[Union[str, int]]
) -> None:
    """Assert GraphQL error has expected path.

    Args:
        response: GraphQL response dictionary
        expected_path: Expected error path (e.g., ["user", "profile", 0, "email"])

    Raises:
        AssertionError: If error path not found
    """
    assert_graphql_errors_present(response)

    error_paths = [error.get("path") for error in response["errors"] if "path" in error]

    assert expected_path in error_paths, f"Error path {expected_path} not found in: {error_paths}"


def assert_graphql_error_extensions(
    response: Dict[str, Any], expected_extensions: Dict[str, Any]
) -> None:
    """Assert GraphQL error has expected extensions.

    Args:
        response: GraphQL response dictionary
        expected_extensions: Expected error extensions

    Raises:
        AssertionError: If extensions not found
    """
    assert_graphql_errors_present(response)

    for error in response["errors"]:
        extensions = error.get("extensions", {})

        for key, expected_value in expected_extensions.items():
            assert key in extensions, (
                f"Extension key '{key}' not found in error extensions: {extensions}"
            )

            actual_value = extensions[key]
            assert actual_value == expected_value, (
                f"Extension '{key}': expected {expected_value}, got {actual_value}"
            )


def assert_graphql_data_equals(response: Dict[str, Any], expected_data: Dict[str, Any]) -> None:
    """Assert GraphQL response data equals expected data.

    Args:
        response: GraphQL response dictionary
        expected_data: Expected data structure

    Raises:
        AssertionError: If data doesn't match
    """
    assert_no_graphql_errors(response)

    actual_data = response.get("data")
    assert actual_data == expected_data, (
        f"Data mismatch:\nExpected: {json.dumps(expected_data, indent=2)}\nActual: {json.dumps(actual_data, indent=2)}"
    )


def assert_graphql_field_equals(
    response: Dict[str, Any], field_path: str, expected_value: Any
) -> None:
    """Assert specific field in GraphQL response equals expected value.

    Args:
        response: GraphQL response dictionary
        field_path: Dot-separated path to field (e.g., "user.profile.email")
        expected_value: Expected field value

    Raises:
        AssertionError: If field doesn't match
    """
    assert_no_graphql_errors(response)

    data = response.get("data", {})
    fields = field_path.split(".")

    current = data
    for i, field in enumerate(fields):
        current_path = ".".join(fields[: i + 1])

        if isinstance(current, list):
            try:
                field_index = int(field)
                assert 0 <= field_index < len(current), (
                    f"Array index {field_index} out of bounds at path '{current_path}'"
                )
                current = current[field_index]
            except ValueError:
                raise AssertionError(
                    f"Expected array index but got '{field}' at path '{current_path}'"
                )
        else:
            assert isinstance(current, dict), (
                f"Expected dict at path '{current_path}', got {type(current)}"
            )
            assert field in current, (
                f"Field '{field}' not found at path '{current_path}'. Available: {list(current.keys())}"
            )
            current = current[field]

    assert current == expected_value, (
        f"Field '{field_path}': expected {expected_value}, got {current}"
    )


def assert_graphql_field_exists(response: Dict[str, Any], field_path: str) -> None:
    """Assert field exists in GraphQL response.

    Args:
        response: GraphQL response dictionary
        field_path: Dot-separated path to field

    Raises:
        AssertionError: If field doesn't exist
    """
    assert_no_graphql_errors(response)

    data = response.get("data", {})
    fields = field_path.split(".")

    current = data
    for field in fields:
        if isinstance(current, list):
            try:
                field_index = int(field)
                assert 0 <= field_index < len(current), (
                    f"Array index {field_index} out of bounds in path '{field_path}'"
                )
                current = current[field_index]
            except ValueError:
                raise AssertionError(
                    f"Expected array index but got '{field}' in path '{field_path}'"
                )
        else:
            assert isinstance(current, dict), (
                f"Expected dict but got {type(current)} in path '{field_path}'"
            )
            assert field in current, (
                f"Field '{field}' not found in path '{field_path}'. Available: {list(current.keys())}"
            )
            current = current[field]


def assert_graphql_field_type(
    response: Dict[str, Any], field_path: str, expected_type: type
) -> None:
    """Assert field type in GraphQL response.

    Args:
        response: GraphQL response dictionary
        field_path: Dot-separated path to field
        expected_type: Expected Python type

    Raises:
        AssertionError: If field type doesn't match
    """
    assert_graphql_field_exists(response, field_path)

    data = response.get("data", {})
    fields = field_path.split(".")

    current = data
    for field in fields:
        if isinstance(current, list):
            current = current[int(field)]
        else:
            current = current[field]

    assert isinstance(current, expected_type), (
        f"Field '{field_path}' type: expected {expected_type.__name__}, got {type(current).__name__}"
    )


def assert_graphql_list_length(
    response: Dict[str, Any], field_path: str, expected_length: int
) -> None:
    """Assert list field has expected length.

    Args:
        response: GraphQL response dictionary
        field_path: Dot-separated path to list field
        expected_length: Expected list length

    Raises:
        AssertionError: If list length doesn't match
    """
    assert_graphql_field_type(response, field_path, list)

    data = response.get("data", {})
    fields = field_path.split(".")

    current = data
    for field in fields:
        current = current[field]

    actual_length = len(current)
    assert actual_length == expected_length, (
        f"List '{field_path}' length: expected {expected_length}, got {actual_length}"
    )


def assert_graphql_typename(
    response: Dict[str, Any], field_path: str, expected_typename: str
) -> None:
    """Assert __typename field matches expected value.

    Args:
        response: GraphQL response dictionary
        field_path: Dot-separated path to object with __typename
        expected_typename: Expected __typename value

    Raises:
        AssertionError: If __typename doesn't match
    """
    typename_path = f"{field_path}.__typename"
    assert_graphql_field_equals(response, typename_path, expected_typename)


def assert_mutation_success(
    response: Dict[str, Any], mutation_name: str, success_type: Optional[str] = None
) -> None:
    """Assert mutation succeeded (for union result types).

    Args:
        response: GraphQL response dictionary
        mutation_name: Name of the mutation field
        success_type: Expected success type name (if using union results)

    Raises:
        AssertionError: If mutation failed
    """
    assert_no_graphql_errors(response)
    assert_graphql_field_exists(response, mutation_name)

    if success_type:
        assert_graphql_typename(response, mutation_name, success_type)


def assert_mutation_error(
    response: Dict[str, Any],
    mutation_name: str,
    error_type: Optional[str] = None,
    error_message: Optional[str] = None,
) -> None:
    """Assert mutation failed (for union result types).

    Args:
        response: GraphQL response dictionary
        mutation_name: Name of the mutation field
        error_type: Expected error type name (if using union results)
        error_message: Expected error message

    Raises:
        AssertionError: If mutation succeeded or wrong error
    """
    # For union result types, mutation should succeed but return error type
    assert_no_graphql_errors(response)
    assert_graphql_field_exists(response, mutation_name)

    if error_type:
        assert_graphql_typename(response, mutation_name, error_type)

    if error_message:
        message_path = f"{mutation_name}.message"
        assert_graphql_field_exists(response, message_path)

        actual_message = response["data"][mutation_name]["message"]
        assert error_message in actual_message, (
            f"Error message '{error_message}' not found in: {actual_message}"
        )


def assert_pagination_response(
    response: Dict[str, Any], field_path: str, has_items: bool = True, has_page_info: bool = True
) -> None:
    """Assert response has valid pagination structure.

    Args:
        response: GraphQL response dictionary
        field_path: Path to paginated field
        has_items: Whether items should be present
        has_page_info: Whether pageInfo should be present

    Raises:
        AssertionError: If pagination structure is invalid
    """
    assert_no_graphql_errors(response)
    assert_graphql_field_exists(response, field_path)

    if has_items:
        items_path = f"{field_path}.edges"
        assert_graphql_field_type(response, items_path, list)

    if has_page_info:
        page_info_path = f"{field_path}.pageInfo"
        assert_graphql_field_exists(response, page_info_path)

        # Check required pageInfo fields
        for field in ["hasNextPage", "hasPreviousPage"]:
            field_path_full = f"{page_info_path}.{field}"
            assert_graphql_field_type(response, field_path_full, bool)


def assert_connection_response(
    response: Dict[str, Any], field_path: str, min_count: int = 0, max_count: Optional[int] = None
) -> None:
    """Assert response follows Relay connection specification.

    Args:
        response: GraphQL response dictionary
        field_path: Path to connection field
        min_count: Minimum expected edge count
        max_count: Maximum expected edge count (if specified)

    Raises:
        AssertionError: If connection structure is invalid
    """
    assert_pagination_response(response, field_path)

    edges_path = f"{field_path}.edges"
    assert_graphql_field_type(response, edges_path, list)

    # Get actual edge count
    edges = response["data"]
    for field in edges_path.split("."):
        edges = edges[field]

    edge_count = len(edges)

    assert edge_count >= min_count, (
        f"Connection '{field_path}' edge count: expected >= {min_count}, got {edge_count}"
    )

    if max_count is not None:
        assert edge_count <= max_count, (
            f"Connection '{field_path}' edge count: expected <= {max_count}, got {edge_count}"
        )

    # Validate edge structure
    for i, edge in enumerate(edges):
        assert "node" in edge, f"Edge {i} missing 'node' field"
        assert "cursor" in edge, f"Edge {i} missing 'cursor' field"
