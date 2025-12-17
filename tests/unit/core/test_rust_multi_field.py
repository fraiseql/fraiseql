"""Test build_multi_field_response() Rust function (Phase 1).

This test verifies the Rust implementation of multi-field GraphQL responses,
which combines multiple root fields into a single {"data": {...}} response.
"""

import json

import fraiseql._fraiseql_rs as fraiseql_rs
import pytest


# Schema registry initialization fixture
@pytest.fixture(scope="module", autouse=True)
def init_schema_registry() -> None:
    """Initialize schema registry for multi-field tests.

    Uses reset_schema_registry_for_testing() to ensure a clean state.
    """
    # Reset the schema registry to allow re-initialization
    fraiseql_rs.reset_schema_registry_for_testing()

    # Schema IR with multiple types for testing
    schema_ir = {
        "version": "1.0",
        "features": ["type_resolution"],
        "types": {
            "DnsServer": {
                "fields": {
                    "id": {"type_name": "Int", "is_nested_object": False, "is_list": False},
                    "ip_address": {
                        "type_name": "String",
                        "is_nested_object": False,
                        "is_list": False,
                    },
                    "hostname": {
                        "type_name": "String",
                        "is_nested_object": False,
                        "is_list": False,
                    },
                }
            },
            "Gateway": {
                "fields": {
                    "id": {"type_name": "Int", "is_nested_object": False, "is_list": False},
                    "hostname": {
                        "type_name": "String",
                        "is_nested_object": False,
                        "is_list": False,
                    },
                    "ip_address": {
                        "type_name": "String",
                        "is_nested_object": False,
                        "is_list": False,
                    },
                }
            },
            "User": {
                "fields": {
                    "id": {"type_name": "Int", "is_nested_object": False, "is_list": False},
                    "user_name": {
                        "type_name": "String",
                        "is_nested_object": False,
                        "is_list": False,
                    },
                    "email": {"type_name": "String", "is_nested_object": False, "is_list": False},
                }
            },
        },
    }

    fraiseql_rs.initialize_schema_registry(json.dumps(schema_ir))


def test_build_multi_field_response_two_fields() -> None:
    """Test multi-field response with two list fields.

    Simulates query:
    {
        dnsServers { id ipAddress }
        gateways { id hostname }
    }
    """
    fields = [
        (
            "dnsServers",
            "DnsServer",
            ['{"id": 1, "ip_address": "8.8.8.8"}', '{"id": 2, "ip_address": "1.1.1.1"}'],
            None,  # No field selections (Phase 1 - basic implementation)
            True,  # is_list
        ),
        (
            "gateways",
            "Gateway",
            ['{"id": 10, "hostname": "gateway1"}'],
            None,  # No field selections
            True,  # is_list
        ),
    ]

    response_bytes = fraiseql_rs.build_multi_field_response(fields)
    result = json.loads(response_bytes.decode("utf-8"))

    # Verify structure
    assert "data" in result
    assert "dnsServers" in result["data"]
    assert "gateways" in result["data"]

    # Verify dnsServers
    assert len(result["data"]["dnsServers"]) == 2
    assert result["data"]["dnsServers"][0]["id"] == 1
    assert result["data"]["dnsServers"][0]["ipAddress"] == "8.8.8.8"  # camelCase
    assert result["data"]["dnsServers"][0]["__typename"] == "DnsServer"

    # Verify gateways
    assert len(result["data"]["gateways"]) == 1
    assert result["data"]["gateways"][0]["id"] == 10
    assert result["data"]["gateways"][0]["hostname"] == "gateway1"
    assert result["data"]["gateways"][0]["__typename"] == "Gateway"


def test_build_multi_field_response_empty_field() -> None:
    """Test multi-field response with one empty field.

    Simulates query where one field returns empty results:
    {
        dnsServers { id }  # returns results
        gateways { id }    # empty
    }
    """
    fields = [
        (
            "dnsServers",
            "DnsServer",
            ['{"id": 1}'],
            None,  # No field selections
            True,
        ),
        (
            "gateways",
            "Gateway",
            [],  # Empty results
            None,  # No field selections
            True,
        ),
    ]

    response_bytes = fraiseql_rs.build_multi_field_response(fields)
    result = json.loads(response_bytes.decode("utf-8"))

    # Verify structure
    assert "data" in result
    assert len(result["data"]["dnsServers"]) == 1
    assert result["data"]["gateways"] == []  # Empty array


