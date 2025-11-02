# Extracted from: docs/reference/decorators.md
# Block number: 10
from fraiseql import type, query, mutation, input, field

@mutation(
    function: str | None = None,
    schema: str | None = None,
    context_params: dict[str, str] | None = None,
    error_config: MutationErrorConfig | None = None
)
class MutationName:
    input: InputType
    success: SuccessType
    failure: FailureType
