# Extracted from: docs/advanced/nested-array-filtering.md
# Block number: 8
# Automatic registration
enable_nested_array_filtering(parent_type: Type) -> None

# Manual registration
register_nested_array_filter(parent_type: Type, field_name: str, element_type: Type) -> None

# Query functions
get_nested_array_filter(parent_type: Type, field_name: str) -> Type | None
is_nested_array_filterable(parent_type: Type, field_name: str) -> bool
list_registered_filters() -> dict[str, dict[str, str]]

# Utility
clear_registry() -> None  # For testing
