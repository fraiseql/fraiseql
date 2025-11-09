"""Comprehensive tests for nested input conversion issue.

This test verifies the complete pipeline from GraphQL input to database function
to ensure camelCase→snake_case conversion works consistently for both direct
and nested input objects.
"""

import fraiseql
from fraiseql.types.definitions import UNSET


@fraiseql.input
class AddressInput:
    """Test address input with snake_case field names."""

    street_number: str
    street_name: str
    postal_code: str
    country_code: str | None = UNSET


@fraiseql.input
class LocationInput:
    """Test location input with nested address."""

    name: str
    description: str | None = UNSET
    address: AddressInput | None = UNSET  # Nested input object


@fraiseql.input
class CompanyInput:
    """Test company input with nested location."""

    company_name: str
    registration_number: str | None = UNSET
    location: LocationInput | None = UNSET  # Nested nested input object


def test_direct_input_field_names() -> None:
    """Test that direct input object has correct field names."""
    # Simulate how GraphQL would pass arguments
    address = AddressInput(
        street_number="123", street_name="Main Street", postal_code="12345", country_code="US"
    )

    # Verify the object has snake_case field names
    assert hasattr(address, "street_number")
    assert hasattr(address, "street_name")
    assert hasattr(address, "postal_code")
    assert hasattr(address, "country_code")

    assert address.street_number == "123"
    assert address.street_name == "Main Street"
    assert address.postal_code == "12345"
    assert address.country_code == "US"


def test_sql_generator_serialization() -> None:
    """Test SQL generator serialization for both direct and nested inputs."""
    from fraiseql.mutations.sql_generator import _serialize_value

    # Create address input object (simulating direct mutation input)
    address = AddressInput(
        street_number="123", street_name="Main Street", postal_code="12345", country_code="US"
    )

    # Serialize the address object
    serialized = _serialize_value(address)

    # Should have snake_case keys in output
    assert isinstance(serialized, dict)
    assert "street_number" in serialized
    assert "street_name" in serialized
    assert "postal_code" in serialized
    assert "country_code" in serialized

    # Verify values
    assert serialized["street_number"] == "123"
    assert serialized["street_name"] == "Main Street"
    assert serialized["postal_code"] == "12345"
    assert serialized["country_code"] == "US"


def test_nested_input_serialization() -> None:
    """Test that nested input objects get serialized correctly."""
    from fraiseql.mutations.sql_generator import _serialize_value

    # Create nested input structure
    address = AddressInput(
        street_number="456", street_name="Oak Avenue", postal_code="67890", country_code="CA"
    )

    location = LocationInput(
        name="Main Office", description="Primary business location", address=address
    )

    # Serialize the location (which contains nested address)
    serialized = _serialize_value(location)

    # Should have snake_case keys at top level
    assert isinstance(serialized, dict)
    assert "name" in serialized
    assert "description" in serialized
    assert "address" in serialized

    # Nested address should also have snake_case keys
    address_data = serialized["address"]
    assert isinstance(address_data, dict)
    assert "street_number" in address_data
    assert "street_name" in address_data
    assert "postal_code" in address_data
    assert "country_code" in address_data

    # Verify nested values
    assert address_data["street_number"] == "456"
    assert address_data["street_name"] == "Oak Avenue"
    assert address_data["postal_code"] == "67890"
    assert address_data["country_code"] == "CA"


def test_deeply_nested_input_serialization() -> None:
    """Test that deeply nested input objects work correctly."""
    from fraiseql.mutations.sql_generator import _serialize_value

    # Create deeply nested structure
    address = AddressInput(
        street_number="789", street_name="Pine Street", postal_code="11111", country_code="UK"
    )

    location = LocationInput(
        name="Branch Office", description="Secondary location", address=address
    )

    company = CompanyInput(
        company_name="Tech Corp", registration_number="12345678", location=location
    )

    # Serialize the company (deeply nested structure)
    serialized = _serialize_value(company)

    # Verify all levels have snake_case keys
    assert isinstance(serialized, dict)
    assert "company_name" in serialized
    assert "registration_number" in serialized
    assert "location" in serialized

    location_data = serialized["location"]
    assert isinstance(location_data, dict)
    assert "name" in location_data
    assert "description" in location_data
    assert "address" in location_data

    address_data = location_data["address"]
    assert isinstance(address_data, dict)
    assert "street_number" in address_data
    assert "street_name" in address_data
    assert "postal_code" in address_data
    assert "country_code" in address_data

    # Verify deeply nested values
    assert address_data["street_number"] == "789"
    assert address_data["street_name"] == "Pine Street"
    assert address_data["postal_code"] == "11111"
    assert address_data["country_code"] == "UK"


