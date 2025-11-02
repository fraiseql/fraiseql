# Extracted from: docs/tutorials/beginner-path.md
# Block number: 3
from fraiseql import query


# WRONG: No type hint
@query
async def users(info): ...


# CORRECT: Always specify return type
@query
async def users(info) -> list[User]: ...
