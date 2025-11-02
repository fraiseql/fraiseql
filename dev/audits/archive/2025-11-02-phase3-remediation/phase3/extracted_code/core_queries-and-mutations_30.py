# Extracted from: docs/core/queries-and-mutations.md
# Block number: 30
@subscription
async def on_private_messages(info) -> AsyncGenerator[Message, None]:
    user_context = info.context.get("user")
    if not user_context:
        raise GraphQLError("Authentication required")

    async for message in message_stream():
        # Only yield messages for authenticated user
        if message.recipient_id == user_context.user_id:
            yield message
