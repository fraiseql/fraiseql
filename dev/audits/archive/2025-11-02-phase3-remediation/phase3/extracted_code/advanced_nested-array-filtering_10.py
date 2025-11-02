# Extracted from: docs/advanced/nested-array-filtering.md
# Block number: 10
# Create enhanced resolver with where support
create_nested_array_field_resolver_with_where(
    field_name: str,
    field_type: Any,
    field_metadata: Any = None
) -> AsyncResolver

# Generate WhereInput types
create_graphql_where_input(cls: type, name: str | None = None) -> type
