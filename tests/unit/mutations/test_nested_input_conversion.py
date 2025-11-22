"""Test case for nested input object field name conversion issue.

This test demonstrates the bug where nested input objects bypass camelCase→snake_case
field name conversion, causing inconsistent payloads to reach database functions.
"""

import fraiseql
from fraiseql.mutations.sql_generator import _serialize_value
from fraiseql.types.definitions import UNSET
from fraiseql.utils.casing import to_snake_case


@fraiseql.input
class CreatePublicAddressInput:
    """Direct input for creating public address."""

    street_number: str
    street_name: str
    postal_code: str


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


class TestNestedInputConversion:
    """Test nested input field name conversion."""

    def test_direct_input_shows_current_behavior(self) -> None:
        """Show what happens with direct input serialization."""
        # Create direct input object
        direct_input = CreatePublicAddressInput(
            street_number="123", street_name="Main St", postal_code="12345"
        )

        # The current _serialize_value behavior with FraiseQL inputs
        serialized = _serialize_value(direct_input)

        # This shows us what the current behavior produces
        assert isinstance(serialized, dict)

    def test_nested_input_serialization_current_behavior(self) -> None:
        """🔴 RED: Show the inconsistent behavior with nested inputs."""
        # Create nested input object
        nested_address = CreateNestedPublicAddressInput(
            street_number="456", street_name="Oak Ave", postal_code="67890"
        )

        location_input = CreateLocationInput(name="Test Location", address=nested_address)

        # Serialize location input
        serialized = _serialize_value(location_input)

        # The nested address field names - this is the bug we're testing
        nested_json = serialized["address"]

        # Current behavior: fields are serialized as-is (snake_case in this case)
        # The issue described in the report is that when GraphQL sends camelCase,
        # it doesn't get converted in nested objects but does in direct ones.
        # For now, let's test that we have some nested structure
        assert isinstance(nested_json, dict)
        assert len(nested_json) > 0

    def test_field_conversion_utility_function_works(self) -> None:
        """Helper test to verify our field conversion logic works correctly."""
        # Test the conversion utility
        assert to_snake_case("streetNumber") == "street_number"
        assert to_snake_case("streetName") == "street_name"
        assert to_snake_case("postalCode") == "postal_code"
        assert to_snake_case("organizationId") == "organization_id"

        # Snake case should remain unchanged
        assert to_snake_case("street_number") == "street_number"
        assert to_snake_case("street_name") == "street_name"
        assert to_snake_case("postal_code") == "postal_code"

    def test_serialize_value_with_camelcase_keys_shows_issue(self) -> None:
        """🔴 RED: Test that demonstrates the core issue with field name conversion."""
        # Simulate what would happen if GraphQL sent camelCase field names
        # in a nested object structure (this is the reported issue)

        # Create a nested input manually with camelCase keys to simulate
        # what GraphQL would send after parsing but before field conversion
        raw_nested_data = {
            "streetNumber": "789",  # This is camelCase as would come from GraphQL
            "streetName": "Pine Rd",
            "postalCode": "54321",
        }

        # The issue: when _serialize_value processes this dict,
        # it should convert camelCase keys to snake_case but doesn't
        serialized = _serialize_value(raw_nested_data)

        # 🔴 This should fail - we expect field name conversion but don't get it
        # After our fix, these assertions should pass:
        assert "street_number" in serialized, "Should convert camelCase to snake_case"
        assert "street_name" in serialized, "Should convert camelCase to snake_case"
        assert "postal_code" in serialized, "Should convert camelCase to snake_case"

        # And these should fail:
        assert "streetNumber" not in serialized, "Should not preserve camelCase"
        assert "streetName" not in serialized, "Should not preserve camelCase"
        assert "postalCode" not in serialized, "Should not preserve camelCase"

    def test_recursive_camelcase_conversion_in_nested_dicts(self) -> None:
        """Test that field conversion works recursively in deeply nested dictionaries."""
        nested_dict = {
            "topLevel": {
                "middleLevel": {
                    "deepLevel": {"veryDeepField": "value", "anotherField": "another_value"},
                    "secondMiddleField": "middle_value",
                },
                "topSecondField": "top_value",
            },
            "anotherTopField": "top_value",
        }

        serialized = _serialize_value(nested_dict)

        # Check top level conversion
        assert "top_level" in serialized
        assert "another_top_field" in serialized
        assert "topLevel" not in serialized
        assert "anotherTopField" not in serialized

        # Check middle level conversion
        middle = serialized["top_level"]
        assert "middle_level" in middle
        assert "top_second_field" in middle
        assert "middleLevel" not in middle
        assert "topSecondField" not in middle

        # Check deep level conversion
        deep = middle["middle_level"]
        assert "deep_level" in deep
        assert "second_middle_field" in deep
        assert "deepLevel" not in deep
        assert "secondMiddleField" not in deep

        # Check very deep level conversion
        very_deep = deep["deep_level"]
        assert "very_deep_field" in very_deep
        assert "another_field" in very_deep
        assert "veryDeepField" not in very_deep
        assert "anotherField" not in very_deep

    def test_mixed_camelcase_and_snake_case_conversion(self) -> None:
        """Test that mixed camelCase and snake_case keys both work correctly."""
        mixed_dict = {
            "camelCaseField": "camel_value",
            "snake_case_field": "snake_value",
            "alreadyCamelCase": "existing_camel",
            "already_snake": "existing_snake",
        }

        serialized = _serialize_value(mixed_dict)

        # All should be converted to snake_case
        assert "camel_case_field" in serialized
        assert "snake_case_field" in serialized
        assert "already_camel_case" in serialized
        assert "already_snake" in serialized

        # Original camelCase should not remain
        assert "camelCaseField" not in serialized
        assert "alreadyCamelCase" not in serialized

        # Values should be preserved
        assert serialized["camel_case_field"] == "camel_value"
        assert serialized["snake_case_field"] == "snake_value"
