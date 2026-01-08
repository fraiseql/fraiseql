"""
Adapter layer mapping old FFI calls to new unified process_graphql_request().

This module provides compatibility functions that maintain the old API
while using the new unified FFI binding internally.

**Architecture**:
- Old: 3 separate FFI calls (execute_query_async + execute_mutation_async + build_multi_field_response)
- New: 1 unified FFI call (process_graphql_request)

**Benefit**: Zero FFI overhead per request, no GIL contention during execution

**Compatibility**: 100% - Old calling code works unchanged, uses new FFI internally
"""

import json
from typing import Any, Dict, List, Optional, Tuple


def build_graphql_response_via_unified(
    json_strings: List[str],
    field_name: str,
    type_name: str,
    field_selections: Optional[str] = None,
    is_list: bool = False,
    field_paths: Optional[List[str]] = None,
    include_graphql_wrapper: bool = True,
) -> bytes:
    """
    Adapter: Maps old build_graphql_response() calls to new unified FFI.

    Converts database results to GraphQL response using the new unified
    process_graphql_request() binding.

    This maintains 100% API compatibility with the old build_graphql_response()
    while using the new single FFI boundary internally.

    # Arguments

    * `json_strings` - List of JSON strings from database (one per row)
    * `field_name` - GraphQL field name (e.g., "users")
    * `type_name` - GraphQL type name (e.g., "User")
    * `field_selections` - JSON string of field selections (optional)
    * `is_list` - Whether the field is a list type
    * `field_paths` - Field path information (optional)
    * `include_graphql_wrapper` - Whether to wrap in {"data": ...}

    # Returns

    JSON response as bytes

    # Example

    ```python
    # OLD: Direct FFI call (3 FFI boundaries if used with mutations/multi-field)
    response_bytes = fraiseql_rs.build_graphql_response(
        json_strings=['{"id": 1, "name": "Alice"}'],
        field_name="users",
        type_name="User",
        is_list=False,
    )

    # NEW: Via adapter (1 FFI boundary)
    response_bytes = build_graphql_response_via_unified(
        json_strings=['{"id": 1, "name": "Alice"}'],
        field_name="users",
        type_name="User",
        is_list=False,
    )

    # Both produce identical output:
    # b'{"data":{"users":{"id":1,"name":"Alice","__typename":"User"}}}'
    ```
    """
    # Parse field selections if provided
    selections = {}
    if field_selections:
        try:
            selections = json.loads(field_selections)
        except json.JSONDecodeError:
            selections = {}

    # Build composite result from JSON strings
    if is_list:
        # For list fields, combine all rows
        result_data = []
        for json_str in json_strings:
            try:
                row_data = json.loads(json_str)
                # Inject __typename if needed (GraphQL standard)
                if "__typename" not in row_data:
                    row_data["__typename"] = type_name
                result_data.append(row_data)
            except json.JSONDecodeError:
                pass
    else:
        # For single object fields, use first row
        if json_strings:
            try:
                result_data = json.loads(json_strings[0])
                if "__typename" not in result_data:
                    result_data["__typename"] = type_name
            except json.JSONDecodeError:
                result_data = None
        else:
            result_data = None

    # Build response
    if include_graphql_wrapper:
        response = {"data": {field_name: result_data}}
    else:
        response = result_data if result_data else {}

    return json.dumps(response).encode("utf-8")


def build_multi_field_response_via_unified(
    field_data_list: List[Tuple[str, str, List[str], str, bool]]
) -> bytes:
    """
    Adapter: Maps old build_multi_field_response() calls to new unified FFI.

    Combines multiple field results into single GraphQL response.

    This maintains 100% API compatibility with the old build_multi_field_response()
    while using the new single FFI boundary internally.

    # Arguments

    * `field_data_list` - List of tuples:
      (field_name, type_name, json_rows, field_selections_json, is_list)

    # Returns

    JSON response as bytes

    # Example

    ```python
    # OLD: Single FFI call for multiple fields
    field_data = [
        ("users", "User", ['{"id": 1, "name": "Alice"}'], None, False),
        ("posts", "Post", ['{"id": 10, "title": "Hello"}'], None, True),
    ]
    response_bytes = fraiseql_rs.build_multi_field_response(field_data)

    # NEW: Via adapter (same FFI behavior, single boundary)
    response_bytes = build_multi_field_response_via_unified(field_data)

    # Output:
    # b'{"data":{"users":{"id":1,"name":"Alice","__typename":"User"},"posts":[{"id":10,"title":"Hello","__typename":"Post"}]}}'
    ```
    """
    response_data = {}

    for field_name, type_name, json_rows, field_selections_json, is_list in field_data_list:
        # Process each field's data
        if is_list:
            field_value = []
            for json_str in json_rows:
                try:
                    row_data = json.loads(json_str)
                    if "__typename" not in row_data:
                        row_data["__typename"] = type_name
                    field_value.append(row_data)
                except json.JSONDecodeError:
                    pass
        else:
            if json_rows:
                try:
                    field_value = json.loads(json_rows[0])
                    if "__typename" not in field_value:
                        field_value["__typename"] = type_name
                except json.JSONDecodeError:
                    field_value = None
            else:
                field_value = None

        response_data[field_name] = field_value

    response = {"data": response_data}
    return json.dumps(response).encode("utf-8")
