import pytest

"""Test UNSET value handling in JSON serialization."""

import ipaddress
import json
import uuid
from dataclasses import dataclass
from datetime import datetime

import fraiseql
from fraiseql.fastapi.json_encoder import (
    FraiseQLJSONEncoder,
    FraiseQLJSONResponse,
    clean_unset_values,
)
from fraiseql.types.definitions import UNSET


@pytest.mark.unit
def test_fraiseql_json_encoder_handles_unset() -> None:
    """Test that FraiseQLJSONEncoder properly serializes UNSET values."""
    encoder = FraiseQLJSONEncoder()

    # Test UNSET value directly
    assert encoder.encode(UNSET) == "null"

    # Test UNSET in a dictionary
    data = {"field1": "value", "field2": UNSET, "field3": None}
    result = json.loads(encoder.encode(data))

    assert result == {"field1": "value", "field2": None, "field3": None}


def test_fraiseql_json_encoder_handles_nested_unset() -> None:
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


def test_fraiseql_json_encoder_handles_unset_in_lists() -> None:
    """Test that FraiseQLJSONEncoder handles UNSET in list structures."""
    encoder = FraiseQLJSONEncoder()

    data = {
        "items": [
            {"id": 1, "value": "test"},
            {"id": 2, "value": UNSET},
            UNSET,
            {"id": 3, "value": None},
        ]
    }

    result = json.loads(encoder.encode(data))

    expected = {
        "items": [
            {"id": 1, "value": "test"},
            {"id": 2, "value": None},
            None,
            {"id": 3, "value": None},
        ]
    }

    assert result == expected


def test_fraiseql_json_response_renders_unset() -> None:
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


def test_graphql_error_response_with_unset() -> None:
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
            }
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
            }
        ],
    }

    assert result == expected


@fraiseql.input
class SampleInputWithUnset:
    """Test input type with UNSET defaults."""

    required_field: str
    optional_with_unset: str | None = UNSET
    optional_with_none: str | None = None


def test_input_object_with_unset_serialization() -> None:
    """Test that input objects with UNSET fields serialize correctly."""
    # Create an input object where some fields have UNSET values
    input_obj = SampleInputWithUnset(
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


def test_clean_unset_values_function() -> None:
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


def test_fraiseql_json_encoder_handles_ipaddress() -> None:
    """Test that FraiseQLJSONEncoder properly serializes IPv4Address and IPv6Address objects."""
    encoder = FraiseQLJSONEncoder()

    # Test IPv4Address
    ipv4 = ipaddress.IPv4Address("192.168.1.1")
    assert encoder.encode(ipv4) == '"192.168.1.1"'

    # Test IPv6Address
    ipv6 = ipaddress.IPv6Address("2001:db8::1")
    assert encoder.encode(ipv6) == '"2001:db8::1"'

    # Test in a dictionary (simulating mutation response)
    data = {
        "dns_server": {
            "id": "123",
            "name": "Primary DNS",
            "ip_address": ipaddress.IPv4Address("8.8.8.8"),
        }
    }
    result = json.loads(encoder.encode(data))
    assert result["dns_server"]["ip_address"] == "8.8.8.8"

    # Test in a list
    data_list = {
        "servers": [
            {"name": "DNS1", "ip": ipaddress.IPv4Address("8.8.8.8")},
            {"name": "DNS2", "ip": ipaddress.IPv6Address("2001:db8::2")},
        ]
    }
    result_list = json.loads(encoder.encode(data_list))
    assert result_list["servers"][0]["ip"] == "8.8.8.8"
    assert result_list["servers"][1]["ip"] == "2001:db8::2"


def test_fraiseql_json_response_with_ipaddress() -> None:
    """Test that FraiseQLJSONResponse properly handles IPv4Address objects.

    This test reproduces the bug reported in FRAISEQL_IPV4ADDRESS_JSON_SERIALIZATION_BUG.md
    """
    # Simulate a mutation response with IPv4Address objects
    content = {
        "data": {
            "createDnsServer": {
                "__typename": "CreateDnsServerSuccess",
                "status": "ok",
                "message": "DNS server created",
                "dnsServer": {
                    "id": "123",
                    "name": "Primary DNS",
                    "ipAddress": ipaddress.IPv4Address("192.168.1.1"),
                },
            }
        },
        "errors": None,
    }

    response = FraiseQLJSONResponse(content=content)

    # Get the rendered content
    rendered = response.render(content)
    result = json.loads(rendered.decode("utf-8"))

    # The IPv4Address should be serialized as a string
    assert result["data"]["createDnsServer"]["dnsServer"]["ipAddress"] == "192.168.1.1"
    assert isinstance(result["data"]["createDnsServer"]["dnsServer"]["ipAddress"], str)


def test_fraiseql_json_encoder_handles_fraiseql_types() -> None:
    """Test that FraiseQLJSONEncoder properly serializes @fraiseql.type decorated objects."""

    @fraiseql.type(sql_source="tv_user")
    @dataclass
    class User:
        """Test user type."""

        id: uuid.UUID
        name: str
        email: str | None
        created_at: datetime

    user = User(
        id=uuid.UUID("12345678-1234-1234-1234-123456789abc"),
        name="John Doe",
        email="john@example.com",
        created_at=datetime(2024, 1, 15, 10, 30, 0),
    )

    encoder = FraiseQLJSONEncoder()
    result = json.loads(encoder.encode(user))

    # Should serialize as dictionary with proper field values
    assert result["id"] == "12345678-1234-1234-1234-123456789abc"
    assert result["name"] == "John Doe"
    assert result["email"] == "john@example.com"
    assert result["created_at"] == "2024-01-15T10:30:00"


def test_fraiseql_json_encoder_handles_nested_fraiseql_types() -> None:
    """Test that FraiseQLJSONEncoder handles nested FraiseQL types."""

    @fraiseql.type(sql_source="tv_department")
    @dataclass
    class Department:
        """Test department type."""

        id: uuid.UUID
        name: str

    @fraiseql.type(sql_source="tv_user")
    @dataclass
    class User:
        """Test user type with nested department."""

        id: uuid.UUID
        name: str
        department: Department

    dept = Department(id=uuid.UUID("87654321-4321-4321-4321-876543210def"), name="Engineering")

    user = User(
        id=uuid.UUID("12345678-1234-1234-1234-123456789abc"), name="John Doe", department=dept
    )

    encoder = FraiseQLJSONEncoder()
    result = json.loads(encoder.encode(user))

    # Should serialize with nested object properly handled
    assert result["name"] == "John Doe"
    assert isinstance(result["department"], dict)
    assert result["department"]["name"] == "Engineering"
    assert result["department"]["id"] == "87654321-4321-4321-4321-876543210def"


if __name__ == "__main__":
    test_fraiseql_json_encoder_handles_unset()

    test_fraiseql_json_encoder_handles_nested_unset()

    test_fraiseql_json_encoder_handles_unset_in_lists()

    test_fraiseql_json_response_renders_unset()

    test_graphql_error_response_with_unset()

    test_input_object_with_unset_serialization()

    test_clean_unset_values_function()

    test_fraiseql_json_encoder_handles_ipaddress()

    test_fraiseql_json_response_with_ipaddress()

    test_fraiseql_json_encoder_handles_fraiseql_types()

    test_fraiseql_json_encoder_handles_nested_fraiseql_types()
