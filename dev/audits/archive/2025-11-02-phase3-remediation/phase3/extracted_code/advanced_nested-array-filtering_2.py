# Extracted from: docs/advanced/nested-array-filtering.md
# Block number: 2
from fraiseql import query
from fraiseql.core.nested_field_resolver import create_nested_array_field_resolver_with_where
from fraiseql.sql.graphql_where_generator import create_graphql_where_input

# Create WhereInput type
PrintServerWhereInput = create_graphql_where_input(PrintServer)

# Create resolver with where filtering support
resolver = create_nested_array_field_resolver_with_where("print_servers", list[PrintServer])


# Use in GraphQL resolvers
@query
async def network_configuration_print_servers(
    parent: NetworkConfiguration,
    info: GraphQLResolveInfo,
    where: PrintServerWhereInput | None = None,
) -> list[PrintServer]:
    return await resolver(parent, info, where=where)
