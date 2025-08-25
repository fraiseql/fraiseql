import pytest

"""Test conflict object camelCase transformation in mutations."""

import uuid

import fraiseql
from fraiseql import failure, success
from fraiseql.mutations.parser import parse_mutation_result

# No need to import MutationResult - parse_mutation_result expects a dict


@pytest.mark.unit
@fraiseql.type
class DnsServer:
    id: uuid.UUID
    name: str
    ip_address: str


@fraiseql.type
class NetworkConfiguration:
    id: uuid.UUID
    name: str
    subnet: str


@success
class CreateDnsServerSuccess:
    message: str
    dns_server: DnsServer


@failure
class CreateDnsServerError:
    message: str
    conflict_object: DnsServer | None = None


@success
class CreateNetworkSuccess:
    message: str
    network_configuration: NetworkConfiguration


@failure
class CreateNetworkError:
    message: str
    conflict_object: NetworkConfiguration | None = None


def test_conflict_object_snake_case_in_extra_metadata():
    """Test that conflict_object in extra_metadata is not transformed to camelCase."""
    # Create a mutation result with conflict_object in extra_metadata
    dns_server_id = uuid.uuid4()
    mutation_result = {
        "updated_fields": [],
        "status": "noop:already_exists",
        "message": "DNS server already exists",
        "object_data": None,
        "extra_metadata": {
            "conflict_object": {
                "id": str(dns_server_id),
                "name": "Primary DNS",
                "ip_address": "8.8.8.8",
            }
        },
    }

    # Parse as error
    result = parse_mutation_result(mutation_result, CreateDnsServerSuccess, CreateDnsServerError)

    # Check that it's parsed as error
    assert isinstance(result, CreateDnsServerError)
    assert result.message == "DNS server already exists"

    # Check if conflict_object was populated
    assert result.conflict_object is not None
    assert result.conflict_object.id == str(dns_server_id)
    assert result.conflict_object.name == "Primary DNS"
    assert result.conflict_object.ip_address == "8.8.8.8"


def test_specific_conflict_entity_alternative():
    """Test using specific conflict entity names like conflict_dns_server."""

    @failure
    class CreateDnsServerErrorSpecific:
        message: str
        conflict_dns_server: DnsServer | None = None

    # Create a mutation result with conflict_dns_server in extra_metadata
    dns_server_id = uuid.uuid4()
    mutation_result = {
        "updated_fields": [],
        "status": "noop:already_exists",
        "message": "DNS server already exists",
        "object_data": None,
        "extra_metadata": {
            "conflict_dns_server": {
                "id": str(dns_server_id),
                "name": "Secondary DNS",
                "ip_address": "8.8.4.4",
            }
        },
    }

    # Parse as error
    result = parse_mutation_result(
        mutation_result, CreateDnsServerSuccess, CreateDnsServerErrorSpecific
    )

    # Check that it's parsed as error
    assert isinstance(result, CreateDnsServerErrorSpecific)
    assert result.message == "DNS server already exists"

    # Check if conflict_dns_server was populated
    assert result.conflict_dns_server is not None
    assert result.conflict_dns_server.id == str(dns_server_id)
    assert result.conflict_dns_server.name == "Secondary DNS"
    assert result.conflict_dns_server.ip_address == "8.8.4.4"


def test_camelcase_field_names_should_work():
    """Test that camelCase field names in failure types should be matched."""

    @failure
    class CreateNetworkErrorCamelCase:
        message: str
        conflictObject: NetworkConfiguration | None = None

    # Create a mutation result with conflict_object in extra_metadata
    network_id = uuid.uuid4()
    mutation_result = {
        "updated_fields": [],
        "status": "noop:already_exists",
        "message": "Network already exists",
        "object_data": None,
        "extra_metadata": {
            "conflict_object": {  # snake_case in database
                "id": str(network_id),
                "name": "Office Network",
                "subnet": "192.168.1.0/24",
            }
        },
    }

    # Parse as error
    result = parse_mutation_result(
        mutation_result, CreateNetworkSuccess, CreateNetworkErrorCamelCase
    )

    # Check that it's parsed as error
    assert isinstance(result, CreateNetworkErrorCamelCase)
    assert result.message == "Network already exists"

    # Check if conflictObject was populated from conflict_object
    # This test might fail if snake_case to camelCase transformation is not implemented
    assert result.conflictObject is not None
    assert result.conflictObject.id == str(network_id)
    assert result.conflictObject.name == "Office Network"
    assert result.conflictObject.subnet == "192.168.1.0/24"
