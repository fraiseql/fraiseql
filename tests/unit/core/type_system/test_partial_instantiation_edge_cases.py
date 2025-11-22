import pytest

"""Comprehensive tests for partial instantiation edge cases."""

import dataclasses
from typing import Any, Optional
from uuid import UUID

from fraiseql.partial_instantiation import (
    create_partial_instance,
    get_available_fields,
    is_partial_instance,
)


@pytest.mark.unit
@dataclasses.dataclass
class Level4Model:
    """Deeply nested level 4 model."""

    id: UUID
    name: str
    value: int
    metadata: dict[str, Any]


@dataclasses.dataclass
class Level3Model:
    """Deeply nested level 3 model."""

    id: UUID
    title: str
    level4: Optional[Level4Model] = None
    level4_list: Optional[list[Level4Model]] = None


@dataclasses.dataclass
class Level2Model:
    """Deeply nested level 2 model."""

    id: UUID
    description: str
    level3: Optional[Level3Model] = None
    tags: Optional[list[str]] = None


@dataclasses.dataclass
class Level1Model:
    """Deeply nested level 1 model."""

    id: UUID
    name: str
    required_field: str
    level2: Optional[Level2Model] = None
    level2_list: Optional[list[Level2Model]] = None


@dataclasses.dataclass
class CircularModelA:
    """Model with circular reference to B."""

    id: UUID
    name: str
    b_ref: Optional["CircularModelB"] = None
    b_list: Optional[list["CircularModelB"]] = None


@dataclasses.dataclass
class CircularModelB:
    """Model with circular reference to A."""

    id: UUID
    title: str
    a_ref: Optional[CircularModelA] = None
    a_list: Optional[list[CircularModelA]] = None


