# Extracted from: docs/reference/database.md
# Block number: 21
async def run_in_transaction(
    func: Callable[..., Awaitable[T]],
    *args,
    **kwargs
) -> T
