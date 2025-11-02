# Extracted from: docs/diagrams/apq-cache-flow.md
# Block number: 3
import redis.asyncio as redis


class RedisAPQCache:
    def __init__(self, redis_url):
        self.redis = redis.from_url(redis_url)

    async def get(self, key):
        return await self.redis.get(key)

    async def set(self, key, value, ttl=3600):
        await self.redis.setex(key, ttl, value)
