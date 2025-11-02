# Extracted from: docs/advanced/nested-array-filtering.md
# Block number: 7
@auto_nested_array_filters  # Just add this decorator
@fraise_type
class NetworkConfiguration:
    print_servers: list[PrintServer] = fraise_field(default_factory=list)
