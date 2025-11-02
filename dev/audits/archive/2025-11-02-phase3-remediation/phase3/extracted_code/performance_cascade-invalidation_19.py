# Extracted from: docs/performance/cascade-invalidation.md
# Block number: 19
# Enable CASCADE logging
await cache.set_cascade_logging(enabled=True, level="DEBUG")

# Then monitor logs:
# [CASCADE] user:123 changed
# [CASCADE] → Evaluating rule: user -> post:author:{id}
# [CASCADE] → Matched 12 keys: post:author:123:*
# [CASCADE] → Invalidating: post:author:123:page:1
# [CASCADE] → Invalidating: post:author:123:page:2
# [CASCADE] → ... (10 more)
# [CASCADE] ✓ CASCADE complete in 5.2ms
