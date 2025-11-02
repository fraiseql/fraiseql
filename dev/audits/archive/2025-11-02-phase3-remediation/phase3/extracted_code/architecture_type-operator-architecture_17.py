# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 17
def safe_create_where_type(cls: type[object]) -> type[DynamicType]:
    """Create a WHERE clause type for a FraiseQL type.

    Generates a dataclass with:
    - Fields for each type attribute
    - A `to_sql()` method returning parameterized SQL (psycopg Composed)
    """
