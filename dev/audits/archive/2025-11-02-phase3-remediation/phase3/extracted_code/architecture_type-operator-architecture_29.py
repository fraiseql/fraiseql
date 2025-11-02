# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 29
# src/fraiseql/sql/where/core/field_detection.py


class FieldType(Enum):
    MY_TYPE = "my_type"


@classmethod
def from_python_type(cls, python_type: type) -> "FieldType":
    try:
        from fraiseql.types.scalars.my_type import MyTypeField

        if python_type == MyTypeField or issubclass(python_type, MyTypeField):
            return cls.MY_TYPE
    except ImportError:
        pass
