"""Real-world test scenario based on the original bug report.

This test replicates the exact scenario described in the bug report:
- Direct address creation works (streetNumber → street_number)
- Nested address creation fails (streetNumber stays as streetNumber)

This test verifies that both now work consistently.
"""

from uuid import uuid4

import fraiseql
from fraiseql.types.definitions import UNSET


@fraiseql.input
class CreatePublicAddressInput:
    """Direct address input that was working in v0.7.13."""

    street_number: str
    street_name: str
    postal_code: str
    country_code: str | None = UNSET
    latitude: float | None = UNSET
    longitude: float | None = UNSET


@fraiseql.input
class CreateNestedPublicAddressInput:
    """Nested address input that was broken in v0.7.13."""

    street_number: str | None = UNSET
    street_name: str
    postal_code: str
    country_code: str | None = UNSET
    latitude: float | None = UNSET
    longitude: float | None = UNSET


@fraiseql.input
class CreateLocationInput:
    """Location input containing nested address."""

    name: str
    description: str | None = UNSET
    address: CreateNestedPublicAddressInput | None = UNSET


@fraiseql.success
class CreatePublicAddressSuccess:
    """Success response for address creation."""

    address: dict  # Simplified - would be proper type in real scenario
    message: str = "Address created successfully"


@fraiseql.error
class CreatePublicAddressError:
    """Error response for address creation."""

    message: str
    code: str
    field_errors: dict | None = UNSET


@fraiseql.success
class CreateLocationSuccess:
    """Success response for location creation."""

    location: dict  # Simplified
    message: str = "Location created successfully"


@fraiseql.error
class CreateLocationError:
    """Error response for location creation."""

    message: str
    code: str
    field_errors: dict | None = UNSET


def test_direct_address_creation_works() -> None:
    """Test that direct address creation works (this was already working in v0.7.13)."""
    from fraiseql.mutations.sql_generator import _serialize_value
    from fraiseql.types.coercion import coerce_input

    # Simulate GraphQL input (camelCase)
    graphql_input = {
        "streetNumber": "15",
        "streetName": "Main Street",
        "postalCode": "12345",
        "countryCode": "US",
        "latitude": 40.7128,
        "longitude": -74.0060,
    }

    # Step 1: Coerce GraphQL input to dataclass
    address_input = coerce_input(CreatePublicAddressInput, graphql_input)

    # Verify coercion worked
    assert isinstance(address_input, CreatePublicAddressInput)
    assert address_input.street_number == "15"
    assert address_input.street_name == "Main Street"
    assert address_input.postal_code == "12345"
    assert address_input.country_code == "US"
    assert address_input.latitude == 40.7128
    assert address_input.longitude == -74.0060

    # Step 2: Serialize to database payload
    db_payload = _serialize_value(address_input)

    # Verify database payload has snake_case keys
    assert isinstance(db_payload, dict)
    assert "street_number" in db_payload
    assert "street_name" in db_payload
    assert "postal_code" in db_payload
    assert "country_code" in db_payload
    assert "latitude" in db_payload
    assert "longitude" in db_payload

    # Verify values are preserved
    assert db_payload["street_number"] == "15"
    assert db_payload["street_name"] == "Main Street"
    assert db_payload["postal_code"] == "12345"
    assert db_payload["country_code"] == "US"
    assert db_payload["latitude"] == 40.7128
    assert db_payload["longitude"] == -74.0060


def test_nested_address_creation_now_works() -> None:
    """Test that nested address creation works (this was broken in v0.7.13, should work now)."""
    from fraiseql.mutations.sql_generator import _serialize_value
    from fraiseql.types.coercion import coerce_input

    # Simulate GraphQL input (camelCase) for nested structure
    graphql_input = {
        "name": "Main Office",
        "description": "Primary business location",
        "address": {
            "streetNumber": "15",  # This would stay as camelCase in v0.7.13
            "streetName": "Main Street",
            "postalCode": "12345",
            "countryCode": "US",
            "latitude": 40.7128,
            "longitude": -74.0060,
        },
    }

    # Step 1: Coerce GraphQL input to dataclass
    location_input = coerce_input(CreateLocationInput, graphql_input)

    # Verify coercion worked for nested structure
    assert isinstance(location_input, CreateLocationInput)
    assert location_input.name == "Main Office"
    assert location_input.description == "Primary business location"
    assert isinstance(location_input.address, CreateNestedPublicAddressInput)

    # Verify nested address has correct field values (the key test!)
    nested_address = location_input.address
    assert nested_address.street_number == "15"
    assert nested_address.street_name == "Main Street"
    assert nested_address.postal_code == "12345"
    assert nested_address.country_code == "US"
    assert nested_address.latitude == 40.7128
    assert nested_address.longitude == -74.0060

    # Step 2: Serialize to database payload
    db_payload = _serialize_value(location_input)

    # Verify top-level has snake_case keys
    assert isinstance(db_payload, dict)
    assert "name" in db_payload
    assert "description" in db_payload
    assert "address" in db_payload

    # Verify nested address has snake_case keys (the critical fix!)
    address_payload = db_payload["address"]
    assert isinstance(address_payload, dict)
    assert "street_number" in address_payload  # This would be missing in v0.7.13
    assert "street_name" in address_payload
    assert "postal_code" in address_payload
    assert "country_code" in address_payload
    assert "latitude" in address_payload
    assert "longitude" in address_payload

    # Verify no camelCase keys remain (this was the bug)
    assert "streetNumber" not in address_payload
    assert "streetName" not in address_payload
    assert "postalCode" not in address_payload
    assert "countryCode" not in address_payload

    # Verify all values are preserved
    assert db_payload["name"] == "Main Office"
    assert db_payload["description"] == "Primary business location"
    assert address_payload["street_number"] == "15"
    assert address_payload["street_name"] == "Main Street"
    assert address_payload["postal_code"] == "12345"
    assert address_payload["country_code"] == "US"
    assert address_payload["latitude"] == 40.7128
    assert address_payload["longitude"] == -74.0060


