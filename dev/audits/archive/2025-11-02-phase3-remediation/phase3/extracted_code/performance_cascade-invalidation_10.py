# Extracted from: docs/performance/cascade-invalidation.md
# Block number: 10
# ✅ Good: 1-2 levels deep
User → Post → Comment  # 2 levels, reasonable

# ⚠️ Careful: 3+ levels deep
User → Post → Comment → Reply → Reaction  # 4 levels, may be expensive
