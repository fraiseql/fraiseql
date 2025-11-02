# Extracted from: docs/performance/caching.md
# Block number: 15
# Create a new user (mutation)
await cached_repo.execute_function("create_user", {"name": "Alice", "email": "alice@example.com"})

# Automatically invalidates:
# - fraiseql:{tenant_id}:user:*
# - fraiseql:{tenant_id}:users:*  (plural form)

# Next query fetches fresh data
users = await cached_repo.find("users")
# Cache miss → fetch from database → re-cache with new version