def test_database_function_simulation() -> None:
    """Simulate what would happen in PostgreSQL function with the payloads."""
    from fraiseql.mutations.sql_generator import _serialize_value
    from fraiseql.types.coercion import coerce_input

    # Test both direct and nested scenarios
    direct_graphql_input = {
        "streetNumber": "123",
        "streetName": "Direct St",
        "postalCode": "11111",
        "countryCode": "CA",
    }

    nested_graphql_input = {
        "name": "Test Location",
        "address": {
            "streetNumber": "456",
            "streetName": "Nested Ave",
            "postalCode": "22222",
            "countryCode": "UK",
        },
    }

    # Process both
    direct_input = coerce_input(CreatePublicAddressInput, direct_graphql_input)
    nested_input = coerce_input(CreateLocationInput, nested_graphql_input)

    direct_payload = _serialize_value(direct_input)
    nested_payload = _serialize_value(nested_input)

    # Simulate PostgreSQL function expecting consistent snake_case
    def simulate_postgres_function(payload: dict) -> dict:
        """Simulate a PostgreSQL function that expects snake_case fields."""
        # This would fail in v0.7.13 for nested inputs because they had camelCase
        try:
            street_number = payload.get("street_number")
            street_name = payload.get("street_name")
            postal_code = payload.get("postal_code")
            country_code = payload.get("country_code")

            if not street_number or not street_name or not postal_code:
                return {"success": False, "error": "Missing required fields"}

            return {
                "success": True,
                "address_id": str(uuid4()),
                "formatted_address": f"{street_number} {street_name}, {postal_code}, {country_code}",
            }
        except Exception as e:
            return {"success": False, "error": str(e)}

    # Test direct payload (always worked)
    direct_result = simulate_postgres_function(direct_payload)
    assert direct_result["success"] is True
    assert "Direct St" in direct_result["formatted_address"]

    # Test nested payload (would fail in v0.7.13, should work now)
    nested_address_payload = nested_payload["address"]
    nested_result = simulate_postgres_function(nested_address_payload)
    assert nested_result["success"] is True
    assert "Nested Ave" in nested_result["formatted_address"]

    # Verify both produce consistent results
    assert all("street_number" in payload for payload in [direct_payload, nested_address_payload])
    assert all(
        "streetNumber" not in payload for payload in [direct_payload, nested_address_payload]
    )


def test_regression_prevention() -> None:
    """Test to prevent future regressions of this issue."""
    from typing import get_args, get_origin

    from fraiseql.mutations.sql_generator import _serialize_value
    from fraiseql.types.coercion import _coerce_field_value

    # Test the specific Union type handling that was broken
    location_field_type = CreateLocationInput.__annotations__["address"]

    # Verify union type detection works
    origin = get_origin(location_field_type)
    args = get_args(location_field_type)

    # This should be types.UnionType in Python 3.10+
    import types

    assert origin is types.UnionType or origin.__name__ == "Union"
    assert len(args) == 2
    assert CreateNestedPublicAddressInput in args
    assert type(None) in args

    # Test _coerce_field_value handles Union correctly
    raw_address_data = {"streetNumber": "789", "streetName": "Union Test St", "postalCode": "99999"}

    coerced = _coerce_field_value(raw_address_data, location_field_type)

    # Should be properly coerced to FraiseQL object, not left as dict
    assert isinstance(coerced, CreateNestedPublicAddressInput)
    assert coerced.street_number == "789"
    assert coerced.street_name == "Union Test St"
    assert coerced.postal_code == "99999"

    # Final serialization should have consistent field names
    serialized = _serialize_value(coerced)
    assert "street_number" in serialized
    assert "streetNumber" not in serialized
