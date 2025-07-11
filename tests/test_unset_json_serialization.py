"""Test UNSET value handling in JSON serialization."""

import json

import fraiseql
from fraiseql.fastapi.json_encoder import FraiseQLJSONEncoder, FraiseQLJSONResponse, clean_unset_values
from fraiseql.types.definitions import UNSET


def test_fraiseql_json_encoder_handles_unset():
    """Test that FraiseQLJSONEncoder properly serializes UNSET values."""
    encoder = FraiseQLJSONEncoder()

    # Test UNSET value directly
    assert encoder.encode(UNSET) == "null"

    # Test UNSET in a dictionary
    data = {"field1": "value", "field2": UNSET, "field3": None}
    result = json.loads(encoder.encode(data))

    assert result == {"field1": "value", "field2": None, "field3": None}


def test_fraiseql_json_encoder_handles_nested_unset():
    """Test that FraiseQLJSONEncoder handles UNSET in nested structures."""
    encoder = FraiseQLJSONEncoder()

    # Test nested structure with UNSET
    data = {
        "user": {
            "id": "123",
            "name": "John",
            "email": UNSET,
            "profile": {
                "bio": "Developer",
                "avatar": UNSET,
            },
        },
        "metadata": UNSET,
    }

    result = json.loads(encoder.encode(data))

    expected = {
        "user": {
            "id": "123",
            "name": "John",
            "email": None,
            "profile": {
                "bio": "Developer",
                "avatar": None,
            },
        },
        "metadata": None,
    }

    assert result == expected


def test_fraiseql_json_encoder_handles_unset_in_lists():
    """Test that FraiseQLJSONEncoder handles UNSET in list structures."""
    encoder = FraiseQLJSONEncoder()

    data = {
        "items": [
            {"id": 1, "value": "test"},
            {"id": 2, "value": UNSET},
            UNSET,
            {"id": 3, "value": None},
        ],
    }

    result = json.loads(encoder.encode(data))

    expected = {
        "items": [
            {"id": 1, "value": "test"},
            {"id": 2, "value": None},
            None,
            {"id": 3, "value": None},
        ],
    }

    assert result == expected


def test_fraiseql_json_response_renders_unset():
    """Test that FraiseQLJSONResponse properly renders UNSET values."""
    content = {
        "data": {
            "field1": "value",
            "field2": UNSET,
        },
        "errors": None,
    }

    response = FraiseQLJSONResponse(content=content)

    # Get the rendered content
    rendered = response.render(content)
    result = json.loads(rendered.decode("utf-8"))

    expected = {
        "data": {
            "field1": "value",
            "field2": None,
        },
        "errors": None,
    }

    assert result == expected


def test_graphql_error_response_with_unset():
    """Test that GraphQL error responses can include UNSET values."""
    # Simulate a GraphQL error response that might include input with UNSET values
    error_response = {
        "data": None,
        "errors": [
            {
                "message": "Validation failed",
                "locations": [{"line": 1, "column": 1}],
                "path": ["createItem"],
                "extensions": {
                    "input": {
                        "required_field": "value",
                        "optional_field": UNSET,
                        "another_field": None,
                    },
                    "details": "Some error details",
                },
            },
        ],
    }

    encoder = FraiseQLJSONEncoder()
    result = json.loads(encoder.encode(error_response))

    expected = {
        "data": None,
        "errors": [
            {
                "message": "Validation failed",
                "locations": [{"line": 1, "column": 1}],
                "path": ["createItem"],
                "extensions": {
                    "input": {
                        "required_field": "value",
                        "optional_field": None,  # UNSET converted to None
                        "another_field": None,
                    },
                    "details": "Some error details",
                },
            },
        ],
    }

    assert result == expected


@fraiseql.input
class TestInputWithUnset:
    """Test input type with UNSET defaults."""

    required_field: str
    optional_with_unset: str | None = UNSET
    optional_with_none: str | None = None


def test_input_object_with_unset_serialization():
    """Test that input objects with UNSET fields serialize correctly."""
    # Create an input object where some fields have UNSET values
    input_obj = TestInputWithUnset(
        required_field="test",
        # optional_with_unset gets UNSET default
        optional_with_none=None,
    )

    # Convert to dict to simulate what might happen in error responses
    input_dict = {
        "required_field": input_obj.required_field,
        "optional_with_unset": input_obj.optional_with_unset,
        "optional_with_none": input_obj.optional_with_none,
    }

    encoder = FraiseQLJSONEncoder()
    result = json.loads(encoder.encode(input_dict))

    expected = {
        "required_field": "test",
        "optional_with_unset": None,  # UNSET converted to None
        "optional_with_none": None,
    }

    assert result == expected


def test_clean_unset_values_function():
    """Test that clean_unset_values utility function works correctly."""
    # Test simple cases
    assert clean_unset_values(UNSET) is None
    assert clean_unset_values("test") == "test"
    assert clean_unset_values(None) is None

    # Test dict with UNSET values
    data = {
        "field1": "value",
        "field2": UNSET,
        "field3": None,
        "nested": {
            "inner1": "value",
            "inner2": UNSET,
        },
    }
    
    cleaned = clean_unset_values(data)
    expected = {
        "field1": "value",
        "field2": None,
        "field3": None,
        "nested": {
            "inner1": "value",
            "inner2": None,
        },
    }
    
    assert cleaned == expected

    # Test list with UNSET values
    list_data = ["test", UNSET, None, {"field": UNSET}]
    cleaned_list = clean_unset_values(list_data)
    expected_list = ["test", None, None, {"field": None}]
    
    assert cleaned_list == expected_list


if __name__ == "__main__":
    test_fraiseql_json_encoder_handles_unset()
    print("✓ test_fraiseql_json_encoder_handles_unset passed")

    test_fraiseql_json_encoder_handles_nested_unset()
    print("✓ test_fraiseql_json_encoder_handles_nested_unset passed")

    test_fraiseql_json_encoder_handles_unset_in_lists()
    print("✓ test_fraiseql_json_encoder_handles_unset_in_lists passed")

    test_fraiseql_json_response_renders_unset()
    print("✓ test_fraiseql_json_response_renders_unset passed")

    test_graphql_error_response_with_unset()
    print("✓ test_graphql_error_response_with_unset passed")

    test_input_object_with_unset_serialization()
    print("✓ test_input_object_with_unset_serialization passed")

    test_clean_unset_values_function()
    print("✓ test_clean_unset_values_function passed")

    print("\nAll tests passed!")
