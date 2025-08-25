import pytest

"""Test that error details are transformed to camelCase when configured."""

import uuid

import fraiseql
from fraiseql import failure
from fraiseql.config.schema_config import SchemaConfig
from fraiseql.mutations.parser import parse_mutation_result
from fraiseql.types.errors import Error


@pytest.mark.unit
@fraiseql.type
class ConflictObject:
    id: uuid.UUID
    ip_address: str
    dns_server_name: str
    created_at: str


@failure
class MutationError:
    message: str
    errors: list[Error] | None = None


def test_error_details_camelcase_transformation():
    """Test that error details from extra_metadata are transformed to camelCase."""
    # Enable camelCase fields
    config = SchemaConfig.get_instance()
    original_setting = config.camel_case_fields
    config.camel_case_fields = True

    try:
        # Create a mutation result with snake_case fields in extra_metadata
        conflict_id = uuid.uuid4()
        mutation_result = {
            "id": None,
            "updated_fields": [],
            "status": "conflict:ip_address_exists",
            "message": "IP address already exists",
            "object_data": None,
            "extra_metadata": {
                "conflict_object": {
                    "id": str(conflict_id),
                    "ip_address": "192.168.1.1",
                    "dns_server_name": "primary-dns",
                    "created_at": "2024-01-01T00:00:00Z",
                },
                "existing_network_id": str(uuid.uuid4()),
                "validation_errors": [{"field_name": "ip_address", "error_code": "DUPLICATE"}],
            },
        }

        # Parse as error (uses a dummy success type)
        @fraiseql.type
        class DummySuccess:
            message: str

        result = parse_mutation_result(mutation_result, DummySuccess, MutationError)

        # Check that it's parsed as error with auto-populated errors
        assert isinstance(result, MutationError)
        assert result.message == "IP address already exists"
        assert result.errors is not None
        assert len(result.errors) == 1

        error = result.errors[0]
        assert error.message == "IP address already exists"
        assert error.code == 409  # conflict code
        assert error.identifier == "ip_address_exists"
        assert error.details is not None

        # Check that all keys in details are camelCase
        assert "conflictObject" in error.details
        assert "conflict_object" not in error.details
        assert "existingNetworkId" in error.details
        assert "existing_network_id" not in error.details
        assert "validationErrors" in error.details
        assert "validation_errors" not in error.details

        # Check nested objects are also transformed
        conflict_obj = error.details["conflictObject"]
        assert "ipAddress" in conflict_obj
        assert "ip_address" not in conflict_obj
        assert "dnsServerName" in conflict_obj
        assert "dns_server_name" not in conflict_obj
        assert "createdAt" in conflict_obj
        assert "created_at" not in conflict_obj

        # Check arrays of objects are transformed
        validation_errors = error.details["validationErrors"]
        assert len(validation_errors) == 1
        assert "fieldName" in validation_errors[0]
        assert "field_name" not in validation_errors[0]
        assert "errorCode" in validation_errors[0]
        assert "error_code" not in validation_errors[0]

    finally:
        # Restore original setting
        config.camel_case_fields = original_setting


def test_error_details_snake_case_preserved():
    """Test that error details keep snake_case when camelCase is disabled."""
    # Disable camelCase fields
    config = SchemaConfig.get_instance()
    original_setting = config.camel_case_fields
    config.camel_case_fields = False

    try:
        # Create a mutation result with snake_case fields in extra_metadata
        mutation_result = {
            "id": None,
            "updated_fields": [],
            "status": "validation_error",
            "message": "Validation failed",
            "object_data": None,
            "extra_metadata": {
                "field_errors": {
                    "email_address": "Invalid format",
                    "phone_number": "Required field",
                }
            },
        }

        # Parse as error
        @fraiseql.type
        class DummySuccess:
            message: str

        result = parse_mutation_result(mutation_result, DummySuccess, MutationError)

        # Check that it's parsed as error
        assert isinstance(result, MutationError)
        assert result.errors is not None
        assert len(result.errors) == 1

        error = result.errors[0]
        assert error.details is not None

        # Check that keys remain snake_case
        assert "field_errors" in error.details
        assert "fieldErrors" not in error.details

        field_errors = error.details["field_errors"]
        assert "email_address" in field_errors
        assert "emailAddress" not in field_errors
        assert "phone_number" in field_errors
        assert "phoneNumber" not in field_errors

    finally:
        # Restore original setting
        config.camel_case_fields = original_setting
