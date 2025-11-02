# Extracted from: docs/rust/RUST_PIPELINE_IMPLEMENTATION_GUIDE.md
# Block number: 17
from uuid import UUID

from fraiseql import field, query, type
from fraiseql.core.rust_pipeline import RustResponseBytes


@type
class User:
    id: UUID
    first_name: str
    last_name: str

    @field
    async def posts(self, info) -> RustResponseBytes:
        repo = info.context["repo"]
        return await repo.find_rust("v_post", "posts", info, user_id=self.id)


@query
async def users(info, limit: int = 20) -> RustResponseBytes:
    repo = info.context["repo"]
    return await repo.find_rust("v_user", "users", info, limit=limit)


@query
async def user(info, id: UUID) -> RustResponseBytes:
    repo = info.context["repo"]
    return await repo.find_one_rust("v_user", "user", info, id=id)
