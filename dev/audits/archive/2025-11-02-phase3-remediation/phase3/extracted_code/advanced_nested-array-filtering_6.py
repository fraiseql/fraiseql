# Extracted from: docs/advanced/nested-array-filtering.md
# Block number: 6
print_servers: list[PrintServer] = fraise_field(
    default_factory=list, supports_where_filtering=True, nested_where_type=PrintServer
)
