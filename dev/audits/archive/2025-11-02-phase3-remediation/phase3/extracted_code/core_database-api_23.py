# Extracted from: docs/core/database-api.md
# Block number: 23
from fraiseql.sql import create_graphql_where_input

MachineWhereInput = create_graphql_where_input(Machine)
AllocationWhereInput = create_graphql_where_input(Allocation)

where = AllocationWhereInput(machine=MachineWhereInput(name=StringFilter(eq="Server-01")))
results = await repo.find("allocations", where=where)
