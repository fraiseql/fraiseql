# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 24
# src/fraiseql/types/scalars/my_type.py


def serialize_my_type(value: Any) -> str:
    """Serialize to GraphQL output."""


def parse_my_type_value(value: Any) -> str:
    """Parse from GraphQL input."""


def parse_my_type_literal(ast: ValueNode, variables: dict | None = None) -> str:
    """Parse from GraphQL literal."""


MyTypeScalar = GraphQLScalarType(
    name="MyType",
    serialize=serialize_my_type,
    parse_value=parse_my_type_value,
    parse_literal=parse_my_type_literal,
)


class MyTypeField(str, ScalarMarker):
    __slots__ = ()

    def __repr__(self) -> str:
        return "MyType"
