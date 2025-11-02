# Extracted from: docs/core/queries-and-mutations.md
# Block number: 7
from fraiseql import type, query, mutation, input, field

@field(
    resolver: Callable[..., Any] | None = None,
    description: str | None = None,
    track_n1: bool = True
)
def method_name(self, info, ...params) -> ReturnType:
    pass