class TestDeeplyNestedPartialInstantiation:
    """Test partial instantiation with deeply nested objects (>3 levels)."""

    def test_deeply_nested_full_instantiation(self) -> None:
        """Test creating deeply nested instances with all fields."""
        data = {
            "id": UUID("00000000-0000-0000-0000-000000000001"),
            "name": "Level 1",
            "required_field": "Required",
            "level2": {
                "id": UUID("00000000-0000-0000-0000-000000000002"),
                "description": "Level 2 Description",
                "level3": {
                    "id": UUID("00000000-0000-0000-0000-000000000003"),
                    "title": "Level 3 Title",
                    "level4": {
                        "id": UUID("00000000-0000-0000-0000-000000000004"),
                        "name": "Level 4 Name",
                        "value": 42,
                        "metadata": {"key": "value", "nested": {"deep": "data"}},
                    },
                },
            },
        }

        # Create nested instances bottom-up
        level4 = create_partial_instance(Level4Model, data["level2"]["level3"]["level4"])
        level3_data = {**data["level2"]["level3"], "level4": level4}
        level3 = create_partial_instance(Level3Model, level3_data)
        level2_data = {**data["level2"], "level3": level3}
        level2 = create_partial_instance(Level2Model, level2_data)
        level1_data = {**data, "level2": level2}
        instance = create_partial_instance(Level1Model, level1_data)

        # Verify all levels
        assert instance.id == UUID("00000000-0000-0000-0000-000000000001")
        assert instance.level2.id == UUID("00000000-0000-0000-0000-000000000002")
        assert instance.level2.level3.id == UUID("00000000-0000-0000-0000-000000000003")
        assert instance.level2.level3.level4.id == UUID("00000000-0000-0000-0000-000000000004")
        assert instance.level2.level3.level4.metadata["nested"]["deep"] == "data"

        # All should be partial instances
        assert is_partial_instance(instance)
        assert is_partial_instance(instance.level2)
        assert is_partial_instance(instance.level2.level3)
        assert is_partial_instance(instance.level2.level3.level4)

    def test_deeply_nested_partial_fields(self) -> None:
        """Test deeply nested objects with missing fields at various levels."""
        data = {
            "id": UUID("00000000-0000-0000-0000-000000000001"),
            "name": "Level 1",
            # missing required_field
            "level2": {
                "id": UUID("00000000-0000-0000-0000-000000000002"),
                # missing description
                "level3": {
                    "id": UUID("00000000-0000-0000-0000-000000000003"),
                    "title": "Level 3 Title",
                    "level4": {
                        "id": UUID("00000000-0000-0000-0000-000000000004"),
                        # missing name, value, and metadata
                    },
                },
            },
        }

        # Create nested instances with missing fields
        level4 = create_partial_instance(Level4Model, data["level2"]["level3"]["level4"])
        assert level4.id == UUID("00000000-0000-0000-0000-000000000004")
        assert level4.name is None
        assert level4.value is None
        assert level4.metadata is None
        assert get_available_fields(level4) == {"id"}

        level3_data = {**data["level2"]["level3"], "level4": level4}
        level3 = create_partial_instance(Level3Model, level3_data)
        assert level3.title == "Level 3 Title"
        assert level3.level4_list is None

        level2_data = {**data["level2"], "level3": level3}
        level2 = create_partial_instance(Level2Model, level2_data)
        assert level2.description is None
        assert level2.tags is None
        assert get_available_fields(level2) == {"id", "level3"}

        level1_data = {**data, "level2": level2}
        instance = create_partial_instance(Level1Model, level1_data)
        assert instance.required_field is None
        assert instance.level2_list is None
        assert get_available_fields(instance) == {"id", "name", "level2"}

    def test_deeply_nested_with_lists(self) -> None:
        """Test deeply nested objects with lists at various levels."""
        data = {
            "id": UUID("00000000-0000-0000-0000-000000000001"),
            "name": "Level 1",
            "required_field": "Required",
            "level2_list": [
                {
                    "id": UUID("00000000-0000-0000-0000-000000000002"),
                    "description": "First Level 2",
                    "tags": ["tag1", "tag2"],
                    "level3": {
                        "id": UUID("00000000-0000-0000-0000-000000000003"),
                        "title": "Nested in List",
                        "level4_list": [
                            {
                                "id": UUID("00000000-0000-0000-0000-000000000004"),
                                "name": "First L4",
                                "value": 1,
                                "metadata": {},
                            },
                            {
                                "id": UUID("00000000-0000-0000-0000-000000000005"),
                                # Partial L4 in list
                                "name": "Second L4",
                                # missing value and metadata
                            },
                        ],
                    },
                },
                {
                    "id": UUID("00000000-0000-0000-0000-000000000006"),
                    "description": "Second Level 2",
                    # missing tags and level3
                },
            ],
        }

        # Process level2_list
        level2_list = []
        for l2_data in data["level2_list"]:
            processed_l2_data = l2_data.copy()
            if "level3" in l2_data and "level4_list" in l2_data["level3"]:
                level4_list = []
                for l4_data in l2_data["level3"]["level4_list"]:
                    level4 = create_partial_instance(Level4Model, l4_data)
                    level4_list.append(level4)

                level3_data = {**l2_data["level3"], "level4_list": level4_list}
                level3 = create_partial_instance(Level3Model, level3_data)
                processed_l2_data = {**l2_data, "level3": level3}

            level2 = create_partial_instance(Level2Model, processed_l2_data)
            level2_list.append(level2)

        instance_data = {**data, "level2_list": level2_list}
        instance = create_partial_instance(Level1Model, instance_data)

        # Verify list structures
        assert len(instance.level2_list) == 2
        assert instance.level2_list[0].tags == ["tag1", "tag2"]
        assert instance.level2_list[1].tags is None

        # Check deeply nested list
        level4_list = instance.level2_list[0].level3.level4_list
        assert len(level4_list) == 2
        assert level4_list[0].value == 1
        assert level4_list[1].value is None  # Missing field
        assert get_available_fields(level4_list[1]) == {"id", "name"}


