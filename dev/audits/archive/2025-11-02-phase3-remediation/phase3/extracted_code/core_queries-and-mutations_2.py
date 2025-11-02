# Extracted from: docs/core/queries-and-mutations.md
# Block number: 2
from uuid import UUID

from fraiseql import query


@query
async def get_user(info, id: UUID) -> User:
    repo = info.context["repo"]
    # Returns RustResponseBytes - automatically processed by exclusive Rust pipeline
    return await repo.find_one_rust("v_user", "user", info, id=id)
