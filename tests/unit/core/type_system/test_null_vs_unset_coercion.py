import pytest

"""Test null vs unset field handling in coercion."""

import fraiseql
from fraiseql.types.coercion import coerce_input, coerce_input_arguments
from fraiseql.types.definitions import UNSET


@pytest.mark.unit
@fraiseql.input
class SampleInput:
    """Input type with optional fields that default to UNSET."""

    required_field: str
    optional_with_unset: str | None = UNSET
    optional_with_none: str | None = None
    optional_no_default: str | None = fraiseql.fraise_field(default=UNSET)


def test_coerce_input_omitted_fields_use_default() -> None:
    """Test that omitted fields use their default values in coerce_input."""
    # Test with only required field
    raw_data = {"required_field": "test_value"}

    result = coerce_input(SampleInput, raw_data)

    # Check that required field is set
    assert result.required_field == "test_value"

    # Check that fields with UNSET default have UNSET value
    assert result.optional_with_unset is UNSET
    assert result.optional_no_default is UNSET

    # Check that field with None default has None value
    assert result.optional_with_none is None


def test_coerce_input_explicit_null_overrides_default() -> None:
    """Test that explicit null values override default values."""
    # Test with explicit nulls
    raw_data = {
        "required_field": "test_value",
        "optional_with_unset": None,
        "optional_with_none": None,
        "optional_no_default": None,
    }

    result = coerce_input(SampleInput, raw_data)

    # Check that all fields have None, not their defaults
    assert result.required_field == "test_value"
    assert result.optional_with_unset is None  # Not UNSET
    assert result.optional_with_none is None
    assert result.optional_no_default is None  # Not UNSET


def test_coerce_input_arguments_preserves_omitted_fields() -> None:
    """Test that coerce_input_arguments doesn't add omitted fields."""

    # Define a test function
    def test_func(input: SampleInput) -> None:
        pass

    # Test with only required field in raw args
    raw_args = {"input": {"required_field": "test_value"}}

    coerced = coerce_input_arguments(test_func, raw_args)

    # The input should be coerced properly
    assert "input" in coerced
    input_obj = coerced["input"]
    assert input_obj.required_field == "test_value"
    assert input_obj.optional_with_unset is UNSET
    assert input_obj.optional_no_default is UNSET
    assert input_obj.optional_with_none is None


def test_sql_generation_excludes_unset_fields() -> None:
    """Test that UNSET fields are excluded from SQL generation."""
    from dataclasses import dataclass

    from fraiseql.mutations.sql_generator import generate_insert_json_call

    @dataclass
    class SQLTestInput:
        """Input for SQL generation test."""

        required_field: str
        explicit_null: str | None = None
        unset_field: str | None = UNSET

    # Create input with some fields
    input_obj = SQLTestInput(
        required_field="test_value",
        explicit_null=None,
        # unset_field is not provided, so it gets UNSET default
    )

    # Generate SQL
    query = generate_insert_json_call(
        input_object=input_obj,
        context={"tenant_id": "test-tenant"},
        sql_function_name="test_function",
    )

    # The JSON data should only include required_field and explicit_null
    # unset_field should be excluded
    assert "input_json" in query.params
    # Jsonb wraps the actual data
    json_data = query.params["input_json"].obj
    assert json_data == {"required_field": "test_value", "explicit_null": None}
    assert "unset_field" not in json_data


if __name__ == "__main__":
    test_coerce_input_omitted_fields_use_default()
    # test_coerce_input_omitted_fields_use_default passed

    test_coerce_input_explicit_null_overrides_default()
    # test_coerce_input_explicit_null_overrides_default passed

    test_coerce_input_arguments_preserves_omitted_fields()
    # test_coerce_input_arguments_preserves_omitted_fields passed

    test_sql_generation_excludes_unset_fields()
    # test_sql_generation_excludes_unset_fields passed

    # All tests passed!
