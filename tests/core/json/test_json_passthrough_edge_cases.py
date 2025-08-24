"""Edge case tests for JSON passthrough to ensure robustness."""

from decimal import Decimal
from typing import Any, Optional
from uuid import UUID, uuid4

import pytest

import fraiseql
from fraiseql.config.schema_config import SchemaConfig
from fraiseql.core.json_passthrough import JSONPassthrough, is_json_passthrough



@pytest.mark.unit
@fraiseql.type
class NestedType:
    """Nested type for testing."""

    value: str
    count: int


@fraiseql.type
class ComplexType:
    """Complex type with various field types."""

    id: UUID
    name: str
    score: Decimal
    metadata: dict[str, Any]
    tags: list[str]
    nested: Optional[NestedType] = None


class TestJSONPassthroughEdgeCases:
    """Test edge cases and corner scenarios for JSON passthrough."""

    def test_decimal_handling(self):
        """Test that Decimal values are handled correctly."""
        data = {
            "id": str(uuid4()),
            "name": "Test",
            "score": "99.99",  # Decimals often come as strings from JSON
            "metadata": {},
            "tags": [],
        }

        wrapped = JSONPassthrough(data, "ComplexType", ComplexType)

        # Should return the string representation
        assert wrapped.score == "99.99"

    def test_uuid_handling(self):
        """Test that UUID values are handled correctly."""
        test_uuid = uuid4()
        data = {
            "id": str(test_uuid),  # UUIDs come as strings from JSON
            "name": "Test",
            "score": "0",
            "metadata": {},
            "tags": [],
        }

        wrapped = JSONPassthrough(data, "ComplexType", ComplexType)

        # Should return the string representation
        assert wrapped.id == str(test_uuid)

    def test_self_referencing_structure(self):
        """Test that self-referencing structures are handled properly."""
        data = {
            "id": str(uuid4()),
            "name": "Parent",
            "score": "0",
            "metadata": {"parent_ref": {"id": str(uuid4()), "name": "Referenced", "score": "0"}},
            "tags": [],
            "nested": None,
        }

        wrapped = JSONPassthrough(data, "ComplexType", ComplexType)

        # metadata field might be wrapped or not depending on type hints
        # Since metadata is dict[str, Any], it should be returned as-is
        metadata = wrapped.metadata
        if is_json_passthrough(metadata):
            # If wrapped, we can't subscript directly
            # This would be a bug - dicts should not be wrapped
            pytest.fail("metadata dict should not be wrapped in JSONPassthrough")
        else:
            # Should be a regular dict
            assert isinstance(metadata, dict)
            parent_ref = metadata["parent_ref"]
            assert isinstance(parent_ref, dict)
            assert parent_ref["name"] == "Referenced"

    def test_empty_collections(self):
        """Test handling of empty lists and dicts."""
        data = {"id": str(uuid4()), "name": "Test", "score": "0", "metadata": {}, "tags": []}

        wrapped = JSONPassthrough(data, "ComplexType", ComplexType)

        assert wrapped.metadata == {}
        assert wrapped.tags == []

    def test_large_nested_structure(self):
        """Test handling of deeply nested structures."""
        # Create a deeply nested structure
        deep_data = {"value": "bottom", "count": 100}
        for i in range(10):
            deep_data = {"value": f"level_{i}", "count": i, "nested": deep_data}

        data = {
            "id": str(uuid4()),
            "name": "Test",
            "score": "0",
            "metadata": {"deep": deep_data},
            "tags": [],
        }

        wrapped = JSONPassthrough(data, "ComplexType", ComplexType)

        # Should handle deep nesting in metadata
        assert isinstance(wrapped.metadata, dict)
        assert "deep" in wrapped.metadata

    def test_special_characters_in_field_names(self):
        """Test handling of special characters in field names."""
        data = {
            "id": str(uuid4()),
            "name": "Test",
            "score": "0",
            "metadata": {
                "key-with-dash": "value1",
                "key.with.dots": "value2",
                "key with spaces": "value3",
                "$special@chars!": "value4",
            },
            "tags": [],
        }

        wrapped = JSONPassthrough(data, "ComplexType", ComplexType)

        # Metadata should preserve special characters
        assert wrapped.metadata["key-with-dash"] == "value1"
        assert wrapped.metadata["key.with.dots"] == "value2"
        assert wrapped.metadata["key with spaces"] == "value3"
        assert wrapped.metadata["$special@chars!"] == "value4"

    def test_unicode_handling(self):
        """Test handling of Unicode characters."""
        data = {
            "id": str(uuid4()),
            "name": "测试 テスト тест 🚀",  # Various Unicode chars
            "score": "0",
            "metadata": {
                "emoji": "😀🎉🌟",
                "chinese": "你好世界",
                "japanese": "こんにちは",
                "russian": "Привет мир",
            },
            "tags": ["标签", "タグ", "тег"],
        }

        wrapped = JSONPassthrough(data, "ComplexType", ComplexType)

        assert wrapped.name == "测试 テスト тест 🚀"
        assert wrapped.metadata["emoji"] == "😀🎉🌟"
        assert wrapped.tags[0] == "标签"

    def test_mixed_case_field_access(self):
        """Test that field access works with various casing styles."""
        config = SchemaConfig.get_instance()
        original_setting = config.camel_case_fields
        config.camel_case_fields = True

        try:
            data = {
                "mixedCaseField": "value1",
                "PascalCaseField": "value2",
                "snake_case_field": "value3",
                "UPPER_CASE_FIELD": "value4",
                "kebab-case-field": "value5",  # Not a valid Python identifier
            }

            wrapped = JSONPassthrough(data, "TestType")

            # Should access various case styles
            assert wrapped.mixedCaseField == "value1"
            assert wrapped.PascalCaseField == "value2"
            assert wrapped.snake_case_field == "value3"
            assert wrapped.UPPER_CASE_FIELD == "value4"

            # Kebab-case can't be accessed as attribute (not valid Python)
            assert "kebab-case-field" in wrapped._data

        finally:
            config.camel_case_fields = original_setting

    def test_performance_with_many_fields(self):
        """Test performance doesn't degrade with many fields."""
        # Create object with many fields
        data = {f"field_{i}": f"value_{i}" for i in range(1000)}
        data["__typename"] = "ManyFieldsType"

        wrapped = JSONPassthrough(data)

        # Access should be fast even with many fields
        assert wrapped.field_0 == "value_0"
        assert wrapped.field_500 == "value_500"
        assert wrapped.field_999 == "value_999"

        # Check cache is working
        assert len(wrapped._wrapped_cache) == 0  # Scalars aren't cached

    def test_boolean_field_handling(self):
        """Test that boolean fields are handled correctly."""
        data = {
            "id": str(uuid4()),
            "name": "Test",
            "score": "0",
            "metadata": {"is_active": True, "is_deleted": False, "is_none": None},
            "tags": [],
        }

        wrapped = JSONPassthrough(data, "ComplexType", ComplexType)

        assert wrapped.metadata["is_active"] is True
        assert wrapped.metadata["is_deleted"] is False
        assert wrapped.metadata["is_none"] is None

    def test_numeric_field_handling(self):
        """Test various numeric types."""
        data = {
            "id": str(uuid4()),
            "name": "Test",
            "score": "0",
            "metadata": {
                "int_value": 42,
                "float_value": 3.14159,
                "negative": -100,
                "zero": 0,
                "large_number": 9999999999999999,
                "scientific": 1.23e-4,
            },
            "tags": [],
        }

        wrapped = JSONPassthrough(data, "ComplexType", ComplexType)

        assert wrapped.metadata["int_value"] == 42
        assert wrapped.metadata["float_value"] == 3.14159
        assert wrapped.metadata["negative"] == -100
        assert wrapped.metadata["zero"] == 0
        assert wrapped.metadata["large_number"] == 9999999999999999
        assert wrapped.metadata["scientific"] == 1.23e-4

    def test_error_messages_with_suggestions(self):
        """Test that error messages provide helpful suggestions."""
        data = {"firstName": "John", "lastName": "Doe", "emailAddress": "john@example.com"}

        wrapped = JSONPassthrough(data, "User")

        # The wrapper should handle case conversion automatically
        # So first_name should work for firstName
        assert wrapped.first_name == "John"  # Auto-converts to firstName

        # Try to access a field that truly doesn't exist
        with pytest.raises(AttributeError) as exc_info:
            _ = wrapped.middle_name  # This field doesn't exist at all

        error_msg = str(exc_info.value)
        assert "middle_name" in error_msg
        assert "firstName" in error_msg or "lastName" in error_msg  # Should show available fields

    def test_list_of_mixed_types(self):
        """Test handling lists with mixed types."""
        data = {
            "id": str(uuid4()),
            "name": "Test",
            "score": "0",
            "metadata": {},
            "tags": ["string", 123, True, None, {"nested": "object"}],
        }

        wrapped = JSONPassthrough(data, "ComplexType", ComplexType)

        tags = wrapped.tags
        assert tags[0] == "string"
        assert tags[1] == 123
        assert tags[2] is True
        assert tags[3] is None
        assert isinstance(tags[4], dict)
        assert tags[4]["nested"] == "object"

    def test_passthrough_with_graphql_introspection_fields(self):
        """Test handling of GraphQL introspection fields."""
        data = {
            "__typename": "ComplexType",
            "__schema": {"should": "ignore"},
            "__type": {"should": "also_ignore"},
            "id": str(uuid4()),
            "name": "Test",
            "score": "0",
            "metadata": {},
            "tags": [],
        }

        wrapped = JSONPassthrough(data)

        assert wrapped.__typename == "ComplexType"
        # These shouldn't be accessible as regular fields
        assert wrapped.id == data["id"]
        assert wrapped.name == "Test"