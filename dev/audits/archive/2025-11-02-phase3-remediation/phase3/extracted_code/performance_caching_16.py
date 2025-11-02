# Extracted from: docs/performance/caching.md
# Block number: 16
from fraiseql.caching import CachedRepository

cached_repo = CachedRepository(base_repo, result_cache)

# All find() calls automatically cached
users = await cached_repo.find("users", status="active")
user = await cached_repo.find_one("users", id=user_id)

# Mutations automatically invalidate related cache
await cached_repo.execute_function("create_user", user_data)
