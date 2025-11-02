# Extracted from: docs/production/monitoring.md
# Block number: 7
# Unlike in-memory cache, PostgreSQL cache is shared
# All app instances see the same cached data

# Instance 1
await cache.set("config:feature_flags", flags)

# Instance 2 (immediately available)
flags = await cache.get("config:feature_flags")
