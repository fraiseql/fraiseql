# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 28
# src/fraiseql/sql/graphql_where_generator.py


@fraise_input
class MyTypeFilter:
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    my_special_op_1: str | None = None
    my_special_op_2: str | None = None
    isnull: bool | None = None
