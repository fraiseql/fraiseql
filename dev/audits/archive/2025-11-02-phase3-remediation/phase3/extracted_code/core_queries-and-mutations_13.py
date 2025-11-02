# Extracted from: docs/core/queries-and-mutations.md
# Block number: 13
from fraiseql import field, type


@type
class Post:
    id: UUID

    @field(description="Number of likes (cached)")
    async def like_count(self, info) -> int:
        cache = info.context.get("cache")
        cache_key = f"post:{self.id}:likes"

        # Try cache first
        if cache:
            cached_count = await cache.get(cache_key)
            if cached_count is not None:
                return int(cached_count)

        # Fallback to database
        repo = info.context["repo"]
        result = await repo.execute_raw("SELECT count(*) FROM likes WHERE post_id = $1", self.id)
        count = result[0]["count"]

        # Cache for 5 minutes
        if cache:
            await cache.set(cache_key, count, ttl=300)

        return count
