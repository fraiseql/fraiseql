# Extracted from: docs/advanced/where_input_types.md
# Block number: 2
from fraiseql.sql import create_graphql_where_input

# Automatically generate UserWhereInput type
UserWhereInput = create_graphql_where_input(User)
