# Extracted from: docs/advanced/nested-array-filtering.md
# Block number: 9
# Automatic detection for all list[FraiseQLType] fields
@auto_nested_array_filters
class MyType: ...


# Selective registration for specific fields
@nested_array_filterable("field1", "field2")
class MyType: ...
