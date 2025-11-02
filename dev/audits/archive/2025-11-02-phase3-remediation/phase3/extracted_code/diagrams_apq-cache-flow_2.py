# Extracted from: docs/diagrams/apq-cache-flow.md
# Block number: 2
from cachetools import TTLCache


class MemoryAPQCache:
    def __init__(self, max_size=1000, ttl=3600):
        self.cache = TTLCache(maxsize=max_size, ttl=ttl)

    async def get(self, key):
        return self.cache.get(key)

    async def set(self, key, value):
        self.cache[key] = value
