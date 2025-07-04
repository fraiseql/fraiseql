# tests/types/test_fraise_type_decorator.py

from dataclasses import is_dataclass

from fraiseql.types.fraise_type import fraise_type


def test_fraise_type_applies_dataclass_behavior() -> None:
    @fraise_type
    class Example:
        name: str
        count: int

    instance = Example(name="X", count=3)
    # FraiseQL types are not dataclasses, they have their own type system
    assert not is_dataclass(Example)
    assert hasattr(Example, "__fraiseql_definition__")
    assert hasattr(Example, "__gql_typename__")
    assert instance.name == "X"
    assert instance.count == 3