def test_raw_dict_conversion() -> None:
    """Test that raw dictionaries (simulating GraphQL input) get converted correctly."""
    from fraiseql.mutations.sql_generator import _serialize_value

    # Simulate raw GraphQL input with camelCase keys (this is the problematic case)
    raw_address_data = {
        "streetNumber": "999",
        "streetName": "Elm Street",
        "postalCode": "99999",
        "countryCode": "DE",
    }

    # This should convert camelCase keys to snake_case
    serialized = _serialize_value(raw_address_data)

    # Should now have snake_case keys
    assert isinstance(serialized, dict)
    assert "street_number" in serialized
    assert "street_name" in serialized
    assert "postal_code" in serialized
    assert "country_code" in serialized

    # Verify converted values
    assert serialized["street_number"] == "999"
    assert serialized["street_name"] == "Elm Street"
    assert serialized["postal_code"] == "99999"
    assert serialized["country_code"] == "DE"


def test_mixed_nested_dict_conversion() -> None:
    """Test conversion when we have mixed FraiseQL objects and raw dicts."""
    from fraiseql.mutations.sql_generator import _serialize_value

    # Simulate a case where we have a FraiseQL object containing a raw dict
    # (this might happen in complex nested scenarios)
    raw_nested_data = {
        "name": "Mixed Office",
        "description": "Office with mixed input",
        "address": {
            "streetNumber": "111",
            "streetName": "Maple Street",
            "postalCode": "22222",
            "countryCode": "FR",
        },
    }

    serialized = _serialize_value(raw_nested_data)

    # Top level should be fine
    assert "name" in serialized
    assert "description" in serialized
    assert "address" in serialized

    # Nested dict should have converted keys
    address_data = serialized["address"]
    assert "street_number" in address_data
    assert "street_name" in address_data
    assert "postal_code" in address_data
    assert "country_code" in address_data

    assert address_data["street_number"] == "111"
    assert address_data["street_name"] == "Maple Street"


def test_coercion_from_camel_case() -> None:
    """Test that the coercion system properly converts camelCase GraphQL input."""
    from fraiseql.types.coercion import coerce_input

    # Simulate GraphQL input with camelCase keys
    graphql_input = {
        "streetNumber": "777",
        "streetName": "Cedar Avenue",
        "postalCode": "77777",
        "countryCode": "JP",
    }

    # This should create a proper AddressInput object with snake_case fields
    address = coerce_input(AddressInput, graphql_input)

    assert isinstance(address, AddressInput)
    assert address.street_number == "777"
    assert address.street_name == "Cedar Avenue"
    assert address.postal_code == "77777"
    assert address.country_code == "JP"


def test_end_to_end_pipeline() -> None:
    """Test the complete pipeline: GraphQL input → coercion → serialization."""
    from fraiseql.mutations.sql_generator import _serialize_value
    from fraiseql.types.coercion import coerce_input

    # Step 1: Simulate GraphQL input (camelCase)
    graphql_input = {
        "companyName": "End to End Corp",
        "registrationNumber": "E2E123456",
        "location": {
            "name": "E2E Office",
            "description": "Test location",
            "address": {
                "streetNumber": "555",
                "streetName": "Test Boulevard",
                "postalCode": "55555",
                "countryCode": "NL",
            },
        },
    }

    # Step 2: Coerce into FraiseQL objects (simulating GraphQL coercion)
    company = coerce_input(CompanyInput, graphql_input)

    # Verify coercion worked
    assert isinstance(company, CompanyInput)
    assert company.company_name == "End to End Corp"
    assert company.registration_number == "E2E123456"
    assert isinstance(company.location, LocationInput)
    assert isinstance(company.location.address, AddressInput)

    # Step 3: Serialize for database (simulating mutation SQL generator)
    serialized = _serialize_value(company)

    # Step 4: Verify final database payload has consistent snake_case
    assert "company_name" in serialized
    assert "registration_number" in serialized
    assert "location" in serialized

    location_data = serialized["location"]
    assert "name" in location_data
    assert "description" in location_data
    assert "address" in location_data

    address_data = location_data["address"]
    assert "street_number" in address_data
    assert "street_name" in address_data
    assert "postal_code" in address_data
    assert "country_code" in address_data

    # All data should be preserved through the pipeline
    assert serialized["company_name"] == "End to End Corp"
    assert address_data["street_number"] == "555"
    assert address_data["street_name"] == "Test Boulevard"
