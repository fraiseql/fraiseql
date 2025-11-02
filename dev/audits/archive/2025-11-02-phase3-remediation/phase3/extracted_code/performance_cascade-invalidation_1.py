# Extracted from: docs/performance/cascade-invalidation.md
# Block number: 1
# User changes
await update_user(user_id, new_name="Alice Smith")

# But cached posts still show old user name!
posts = await cache.get(f"user:{user_id}:posts")
# Returns: Posts with "Alice Johnson" (stale!)
