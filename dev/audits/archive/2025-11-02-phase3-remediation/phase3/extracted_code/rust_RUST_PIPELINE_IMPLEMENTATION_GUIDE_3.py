# Extracted from: docs/rust/RUST_PIPELINE_IMPLEMENTATION_GUIDE.md
# Block number: 3
from fraiseql import field, type


@type
class User:
    id: UUID

    @field
    async def posts(self, info) -> RustResponseBytes:
        """Get user's posts."""
        repo = info.context["repo"]
        return await repo.find_rust("v_post", "posts", info, user_id=self.id)
