# Extracted from: docs/diagrams/apq-cache-flow.md
# Block number: 5
# Time-based expiration
APQ_TTL = 24 * 60 * 60  # 24 hours

# Size-based eviction
MAX_CACHE_SIZE = 10000

# LRU eviction for memory cache
# Automatic expiration for Redis
