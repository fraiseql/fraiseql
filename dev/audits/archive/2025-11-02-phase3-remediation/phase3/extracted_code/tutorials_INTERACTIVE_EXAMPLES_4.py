# Extracted from: docs/tutorials/INTERACTIVE_EXAMPLES.md
# Block number: 4
from fraiseql import input, mutation


@input
class CreatePostInput:
    title: str
    content: str
    author_id: UUID


@mutation
async def create_post(self, info, input: CreatePostInput) -> Post:
    # Call database function
    post_id = await db.execute_scalar(
        "SELECT fn_create_post($1, $2, $3)", [input.title, input.content, input.author_id]
    )

    # Return created post
    return await self.post(info, id=post_id)
