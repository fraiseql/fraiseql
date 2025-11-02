# Extracted from: docs/reference/decorators.md
# Block number: 9
from fraiseql import mutation


@mutation
async def mutation_name(info, input: InputType) -> ReturnType:
    pass
