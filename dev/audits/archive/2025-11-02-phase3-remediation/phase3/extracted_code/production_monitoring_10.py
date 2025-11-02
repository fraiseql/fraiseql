# Extracted from: docs/production/monitoring.md
# Block number: 10
import redis.asyncio as redis

redis_client = redis.from_url("redis://localhost:6379")

await redis_client.set("key", "value", ex=3600)
value = await redis_client.get("key")
