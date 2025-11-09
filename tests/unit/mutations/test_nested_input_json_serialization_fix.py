"""Test for JSON serialization fix with nested FraiseQL input objects.

This test verifies that the v0.7.14 JSON serialization issue is resolved
in v0.7.15 with built-in to_dict() and __json__() methods for FraiseQL input objects.
"""

import json

import fraiseql
from fraiseql.fastapi.json_encoder import FraiseQLJSONEncoder
from fraiseql.types.definitions import UNSET


@fraiseql.input
class CreateNestedPublicAddressInput:
    """Nested input for creating public address (used within other inputs)."""

    street_number: str | None = UNSET
    street_name: str
    postal_code: str


@fraiseql.input
class CreateLocationInput:
    """Input that contains nested address input."""

    name: str
    address: CreateNestedPublicAddressInput | None = UNSET


class TestNestedInputJSONSerializationFix:
    """Test JSON serialization fix for nested FraiseQL input objects."""

    def test_fraiseql_input_has_to_dict_method(self) -> None:
        """🟢 GREEN: FraiseQL input objects have to_dict() method."""
        nested_address = CreateNestedPublicAddressInput(
            street_number="456", street_name="Oak Ave", postal_code="67890"
        )

        assert hasattr(nested_address, "to_dict")
        assert callable(nested_address.to_dict)

    def test_fraiseql_input_has_json_method(self) -> None:
        """🟢 GREEN: FraiseQL input objects have __json__() method."""
        nested_address = CreateNestedPublicAddressInput(
            street_number="456", street_name="Oak Ave", postal_code="67890"
        )

        assert hasattr(nested_address, "__json__")
        assert callable(nested_address.__json__)

    def test_to_dict_excludes_unset_values(self) -> None:
        """🟢 GREEN: to_dict() method excludes UNSET values."""
        nested_address = CreateNestedPublicAddressInput(
            street_name="Oak Ave",
            postal_code="67890",
            # street_number is UNSET by default
        )

        result = nested_address.to_dict()

        assert "street_name" in result
        assert "postal_code" in result
        assert "street_number" not in result  # UNSET values are excluded
        assert result["street_name"] == "Oak Ave"
        assert result["postal_code"] == "67890"

    def test_to_dict_includes_set_values(self) -> None:
        """🟢 GREEN: to_dict() method includes all set values."""
        nested_address = CreateNestedPublicAddressInput(
            street_number="123", street_name="Main St", postal_code="12345"
        )

        result = nested_address.to_dict()

        assert "street_number" in result
        assert "street_name" in result
        assert "postal_code" in result
        assert result["street_number"] == "123"
        assert result["street_name"] == "Main St"
        assert result["postal_code"] == "12345"

    def test_json_method_returns_dict(self) -> None:
        """🟢 GREEN: __json__() method returns dictionary."""
        nested_address = CreateNestedPublicAddressInput(
            street_number="456", street_name="Oak Ave", postal_code="67890"
        )

        result = nested_address.__json__()

        assert isinstance(result, dict)
        assert result == nested_address.to_dict()

    def test_nested_fraiseql_input_works_with_custom_encoder(self) -> None:
        """🟢 GREEN: FraiseQL input objects work with FraiseQLJSONEncoder."""
        nested_address = CreateNestedPublicAddressInput(
            street_number="456", street_name="Oak Ave", postal_code="67890"
        )

        location_input = CreateLocationInput(name="Test Location", address=nested_address)

        # This should now work with FraiseQLJSONEncoder
        result = json.dumps(location_input, cls=FraiseQLJSONEncoder)
        assert isinstance(result, str)

        # Parse back to verify structure
        parsed = json.loads(result)
        assert parsed["name"] == "Test Location"
        assert "address" in parsed
        assert parsed["address"]["street_number"] == "456"
        assert parsed["address"]["street_name"] == "Oak Ave"
        assert parsed["address"]["postal_code"] == "67890"

    def test_nested_object_to_dict_recursive_conversion(self) -> None:
        """🟢 GREEN: Nested objects are recursively converted to dictionaries."""
        nested_address = CreateNestedPublicAddressInput(
            street_number="789", street_name="Elm St", postal_code="54321"
        )

        location_input = CreateLocationInput(name="Complex Location", address=nested_address)

        result = location_input.to_dict()

        assert result["name"] == "Complex Location"
        assert isinstance(result["address"], dict)
        assert result["address"]["street_number"] == "789"
        assert result["address"]["street_name"] == "Elm St"
        assert result["address"]["postal_code"] == "54321"

    def test_nested_object_with_unset_field_conversion(self) -> None:
        """🟢 GREEN: Nested objects with UNSET fields work correctly."""
        nested_address = CreateNestedPublicAddressInput(
            street_name="Pine Ave",
            postal_code="98765",
            # street_number is UNSET
        )

        location_input = CreateLocationInput(
            name="Location with Partial Address", address=nested_address
        )

        result = location_input.to_dict()

        assert result["name"] == "Location with Partial Address"
        assert isinstance(result["address"], dict)
        assert "street_name" in result["address"]
        assert "postal_code" in result["address"]
        assert "street_number" not in result["address"]  # UNSET excluded

    def test_standard_json_serialization_now_works(self) -> None:
        """🟢 GREEN: Standard JSON serialization now works with FraiseQL objects."""
        nested_address = CreateNestedPublicAddressInput(
            street_number="999", street_name="Test Ave", postal_code="11111"
        )

        # Test with FraiseQLJSONEncoder
        result = json.dumps(nested_address, cls=FraiseQLJSONEncoder)
        parsed = json.loads(result)

        assert parsed["street_number"] == "999"
        assert parsed["street_name"] == "Test Ave"
        assert parsed["postal_code"] == "11111"

    def test_unset_values_handled_by_encoder(self) -> None:
        """🟢 GREEN: UNSET values are properly handled by the JSON encoder."""
        from fraiseql.types.definitions import UNSET

        # Test UNSET serialization with FraiseQLJSONEncoder
        result = json.dumps(UNSET, cls=FraiseQLJSONEncoder)
        assert result == "null"

    def test_complex_nested_structure(self) -> None:
        """🟢 GREEN: Complex nested structures work correctly."""

        @fraiseql.input
        class NestedAddressInput:
            street: str
            city: str

        @fraiseql.input
        class PersonInput:
            name: str
            addresses: list[NestedAddressInput] | None = UNSET

        addresses = [
            NestedAddressInput(street="123 Main St", city="City A"),
            NestedAddressInput(street="456 Oak Ave", city="City B"),
        ]

        person = PersonInput(name="John Doe", addresses=addresses)

        result = person.to_dict()

        assert result["name"] == "John Doe"
        assert len(result["addresses"]) == 2
        assert result["addresses"][0]["street"] == "123 Main St"
        assert result["addresses"][0]["city"] == "City A"
        assert result["addresses"][1]["street"] == "456 Oak Ave"
        assert result["addresses"][1]["city"] == "City B"
