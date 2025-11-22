"""Tests for empty string validation in FraiseQL input types.

This module tests that required string fields (name: str) properly reject:
- Empty strings ("")
- Whitespace-only strings ("   ")

While accepting:
- Valid non-empty strings ("valid")
- null values for optional fields (name: str | None = None)
"""

import pytest

from fraiseql.types.fraise_input import fraise_input


@pytest.mark.unit
def test_required_string_rejects_empty_string() -> None:
    """Required string fields should reject empty strings."""

    @fraise_input
    class TestInput:
        name: str

    # Empty string should raise ValueError
    with pytest.raises(ValueError, match="Field 'name' cannot be empty"):
        TestInput(name="")


@pytest.mark.unit
def test_required_string_rejects_whitespace_only() -> None:
    """Required string fields should reject whitespace-only strings."""

    @fraise_input
    class TestInput:
        name: str

    # Whitespace-only string should raise ValueError
    with pytest.raises(ValueError, match="Field 'name' cannot be empty"):
        TestInput(name="   ")


@pytest.mark.unit
def test_required_string_rejects_tab_and_newline() -> None:
    """Required string fields should reject tab/newline-only strings."""

    @fraise_input
    class TestInput:
        name: str

    # Tab-only string should raise ValueError
    with pytest.raises(ValueError, match="Field 'name' cannot be empty"):
        TestInput(name="\t")

    # Newline-only string should raise ValueError
    with pytest.raises(ValueError, match="Field 'name' cannot be empty"):
        TestInput(name="\n")

    # Mixed whitespace should raise ValueError
    with pytest.raises(ValueError, match="Field 'name' cannot be empty"):
        TestInput(name=" \t\n ")


@pytest.mark.unit
def test_required_string_accepts_valid_strings() -> None:
    """Required string fields should accept non-empty strings."""

    @fraise_input
    class TestInput:
        name: str

    # Valid strings should work
    instance1 = TestInput(name="valid")
    assert instance1.name == "valid"

    instance2 = TestInput(name="a")
    assert instance2.name == "a"

    instance3 = TestInput(name="  valid with spaces  ")
    assert instance3.name == "  valid with spaces  "


@pytest.mark.unit
def test_optional_string_allows_none() -> None:
    """Optional string fields should allow None values."""

    @fraise_input
    class TestInput:
        name: str | None = None

    # None should be allowed for optional fields
    instance = TestInput(name=None)
    assert instance.name is None

    # Default None should work
    instance2 = TestInput()
    assert instance2.name is None


@pytest.mark.unit
def test_optional_string_accepts_empty_when_provided() -> None:
    """Optional string fields should accept empty strings (they will be converted to None)."""

    @fraise_input
    class TestInput:
        name: str | None = None

    # Optional fields should accept empty strings (will be converted to None by to_dict)
    instance1 = TestInput(name="")
    assert instance1.name == ""  # Stored as empty string in the object

    instance2 = TestInput(name="  ")
    assert instance2.name == "  "  # Stored as whitespace in the object

    # But when converted to dict for database, empty strings become None
    assert instance1.to_dict()["name"] is None
    assert instance2.to_dict()["name"] is None


@pytest.mark.unit
def test_multiple_required_strings() -> None:
    """Multiple required string fields should all be validated."""

    @fraise_input
    class TestInput:
        name: str
        description: str

    # All fields should be validated
    with pytest.raises(ValueError, match="Field 'name' cannot be empty"):
        TestInput(name="", description="valid")

    with pytest.raises(ValueError, match="Field 'description' cannot be empty"):
        TestInput(name="valid", description="")

    # Valid case should work
    instance = TestInput(name="valid name", description="valid desc")
    assert instance.name == "valid name"
    assert instance.description == "valid desc"


@pytest.mark.unit
def test_mixed_string_and_non_string_fields() -> None:
    """String validation should only apply to string fields."""

    @fraise_input
    class TestInput:
        name: str
        age: int
        active: bool

    # String field should be validated
    with pytest.raises(ValueError, match="Field 'name' cannot be empty"):
        TestInput(name="", age=25, active=True)

    # Non-string fields should not be affected
    instance = TestInput(name="valid", age=0, active=False)
    assert instance.name == "valid"
    assert instance.age == 0
    assert instance.active is False


@pytest.mark.unit
def test_error_message_includes_field_name() -> None:
    """Error message should clearly identify which field is invalid."""

    @fraise_input
    class TestInput:
        first_name: str
        last_name: str

    # Error should specify the exact field name
    with pytest.raises(ValueError, match="Field 'first_name' cannot be empty"):
        TestInput(first_name="", last_name="valid")

    with pytest.raises(ValueError, match="Field 'last_name' cannot be empty"):
        TestInput(first_name="valid", last_name="   ")


@pytest.mark.unit
def test_inheritance_preserves_string_validation() -> None:
    """String validation should work with inherited input types."""

    @fraise_input
    class BaseInput:
        name: str

    @fraise_input
    class ChildInput(BaseInput):
        description: str

    # Both base and child fields should be validated
    with pytest.raises(ValueError, match="Field 'name' cannot be empty"):
        ChildInput(name="", description="valid")

    with pytest.raises(ValueError, match="Field 'description' cannot be empty"):
        ChildInput(name="valid", description="")

    # Valid case should work
    instance = ChildInput(name="valid name", description="valid desc")
    assert instance.name == "valid name"
    assert instance.description == "valid desc"