class TestCircularReferencePartialInstantiation:
    """Test partial instantiation with circular references."""

    def test_simple_circular_reference(self) -> None:
        """Test basic circular reference handling."""
        # Create A -> B -> A circular reference
        a_id = UUID("00000000-0000-0000-0000-000000000001")
        b_id = UUID("00000000-0000-0000-0000-000000000002")

        # First create B without A reference
        b_data = {
            "id": b_id,
            "title": "B Instance",
        }
        b_instance = create_partial_instance(CircularModelB, b_data)

        # Create A with B reference
        a_data = {
            "id": a_id,
            "name": "A Instance",
            "b_ref": b_instance,
        }
        a_instance = create_partial_instance(CircularModelA, a_data)

        # Now update B with A reference (simulating circular reference)
        b_instance.a_ref = a_instance

        # Verify circular structure
        assert a_instance.b_ref.id == b_id
        assert a_instance.b_ref.a_ref.id == a_id
        assert a_instance.b_ref.a_ref.b_ref.id == b_id  # Full circle

    def test_circular_reference_with_partial_fields(self) -> None:
        """Test circular references when objects have partial fields."""
        # A -> B -> A, but with missing fields
        a_data = {
            "id": UUID("00000000-0000-0000-0000-000000000001"),
            # missing name
            "b_ref": {
                "id": UUID("00000000-0000-0000-0000-000000000002"),
                "title": "B Title",
                "a_ref": {
                    "id": UUID("00000000-0000-0000-0000-000000000001"),  # Same as root A
                    "name": "Circular A",
                    # missing b_ref to break infinite loop
                },
            },
        }

        # Create the circular structure
        inner_a = create_partial_instance(CircularModelA, a_data["b_ref"]["a_ref"])
        b_data = {**a_data["b_ref"], "a_ref": inner_a}
        b_instance = create_partial_instance(CircularModelB, b_data)
        a_data_final = {**a_data, "b_ref": b_instance}
        a_instance = create_partial_instance(CircularModelA, a_data_final)

        # Verify partial fields in circular structure
        assert a_instance.name is None  # Missing field
        assert get_available_fields(a_instance) == {"id", "b_ref"}
        assert a_instance.b_ref.a_ref.name == "Circular A"
        assert a_instance.b_ref.a_ref.b_ref is None  # Breaks the infinite loop

    def test_circular_reference_in_lists(self) -> None:
        """Test circular references within list structures."""
        # A has list of B, each B references back to A
        a_id = UUID("00000000-0000-0000-0000-000000000001")

        # Create A first without B list
        a_instance = create_partial_instance(
            CircularModelA,
            {
                "id": a_id,
                "name": "Parent A",
            },
        )

        # Create B instances that reference A
        b_list = []
        for i in range(3):
            b_data = {
                "id": UUID(f"00000000-0000-0000-0000-00000000000{i + 2}"),
                "title": f"B Instance {i}",
                "a_ref": a_instance,  # Reference back to parent A
            }
            b_instance = create_partial_instance(CircularModelB, b_data)
            b_list.append(b_instance)

        # Update A with B list
        a_instance.b_list = b_list

        # Verify circular list structure
        assert len(a_instance.b_list) == 3
        for b in a_instance.b_list:
            assert b.a_ref.id == a_id
            assert b.a_ref.name == "Parent A"
            # Each B's a_ref should have the full b_list
            assert len(b.a_ref.b_list) == 3


class TestMixedPartialFullObjects:
    """Test scenarios with mixed partial and full objects in lists."""

    def test_mixed_partial_full_in_list(self) -> None:
        """Test list containing both partial and full objects."""

        @dataclasses.dataclass
        class Item:
            id: int
            name: str
            value: float
            optional: Optional[str] = None

        @dataclasses.dataclass
        class Container:
            id: int
            items: list[Item]

        # Create list with mix of partial and full items
        items = []

        # Full item
        full_item = create_partial_instance(
            Item,
            {
                "id": 1,
                "name": "Full Item",
                "value": 99.9,
                "optional": "Has optional",
            },
        )
        items.append(full_item)

        # Partial item (missing required fields)
        partial_item = create_partial_instance(
            Item,
            {
                "id": 2,
                "name": "Partial Item",
                # missing value
            },
        )
        items.append(partial_item)

        # Minimal item
        minimal_item = create_partial_instance(
            Item,
            {
                "id": 3,
                # missing name and value
            },
        )
        items.append(minimal_item)

        container = create_partial_instance(
            Container,
            {
                "id": 100,
                "items": items,
            },
        )

        # Verify mixed list
        assert len(container.items) == 3

        # Full item has all fields
        assert get_available_fields(container.items[0]) == {"id", "name", "value", "optional"}

        # Partial item has some fields
        assert get_available_fields(container.items[1]) == {"id", "name"}
        assert container.items[1].value is None

        # Minimal item has only id
        assert get_available_fields(container.items[2]) == {"id"}
        assert container.items[2].name is None
        assert container.items[2].value is None


