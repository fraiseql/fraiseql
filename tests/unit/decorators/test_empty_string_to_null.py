"""Tests for automatic empty string to null conversion in mutations.

This addresses the issue where frontends send empty strings when clearing
text fields, but the backend should convert them to NULL for nullable fields.

See: https://github.com/printoptim/printoptim-front/issues/35
"""

import pytest

import fraiseql
from fraiseql.mutations.decorators import error, success
from fraiseql.types.fraise_input import fraise_input


@fraise_input
class UpdateNoteInput:
    id: str
    notes: str | None = None  # Optional field - empty string should convert to None


@fraiseql.type
class Note:
    id: str
    notes: str | None


@success
class UpdateNoteSuccess:
    message: str
    note: Note


@error
class UpdateNoteError:
    message: str
    code: str = "ERROR"


@pytest.mark.unit
class TestEmptyStringToNullConversion:
    """Test that empty strings are converted to None for optional fields in mutations."""

    def test_optional_field_accepts_empty_string_in_input_type(self) -> None:
        """Optional string fields should accept empty strings without validation error."""
        # This should NOT raise a validation error since notes is optional
        input_obj = UpdateNoteInput(id="note-123", notes="")
        assert input_obj.notes == ""  # Should accept empty string initially

    def test_to_dict_converts_empty_string_to_none_for_optional_fields(self) -> None:
        """_to_dict should convert empty strings to None for optional fields."""
        from fraiseql.mutations.mutation_decorator import _to_dict

        input_obj = UpdateNoteInput(id="note-123", notes="")
        result = _to_dict(input_obj)

        # Empty string should be converted to None for nullable fields
        assert result["id"] == "note-123"
        assert result["notes"] is None  # Empty string converted to None

    def test_to_dict_preserves_non_empty_strings(self) -> None:
        """_to_dict should preserve non-empty strings."""
        from fraiseql.mutations.mutation_decorator import _to_dict

        input_obj = UpdateNoteInput(id="note-123", notes="Important note")
        result = _to_dict(input_obj)

        assert result["notes"] == "Important note"

    def test_to_dict_preserves_explicit_none(self) -> None:
        """_to_dict should preserve explicit None values."""
        from fraiseql.mutations.mutation_decorator import _to_dict

        input_obj = UpdateNoteInput(id="note-123", notes=None)
        result = _to_dict(input_obj)

        assert result["notes"] is None

    def test_required_string_still_rejects_empty_string(self) -> None:
        """Required string fields should still reject empty strings."""

        @fraise_input
        class RequiredInput:
            name: str  # Required - should reject empty string

        with pytest.raises(ValueError, match="Field 'name' cannot be empty"):
            RequiredInput(name="")

    def test_optional_field_with_default_accepts_empty_string(self) -> None:
        """Optional fields with defaults should accept empty strings."""

        @fraise_input
        class OptionalWithDefaultInput:
            name: str = "default"  # Has default - technically optional

        # Should accept empty string but convert to None
        from fraiseql.mutations.mutation_decorator import _to_dict

        input_obj = OptionalWithDefaultInput(name="")
        result = _to_dict(input_obj)

        # Empty string should be converted to None for optional fields
        assert result["name"] is None
