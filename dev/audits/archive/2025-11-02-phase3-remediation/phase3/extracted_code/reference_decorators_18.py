# Extracted from: docs/reference/decorators.md
# Block number: 18
from fraiseql import dataloader_field
from fraiseql.optimization.dataloader import DataLoader


# Define DataLoader
class UserDataLoader(DataLoader):
    async def batch_load(self, keys: list[UUID]) -> list[User | None]:
        db = self.context["db"]
        users = await db.find("v_user", where={"id__in": keys})
        # Return in same order as keys
        user_map = {user.id: user for user in users}
        return [user_map.get(key) for key in keys]


# Use in type
@type
class Post:
    author_id: UUID

    @dataloader_field(UserDataLoader, key_field="author_id")
    async def author(self, info) -> User | None:
        """Load post author using DataLoader."""
        # Implementation is auto-generated


# GraphQL query automatically batches author loads
# query {
#   posts {
#     title
#     author { name }  # Batched into single query
#   }
# }