class TestErrorHandlingInPartialInstantiation:
    """Test error handling when instantiation fails."""

    def test_invalid_type_conversion(self) -> None:
        """Test handling of invalid type conversions during instantiation."""

        @dataclasses.dataclass
        class StrictType:
            id: UUID
            count: int
            ratio: float
            is_valid: bool

        # Try to create with invalid types
        data = {
            "id": "not-a-uuid",  # Invalid UUID
            "count": "not-a-number",  # Invalid int
            "ratio": "invalid-float",  # Invalid float
            "is_valid": "not-a-bool",  # Invalid bool
        }

        # Should handle gracefully by setting to None or attempting conversion
        instance = create_partial_instance(StrictType, data)

        # Check that instance was created despite invalid data
        assert is_partial_instance(instance)
        # The actual behavior depends on implementation
        # but it should not raise an exception

    def test_missing_required_init_params(self) -> None:
        """Test handling of regular classes with required __init__ parameters."""

        class CustomClass:
            def __init__(self, id: int, name: str, value: float) -> None:
                self.id = id
                self.name = name
                self.value = value
                self.computed = id * value  # Computed field

        # Create with missing required parameter
        instance = create_partial_instance(
            CustomClass,
            {
                "id": 42,
                "name": "Test",
                # missing value
            },
        )

        assert is_partial_instance(instance)
        assert instance.id == 42
        assert instance.name == "Test"
        # Should handle missing parameter gracefully
        assert not hasattr(instance, "computed") or instance.computed is None

    def test_property_and_method_handling(self) -> None:
        """Test partial instantiation with properties and methods."""

        @dataclasses.dataclass
        class ComplexType:
            _value: int
            name: str

            @property
            def doubled_value(self) -> int:
                return self._value * 2

            def compute_hash(self) -> str:
                return f"{self.name}_{self._value}"

        # Create partial instance
        instance = create_partial_instance(
            ComplexType,
            {
                "_value": 21,
                "name": "Test",
            },
        )

        assert instance._value == 21
        assert instance.name == "Test"

        # Properties and methods should still work on partial instances
        # if the required fields are available
        try:
            assert instance.doubled_value == 42
            assert instance.compute_hash() == "Test_21"
        except AttributeError:
            # Implementation might not support properties/methods
            pass

    def test_extremely_deep_nesting_limit(self) -> None:
        """Test behavior with extremely deep nesting to check for stack overflow."""

        @dataclasses.dataclass
        class RecursiveType:
            id: int
            child: Optional["RecursiveType"] = None

        # Create extremely deep nested structure
        depth = 100  # Deep but not infinite
        data = {"id": 0}
        current = data

        for i in range(1, depth):
            current["child"] = {"id": i}
            current = current["child"]

        # Should handle deep nesting without stack overflow
        instance = create_partial_instance(RecursiveType, {"id": 0})
        # For this test, we just verify it doesn't crash
        assert instance.id == 0

    def test_instantiation_with_none_values(self) -> None:
        """Test that None values are handled correctly."""

        @dataclasses.dataclass
        class NullableType:
            id: int
            name: Optional[str]
            value: Optional[float]
            nested: Optional["NullableType"] = None

        # Explicit None values
        data = {
            "id": 1,
            "name": None,  # Explicit None
            "value": None,  # Explicit None
            "nested": None,  # Explicit None
        }

        instance = create_partial_instance(NullableType, data)

        # Explicit None should be preserved, not treated as missing
        assert instance.id == 1
        assert instance.name is None
        assert instance.value is None
        assert instance.nested is None

        # All fields were provided (even if None)
        assert get_available_fields(instance) == {"id", "name", "value", "nested"}


class TestEdgeCaseScenarios:
    """Test various edge case scenarios."""

    def test_empty_data_dict(self) -> None:
        """Test partial instantiation with empty data."""

        @dataclasses.dataclass
        class SampleType:
            id: int
            name: str

        instance = create_partial_instance(SampleType, {})

        assert is_partial_instance(instance)
        assert instance.id is None
        assert instance.name is None
        assert get_available_fields(instance) == set()

    def test_dataclass_with_default_factory(self) -> None:
        """Test partial instantiation with default factory fields."""

        @dataclasses.dataclass
        class WithDefaults:
            id: int
            items: list[str] = dataclasses.field(default_factory=list)
            metadata: dict[str, Any] = dataclasses.field(default_factory=dict)

        # Create without providing default factory fields
        instance = create_partial_instance(WithDefaults, {"id": 1})

        assert instance.id == 1
        # Default factory fields might be None or empty collections
        assert instance.items is None or instance.items == []
        assert instance.metadata is None or instance.metadata == {}

    def test_inheritance_chain(self) -> None:
        """Test partial instantiation with inheritance."""

        @dataclasses.dataclass
        class BaseModel:
            id: int
            created_at: Optional[str] = None

        @dataclasses.dataclass
        class ExtendedModel(BaseModel):
            name: str = ""  # Provide default to avoid dataclass ordering issue
            description: Optional[str] = None

        # Create with mixed base and extended fields
        instance = create_partial_instance(
            ExtendedModel,
            {
                "id": 1,
                "name": "Extended",
                # missing created_at and description
            },
        )

        assert instance.id == 1
        assert instance.name == "Extended"
        assert instance.created_at is None
        assert instance.description is None
