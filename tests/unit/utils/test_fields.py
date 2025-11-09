import pytest

from fraiseql.fields import fraise_field
from fraiseql.utils.fields import patch_missing_field_types


@pytest.mark.unit
class TestClassWithFraiseQLField:
    field1: str = fraise_field()
    field2: int = fraise_field()


@pytest.fixture
def test_class() -> None:
    patch_missing_field_types(TestClassWithFraiseQLField)
    return TestClassWithFraiseQLField


def test_patch_missing_field_types(test_class) -> None:
    """Test that patch_missing_field_types sets field_type for fields."""
    # Ensure the fields exist
    assert hasattr(test_class, "field1"), "field1 was not found in the class"
    assert hasattr(test_class, "field2"), "field2 was not found in the class"

    # Check that `field_type` is properly set
    field1 = test_class.field1
    field2 = test_class.field2

    assert field1.field_type is str, (
        f"Expected field1 to have type 'str', but got {field1.field_type}"
    )
    assert field2.field_type is int, (
        f"Expected field2 to have type 'int', but got {field2.field_type}"
    )
