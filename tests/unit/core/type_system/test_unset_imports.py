import pytest

"""Test that UNSET can be imported from different paths."""

import uuid
from datetime import datetime

from fraiseql import UNSET as UNSET_FROM_PACKAGE
from fraiseql import fraise_input
from fraiseql.types.definitions import UNSET as UNSET_FROM_DEFINITIONS


@pytest.mark.unit
def test_unset_imports_are_same() -> None:
    """Test that UNSET imported from different paths is the same object."""
    assert UNSET_FROM_PACKAGE is UNSET_FROM_DEFINITIONS


def test_both_import_styles_work_with_types() -> None:
    """Test that both import styles work with type annotations."""

    @fraise_input
    class TestInput1:
        """Using UNSET from package import."""

        id: uuid.UUID
        name: str | None = UNSET_FROM_PACKAGE
        value: int | None = UNSET_FROM_PACKAGE
        timestamp: datetime | None = UNSET_FROM_PACKAGE

    @fraise_input
    class TestInput2:
        """Using UNSET from definitions import."""

        id: uuid.UUID
        name: str | None = UNSET_FROM_DEFINITIONS
        value: int | None = UNSET_FROM_DEFINITIONS
        timestamp: datetime | None = UNSET_FROM_DEFINITIONS

    # Create instances
    obj1 = TestInput1(id=uuid.uuid4())
    obj2 = TestInput2(id=uuid.uuid4())

    # Verify UNSET values
    assert obj1.name is UNSET_FROM_PACKAGE
    assert obj2.name is UNSET_FROM_DEFINITIONS
    assert obj1.name is obj2.name  # Same UNSET instance
