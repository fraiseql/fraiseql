# Extracted from: docs/advanced/nested-array-filtering.md
# Block number: 5
# Check registered filters
from fraiseql.nested_array_filters import list_registered_filters

filters = list_registered_filters()
print("Registered filters:", filters)

# Verify WhereInput structure
PrintServerWhereInput = create_graphql_where_input(PrintServer)
where_input = PrintServerWhereInput()
print("Available fields:", dir(where_input))
