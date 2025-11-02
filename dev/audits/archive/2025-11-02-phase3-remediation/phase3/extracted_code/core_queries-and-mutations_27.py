# Extracted from: docs/core/queries-and-mutations.md
# Block number: 27
@subscription
async def subscription_name(info, ...params) -> AsyncGenerator[ReturnType, None]:
    async for item in event_stream():
        yield item
