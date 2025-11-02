# Extracted from: docs/performance/cascade-invalidation.md
# Block number: 3
# When User changes
CASCADE: user:{id} → invalidate:
  - user:{id}:posts
  - post:* where author_id={id}
  - comment:* where author_id={id}

# When Post changes
CASCADE: post:{id} → invalidate:
  - post:{id}:comments
  - comment:* where post_id={id}
  - user:{author_id}:posts  # Parent relationship
