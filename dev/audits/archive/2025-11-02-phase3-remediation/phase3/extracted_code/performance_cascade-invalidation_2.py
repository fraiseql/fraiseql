# Extracted from: docs/performance/cascade-invalidation.md
# Block number: 2
# Setup CASCADE rules (once, at startup)
await setup_auto_cascade_rules(cache, schema, verbose=True)

# User changes
await update_user(user_id, new_name="Alice Smith")

# CASCADE automatically invalidates:
# - user:{user_id}
# - user:{user_id}:posts
# - post:* where author_id = user_id
# - Any other dependent caches
