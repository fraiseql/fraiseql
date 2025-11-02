# Extracted from: docs/reference/decorators.md
# Block number: 19
@subscription
async def subscription_name(info, ...params) -> AsyncGenerator[ReturnType, None]:
    async for item in event_stream():
        yield item
