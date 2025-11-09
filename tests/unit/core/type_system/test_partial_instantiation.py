import pytest

"""Tests for partial object instantiation."""

import dataclasses
from uuid import UUID

from fraiseql.partial_instantiation import (
    create_partial_instance,
    get_available_fields,
    is_partial_instance,
)


@pytest.mark.unit
@dataclasses.dataclass
class NestedModel:
    """A nested model for testing."""

    id: UUID
    name: str
    required_field: str


@dataclasses.dataclass
class SampleModel:
    """A test model with required and optional fields."""

    id: UUID
    name: str
    required_field: str
    optional_field: str | None = None
    nested: NestedModel | None = None


class TestPartialInstantiation:
    """Test partial instantiation functionality."""

    def test_partial_instance_with_all_fields(self) -> None:
        """Test creating instance when all fields are provided."""
        data = {
            "id": UUID("550e8400-e29b-41d4-a716-446655440000"),
            "name": "Test",
            "required_field": "Required",
            "optional_field": "Optional",
        }

        instance = create_partial_instance(SampleModel, data)

        assert instance.id == data["id"]
        assert instance.name == data["name"]
        assert instance.required_field == data["required_field"]
        assert instance.optional_field == data["optional_field"]
        assert instance.nested is None

        # Should be marked as partial
        assert is_partial_instance(instance)
        assert get_available_fields(instance) == {"id", "name", "required_field", "optional_field"}

    def test_partial_instance_missing_required_fields(self) -> None:
        """Test creating instance when required fields are missing."""
        data = {
            "id": UUID("550e8400-e29b-41d4-a716-446655440000"),
            "name": "Test",
            # required_field is missing
        }

        instance = create_partial_instance(SampleModel, data)

        assert instance.id == data["id"]
        assert instance.name == data["name"]
        assert instance.required_field is None  # Missing required field becomes None
        assert instance.optional_field is None

        assert is_partial_instance(instance)
        assert get_available_fields(instance) == {"id", "name"}

    def test_partial_instance_with_nested_partial(self) -> None:
        """Test creating instance with nested partial objects."""
        data = {
            "id": UUID("550e8400-e29b-41d4-a716-446655440000"),
            "name": "Test",
            "required_field": "Required",
            "nested": {
                "id": UUID("650e8400-e29b-41d4-a716-446655440001"),
                "name": "Nested",
                # required_field is missing in nested
            },
        }

        # First create the nested partial instance
        nested_instance = create_partial_instance(NestedModel, data["nested"])
        data["nested"] = nested_instance

        instance = create_partial_instance(SampleModel, data)

        assert instance.id == data["id"]
        assert instance.nested is not None
        assert instance.nested.id == UUID("650e8400-e29b-41d4-a716-446655440001")
        assert instance.nested.name == "Nested"
        assert instance.nested.required_field is None  # Missing required field

        assert is_partial_instance(instance)
        assert is_partial_instance(instance.nested)

    def test_partial_instance_only_requested_fields(self) -> None:
        """Test creating instance with only GraphQL-requested fields."""
        # Simulating a GraphQL query that only requests id and name
        data = {"id": UUID("550e8400-e29b-41d4-a716-446655440000"), "name": "Test"}

        instance = create_partial_instance(SampleModel, data)

        assert instance.id == data["id"]
        assert instance.name == data["name"]
        assert instance.required_field is None
        assert instance.optional_field is None
        assert instance.nested is None

        # Only the provided fields should be tracked
        assert get_available_fields(instance) == {"id", "name"}

    def test_regular_class_partial_instantiation(self) -> None:
        """Test partial instantiation with non-dataclass types."""

        class RegularClass:
            def __init__(self, id: UUID, name: str, required: str) -> None:
                self.id = id
                self.name = name
                self.required = required

        # This would normally fail with TypeError
        data = {
            "id": UUID("550e8400-e29b-41d4-a716-446655440000"),
            "name": "Test",
            # required is missing
        }

        instance = create_partial_instance(RegularClass, data)

        assert instance.id == data["id"]
        assert instance.name == data["name"]
        assert hasattr(instance, "required") is False or instance.required is None
        assert is_partial_instance(instance)
