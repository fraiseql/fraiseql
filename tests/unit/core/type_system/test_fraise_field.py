import pytest

from fraiseql.fields import fraise_field

# Test case to verify the field creation and purpose assignment


@pytest.mark.unit
def test_fraise_field_with_purpose() -> None:
    field = fraise_field(
        field_type=str,
        purpose="input",  # purpose should be one of 'input', 'output', or 'both'
    )

    # Assert that the purpose is correctly set
    assert field.purpose == "input", f"Expected purpose to be 'input', but got {field.purpose}"


def test_fraise_field_with_annotation() -> None:
    # Test that field_type is inferred from annotation if not passed explicitly
    class ExampleClass:
        my_field: str = fraise_field(field_type=str)

    field = ExampleClass.my_field

    # Assert that the field_type is correctly set to 'str'
    assert field.field_type is str, f"Expected field_type to be 'str', but got {field.field_type}"


def test_fraise_field_with_default() -> None:
    field = fraise_field(field_type=int, default=42)

    # Assert that the default value is correctly set
    assert field.default == 42, f"Expected default value to be 42, but got {field.default}"


def test_fraise_field_with_default_factory() -> None:
    # Test that the default_factory is correctly handled
    def default_value() -> int:
        return 100

    field = fraise_field(field_type=int, default_factory=default_value)

    # Assert that the default factory is correctly set
    assert field.default_factory() == 100, (
        f"Expected default value to be 100, but got {field.default_factory()}"
    )


def test_fraise_field_with_invalid_purpose() -> None:
    # Test that an invalid purpose raises an error
    with pytest.raises(ValueError, match="Invalid purpose for FraiseQLField"):
        fraise_field(field_type=str, purpose="invalid")
