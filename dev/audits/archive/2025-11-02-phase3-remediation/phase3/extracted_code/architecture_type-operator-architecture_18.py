# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 18
@fraise_input
class StringFilter:
    eq: str | None = None
    neq: str | None = None
    contains: str | None = None
    startswith: str | None = None
    endswith: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None
