# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 3
# 1. GraphQL Scalar Type Definition
DateRangeScalar = GraphQLScalarType(
    name="DateRange",
    description="Date range values",
    serialize=serialize_date_range,  # Python -> JSON
    parse_value=parse_date_range_value,  # JSON -> Python
    parse_literal=parse_date_range_literal,  # GraphQL AST -> Python
)


# 2. Python Marker Class
class DateRangeField(str, ScalarMarker):
    """Python-side marker for the DateRange scalar."""

    __slots__ = ()

    def __repr__(self) -> str:
        return "DateRange"


# 3. Validation Functions
def serialize_date_range(value: Any) -> str:
    """Convert Python value to serializable form."""
    if isinstance(value, str):
        return value
    raise GraphQLError(f"Invalid value: {value!r}")


def parse_date_range_value(value: Any) -> str:
    """Convert JSON input to Python type."""
    if isinstance(value, str):
        # Validate format: [YYYY-MM-DD, YYYY-MM-DD] or (YYYY-MM-DD, YYYY-MM-DD)
        pattern = r"^[\[\(](\d{4}-\d{2}-\d{2}),\s*(\d{4}-\d{2}-\d{2})[\]\)]$"
        if not re.match(pattern, value):
            raise GraphQLError(f"Invalid format: {value}")
        return value
    raise GraphQLError(f"Expected string, got {type(value)}")


def parse_date_range_literal(ast: ValueNode, variables: dict[str, Any] | None = None) -> str:
    """Convert GraphQL AST literal to Python type."""
    if isinstance(ast, StringValueNode):
        return parse_date_range_value(ast.value)
    raise GraphQLError("Expected string literal")
