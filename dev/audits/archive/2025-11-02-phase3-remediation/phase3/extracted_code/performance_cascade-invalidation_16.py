# Extracted from: docs/performance/cascade-invalidation.md
# Block number: 16
# Only cascade if email changed (not password)
if old_user["email"] != new_user["email"]:
    await cache.invalidate(f"user:{user_id}")
    # Cascade: user's posts need new email

# If only password changed, no cascade needed
# (posts don't show password)
