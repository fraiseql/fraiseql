from typing import Annotated
from uuid import UUID

import pytest

from fraiseql.fields import fraise_field
from fraiseql.types.fraise_input import fraise_input
from fraiseql.types.scalars.json import JSONField


@pytest.mark.unit
def test_simple_fraise_input_definition() -> None:
    @fraise_input
    class SimpleInput:
        name: str
        age: int

    assert hasattr(SimpleInput, "__fraiseql_definition__")
    fields = SimpleInput.__fraiseql_definition__.fields
    assert set(fields) == {"name", "age"}
    assert SimpleInput.__fraiseql_definition__.kind == "input"


def test_fraise_input_with_explicit_fraise_field() -> None:
    @fraise_input
    class WithFraiseField:
        email: Annotated[str, fraise_field(description="User email")]

    field = WithFraiseField.__fraiseql_definition__.field_map["email"]
    assert field.description == "User email"
    assert field.name == "email"
    assert field.field_type is str


def test_fraise_input_default_and_factory() -> None:
    @fraise_input
    class WithDefaults:
        age: int = 42
        tags: list[str] = fraise_field(default_factory=list)

    instance = WithDefaults()
    assert instance.age == 42
    assert isinstance(instance.tags, list)


def test_fraise_input_supports_none_and_optional_json() -> None:
    @fraise_input
    class NullableInput:
        details: JSONField | None = None

    instance = NullableInput()
    assert instance.details is None


def test_fraise_input_field_ordering() -> None:
    @fraise_input
    class OrderedInput:
        a: str
        b: int = 1
        c: list[str] = fraise_field(default_factory=list)

    fields = OrderedInput.__fraiseql_definition__.fields
    assert list(fields) == ["a", "b", "c"]


def test_fraise_input_supports_inheritance() -> None:
    @fraise_input
    class BaseInput:
        id_: UUID = fraise_field(field_type=UUID)

    @fraise_input
    class ChildInput(BaseInput):
        name: str

    fields = ChildInput.__fraiseql_definition__.fields
    assert set(fields) == {"id_", "name"}
    assert ChildInput.__fraiseql_definition__.kind == "input"


def test_fraise_input_type_identity() -> None:
    @fraise_input
    class Identifiable:
        id: str

    assert Identifiable.__name__ == "Identifiable"
    assert Identifiable.__fraiseql_definition__.type is Identifiable
