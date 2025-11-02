# Extracted from: docs/guides/nested-array-filtering.md
# Block number: 3
field_name: list[Type] = fraise_field(
    default_factory=list,
    supports_where_filtering=True,  # Required!
    nested_where_type=Type,  # Required!
)
