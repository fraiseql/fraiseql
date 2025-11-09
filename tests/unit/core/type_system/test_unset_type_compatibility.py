"""Test UNSET type compatibility with type checkers."""

from __future__ import annotations

import uuid
from typing import Optional
from uuid import UUID

import pytest

from fraiseql import fraise_input
from fraiseql.types.definitions import UNSET, UnsetType


@pytest.mark.unit
def test_unset_singleton() -> None:
    """Test that UNSET implements singleton pattern."""
    # Create multiple instances
    unset1 = UnsetType()
    unset2 = UnsetType()

    # They should be the same instance
    assert unset1 is unset2
    assert unset1 is UNSET
    assert unset2 is UNSET


def test_unset_boolean_behavior() -> None:
    """Test UNSET boolean behavior."""
    assert not UNSET
    assert not bool(UNSET)

    # Should work in conditional statements
    if UNSET:
        pytest.fail("UNSET should be falsy")


def test_unset_string_representations() -> None:
    """Test UNSET string representations."""
    assert str(UNSET) == ""
    assert repr(UNSET) == "UNSET"


def test_unset_identity_checks() -> None:
    """Test that identity checks work correctly."""
    value = UNSET
    assert value is UNSET

    # None is not UNSET
    assert None is not UNSET
    assert UNSET is not None


def test_unset_type_annotations() -> None:
    """Test that UNSET works with various type annotations."""
    # These should not raise type errors with pyright/mypy

    # UUID | None
    uuid_field: UUID | None = UNSET
    assert uuid_field is UNSET

    # Optional[str]
    str_field: Optional[str] = UNSET
    assert str_field is UNSET

    # int | None
    int_field: int | None = UNSET
    assert int_field is UNSET

    # Complex nested type
    complex_field: dict[str, list[UUID | None]] | None = UNSET
    assert complex_field is UNSET


@fraise_input
class SampleInput:
    """Test input type with UNSET defaults."""

    required_field: str
    optional_uuid: UUID | None = UNSET
    optional_str: Optional[str] = UNSET
    optional_int: int | None = UNSET
    optional_list: list[str] | None = UNSET
    optional_dict: dict[str, int] | None = UNSET


def test_input_class_with_unset_defaults() -> None:
    """Test that input classes work with UNSET defaults."""
    # Create instance with only required field
    obj = SampleInput(required_field="test")

    # All optional fields should be UNSET
    assert obj.optional_uuid is UNSET
    assert obj.optional_str is UNSET
    assert obj.optional_int is UNSET
    assert obj.optional_list is UNSET
    assert obj.optional_dict is UNSET

    # Create instance with some fields set
    obj2 = SampleInput(
        required_field="test",
        optional_uuid=uuid.uuid4(),
        optional_str=None,  # Explicitly None
        # optional_int remains UNSET
        optional_list=[],
        # optional_dict remains UNSET
    )

    assert obj2.optional_uuid is not UNSET
    assert obj2.optional_str is None  # Explicitly None, not UNSET
    assert obj2.optional_int is UNSET
    assert obj2.optional_list == []
    assert obj2.optional_dict is UNSET


def test_unset_vs_none_distinction() -> None:
    """Test that we can distinguish between UNSET and None."""
    values = [UNSET, None, "", 0, False, []]

    # Only UNSET should be UNSET
    unset_count = sum(1 for v in values if v is UNSET)
    assert unset_count == 1

    # None is not UNSET
    assert None is not UNSET
    assert UNSET is not None


def test_unset_in_collections() -> None:
    """Test UNSET behavior in collections."""
    # List with UNSET
    lst = [1, UNSET, None, "test"]
    assert lst[1] is UNSET
    assert lst[2] is None

    # Dict with UNSET values
    dct = {"a": 1, "b": UNSET, "c": None}
    assert dct["b"] is UNSET
    assert dct["c"] is None

    # Set operations
    s = {UNSET, None, 1}
    assert UNSET in s
    assert None in s