def test_build_multi_field_response_single_objects() -> None:
    """Test multi-field response with single objects (is_list=False).

    Simulates query:
    {
        currentUser { id userName }
        primaryGateway { id hostname }
    }
    """
    fields = [
        (
            "currentUser",
            "User",
            ['{"id": 42, "user_name": "alice"}'],
            None,  # No field selections
            False,  # Single object
        ),
        (
            "primaryGateway",
            "Gateway",
            ['{"id": 1, "hostname": "gw1"}'],
            None,  # No field selections
            False,  # Single object
        ),
    ]

    response_bytes = fraiseql_rs.build_multi_field_response(fields)
    result = json.loads(response_bytes.decode("utf-8"))

    # Verify structure
    assert "data" in result
    assert isinstance(result["data"]["currentUser"], dict)  # Not an array
    assert isinstance(result["data"]["primaryGateway"], dict)  # Not an array

    # Verify currentUser
    assert result["data"]["currentUser"]["id"] == 42
    assert result["data"]["currentUser"]["userName"] == "alice"
    assert result["data"]["currentUser"]["__typename"] == "User"

    # Verify primaryGateway
    assert result["data"]["primaryGateway"]["id"] == 1
    assert result["data"]["primaryGateway"]["hostname"] == "gw1"
    assert result["data"]["primaryGateway"]["__typename"] == "Gateway"


def test_build_multi_field_response_three_fields() -> None:
    """Test multi-field response with three fields.

    Simulates query:
    {
        dnsServers { id }
        gateways { id }
        users { id }
    }
    """
    fields = [
        ("dnsServers", "DnsServer", ['{"id": 1}'], None, True),
        ("gateways", "Gateway", ['{"id": 2}'], None, True),
        ("users", "User", ['{"id": 3}'], None, True),
    ]

    response_bytes = fraiseql_rs.build_multi_field_response(fields)
    result = json.loads(response_bytes.decode("utf-8"))

    # Verify all three fields present
    assert "dnsServers" in result["data"]
    assert "gateways" in result["data"]
    assert "users" in result["data"]

    # Verify each has correct type
    assert result["data"]["dnsServers"][0]["__typename"] == "DnsServer"
    assert result["data"]["gateways"][0]["__typename"] == "Gateway"
    assert result["data"]["users"][0]["__typename"] == "User"


def test_build_multi_field_response_no_selections() -> None:
    """Test multi-field response with no field selections (all fields returned).

    Simulates query without specific field selections:
    {
        dnsServers
        gateways
    }
    """
    fields = [
        (
            "dnsServers",
            "DnsServer",
            ['{"id": 1, "ip_address": "8.8.8.8", "hostname": "dns1"}'],
            None,  # No field selections
            True,
        ),
        (
            "gateways",
            "Gateway",
            ['{"id": 2, "hostname": "gw1", "ip_address": "192.168.1.1"}'],
            None,  # No field selections
            True,
        ),
    ]

    response_bytes = fraiseql_rs.build_multi_field_response(fields)
    result = json.loads(response_bytes.decode("utf-8"))

    # Verify all fields are present (no projection)
    dns = result["data"]["dnsServers"][0]
    assert "id" in dns
    assert "ipAddress" in dns  # camelCase
    assert "hostname" in dns
    assert dns["__typename"] == "DnsServer"

    gw = result["data"]["gateways"][0]
    assert "id" in gw
    assert "hostname" in gw
    assert "ipAddress" in gw  # camelCase
    assert gw["__typename"] == "Gateway"


def test_build_multi_field_response_mixed_list_and_single() -> None:
    """Test multi-field response with mixed list and single object fields.

    Simulates query:
    {
        dnsServers { id }      # List
        currentUser { id }     # Single object
    }
    """
    fields = [
        (
            "dnsServers",
            "DnsServer",
            ['{"id": 1}', '{"id": 2}'],
            None,  # No field selections
            True,  # List
        ),
        (
            "currentUser",
            "User",
            ['{"id": 42, "user_name": "alice"}'],
            None,  # No field selections
            False,  # Single object
        ),
    ]

    response_bytes = fraiseql_rs.build_multi_field_response(fields)
    result = json.loads(response_bytes.decode("utf-8"))

    # Verify list field
    assert isinstance(result["data"]["dnsServers"], list)
    assert len(result["data"]["dnsServers"]) == 2

    # Verify single object field
    assert isinstance(result["data"]["currentUser"], dict)
    assert result["data"]["currentUser"]["id"] == 42
